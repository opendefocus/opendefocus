use crate::TargetPlatform;
use anyhow::{Error, Result};
use async_compression::tokio::bufread::GzipDecoder;
use duct::cmd;
use futures_util::TryStreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use serde_json::Value;
use std::{fmt::Write, path::PathBuf};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncWriteExt as _, BufReader},
};
use tokio_tar::Archive;
use url::Url;
use zip::ZipArchive;

pub async fn get_sources(
    platforms: Vec<TargetPlatform>,
    versions: Vec<String>,
    limit_threads: bool,
) -> Result<PathBuf> {
    log::info!(
        "Getting nuke sources for {:?} and versions: '{:?}'",
        platforms,
        versions
    );
    let mut targets = Vec::with_capacity(versions.len());
    for platform in platforms {
        for version in &versions {
            targets.push(get_target(&version, platform).await?)
        }
    }
    let progressbar = MultiProgress::new();
    let style = ProgressStyle::with_template("{spinner:.green} {msg} [{elapsed_precise}] [{wide_bar:.green/blue}] {decimal_bytes_per_sec} {bytes}/{total_bytes} ({eta})")
    ?.with_key("eta", |state: &ProgressState, w: &mut dyn Write| {
        let total_seconds = state.eta().as_secs();
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        write!(w, "{}m {:02}s", minutes, seconds).unwrap()
    })
    .progress_chars("#>-");

    let mut progressbars = Vec::with_capacity(targets.len());
    for target in &targets {
        let progressbar = progressbar.add(ProgressBar::new(target.package_size));
        progressbar.set_style(style.clone());
        progressbar.set_message(format!(
            "Starting download for Nuke {} ({:?})",
            target.version, target.platform
        ));
        progressbars.push(progressbar)
    }
    progressbar.println("Starting downloads... This can take a while.")?;
    let mut tasks = Vec::with_capacity(targets.len());
    if limit_threads {
        for task in tasks {
            task.await??;
        }
    } else {
        for (i, target) in targets.into_iter().enumerate() {
            tasks.push(tokio::spawn(fetch_nuke_source(
                target,
                progressbars[i].clone(),
            )));
        }
        let mut outputs = Vec::with_capacity(tasks.len());
        for task in tasks {
            outputs.push(task.await?);
        }
        for i in outputs {
            i?;
        }
    }

    Ok(sources_directory())
}

struct NukeTarget {
    pub platform: TargetPlatform,
    pub version: String,
    pub url: Url,
    pub package_size: u64,
}

async fn get_target(version: &str, platform: TargetPlatform) -> Result<NukeTarget> {
    let nuke_sources: Value = reqwest::get(Url::parse("https://raw.githubusercontent.com/olivebowl/NukeVersionParser/refs/heads/main/nuke-minor-releases.json")?)
    .await?
    .json()
    .await?;

    let major = version.split_once(".").unwrap().0;
    let target_installer = match platform {
        TargetPlatform::Linux => "linux_x86_64",
        TargetPlatform::Windows => "windows_x86_64",
        TargetPlatform::MacosAarch64 => "mac_arm",
        TargetPlatform::MacosX86_64 => "mac_x86_64",
    };
    let minor_versions = &nuke_sources[major];

    for full_version in minor_versions.as_object().unwrap().keys() {
        if full_version.starts_with(version) {
            let retrieved_url = minor_versions[full_version]["installer"][target_installer]
                .as_str()
                .unwrap()
                .to_string();

            let file_size = reqwest::get(&retrieved_url)
                .await?
                .content_length()
                .unwrap();

            return Ok(NukeTarget {
                version: version.to_string(),
                platform,
                package_size: file_size,
                url: Url::parse(&retrieved_url)?,
            });
        }
    }
    log::info!("Fetched url for version {version}");
    Err(Error::msg("Version not found"))
}

async fn fetch_nuke_source(target: NukeTarget, progressbar: ProgressBar) -> Result<()> {
    let sources_directory = nuke_source_directory(&target.version);
    let dll_prefix = dll_prefix(target.platform);
    let dll_suffix = dll_suffix(target.platform);
    if sources_directory
        .join(format!("{}DDImage.{}", dll_prefix, dll_suffix))
        .is_file()
    {
        log::info!(
            "Skipping collection for {} as it has already been collected.",
            target.version
        );
        return Ok(());
    }
    let installer_directory = sources_directory.join("installers");
    let filename = PathBuf::from(target.url.path_segments().unwrap().last().unwrap());
    let compressed_installer = installer_directory.join(&filename);
    let response = reqwest::get(target.url.clone()).await?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    tokio::fs::create_dir_all(&installer_directory).await?;
    let mut file = tokio::fs::File::create(&compressed_installer).await?;
    progressbar.set_message(format!("Downloading {:?}", filename));

    while let Some(chunk) = stream.try_next().await? {
        file.write_all(&chunk).await?;
        let new = downloaded + (chunk.len() as u64).min(target.package_size);
        downloaded = new;
        progressbar.set_position(new);
    }

    progressbar.set_message("Extracting required files...");
    let file = tokio::fs::File::open(&compressed_installer).await?;
    let installer = match compressed_installer.extension() {
        Some(extension) => match extension.to_str().unwrap() {
            "tgz" => extract_tar(file, &installer_directory).await?,
            "zip" => extract_zip(file, &installer_directory).await?,
            "dmg" => extract_dmg(&compressed_installer, &installer_directory).await?,
            _ => {
                return Err(Error::msg(
                    "Compressed installer does not have a valid extension",
                ));
            }
        },
        None => return Err(Error::msg("Compressed installer does not have a extension")),
    };
    tokio::fs::remove_file(&compressed_installer).await?;

    progressbar.set_message("Installing required files...");
    let major = target.version.split_once(".").unwrap().0;
    let major = major.parse::<usize>()?;
    install_required_files(major, &installer, &sources_directory, target.platform).await?;
    tokio::fs::remove_dir_all(installer_directory).await?;
    progressbar.finish_with_message(format!("Finished collection for '{}'", &target.version));
    patch_headers(&sources_directory).await?;

    Ok(())
}

pub fn dll_prefix(platform: TargetPlatform) -> String {
    match platform {
        TargetPlatform::Windows => "",
        _ => "lib",
    }
    .to_owned()
}

pub fn dll_suffix(platform: TargetPlatform) -> String {
    match platform {
        TargetPlatform::Windows => "dll",
        TargetPlatform::Linux => "so",
        _ => "dylib",
    }
    .to_owned()
}

async fn extract_tar(compressed_installer: File, installer_directory: &PathBuf) -> Result<PathBuf> {
    let buffer = BufReader::new(compressed_installer);
    let decoder = GzipDecoder::new(buffer);
    let mut archive = Archive::new(decoder);
    archive.unpack(installer_directory).await?;
    let mut entries = tokio::fs::read_dir(installer_directory).await?;

    while let Some(entry) = entries.next_entry().await? {
        let filename = entry.file_name().into_string().unwrap();
        let filepath = installer_directory.join(PathBuf::from(&filename));
        if let Some(extension) = filepath.extension() {
            if extension == "run" {
                return Ok(filepath);
            }
        }
        if filename.contains("installer") {
            return Ok(filepath);
        }
    }

    Err(Error::msg("No installer found in tar"))
}

async fn extract_zip(compressed_installer: File, installer_directory: &PathBuf) -> Result<PathBuf> {
    let mut archive = ZipArchive::new(compressed_installer.try_into_std().unwrap())?;
    archive.extract(installer_directory)?;

    let mut entries = tokio::fs::read_dir(installer_directory).await?;

    while let Some(entry) = entries.next_entry().await? {
        let filename = entry.file_name().into_string().unwrap();
        let filepath = installer_directory.join(PathBuf::from(&filename));
        if let Some(extension) = filepath.extension() {
            if extension == "exe" {
                return Ok(filepath);
            }
            if extension == "msi" {
                return Ok(filepath);
            }
        }
    }

    Err(Error::msg("No installer found in zip"))
}

async fn extract_dmg(
    compressed_installer: &PathBuf,
    installer_directory: &PathBuf,
) -> Result<PathBuf> {
    let _ = cmd!(
        "7z",
        "x",
        compressed_installer,
        "-aoa",
        format!("-o{}", installer_directory.to_str().unwrap())
    )
    .stdout_null()
    .run();
    let filename = compressed_installer
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .replace(".dmg", "");
    let version_name = filename.split_once("-").unwrap().0;
    let filepath = installer_directory
        .join(&filename)
        .join(version_name)
        .join(format!("{version_name}.app"))
        .join("Contents")
        .join("MacOS");
    if filepath.is_dir() {
        return Ok(filepath);
    }
    Err(Error::msg("No installer found in dmg"))
}

async fn install_required_files(
    major: usize,
    installer: &PathBuf,
    target_filepath: &PathBuf,
    platform: TargetPlatform,
) -> Result<()> {
    let file = tokio::fs::File::open(installer).await?;
    let install_path = target_filepath.join("extracted");
    tokio::fs::create_dir_all(&install_path).await?;
    match platform {
        TargetPlatform::Windows => install_windows(major, installer, file, &install_path).await?,
        TargetPlatform::Linux => {
            install_linux(
                major,
                tokio::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(installer)
                    .await?,
                &install_path,
            )
            .await?
        }
        _ => install_macos(installer, &install_path).await?,
    };
    keep_required_files(&install_path, target_filepath, platform).await?;
    tokio::fs::remove_dir_all(install_path).await?;
    Ok(())
}

async fn install_windows(
    major: usize,
    installer: &PathBuf,
    file: File,
    install_path: &PathBuf,
) -> Result<(), Error> {
    if install_path.is_dir() {
        tokio::fs::remove_dir_all(&install_path).await?;
    }
    if major < 14 {
        let mut archive = ZipArchive::new(file.try_into_std().unwrap())?;
        archive.extract(install_path)?;
    } else {
        let installer_name = installer
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split_once("-")
            .unwrap()
            .0;
        let install_directory = installer.parent().unwrap().join(installer_name);
        cmd!("msiextract", installer, "-C", installer.parent().unwrap())
            .stdout_null()
            .run()?;
        tokio::fs::rename(install_directory, install_path).await?;
    };
    Ok(())
}

async fn install_macos(installer: &PathBuf, install_path: &PathBuf) -> Result<(), Error> {
    tokio::fs::rename(installer, install_path).await?;
    Ok(())
}

async fn install_linux(major: usize, file: File, install_path: &PathBuf) -> Result<(), Error> {
    if major < 12 {
        let mut archive = ZipArchive::new(file.try_into_std().unwrap())?;
        archive.extract(install_path)?;
    } else {
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut continue_collection = true;
        while let Some(line) = lines.next_line().await?
            && continue_collection
        {
            // slight hacky way of skipping the installer directly to gzip part as this is much much much quicker
            if line.contains("#-----------------------------------------------------------;") {
                continue_collection = false;
            }
        }

        let decoder = GzipDecoder::new(lines.into_inner());
        let mut archive = Archive::new(decoder);
        archive.unpack(install_path).await?;
    };
    Ok(())
}

async fn keep_required_files(
    installation_path: &PathBuf,
    target_filepath: &PathBuf,
    platform: TargetPlatform,
) -> Result<()> {
    if !target_filepath.join("include").exists() {
        tokio::fs::rename(
            installation_path.join("include"),
            target_filepath.join("include"),
        )
        .await?;
    }

    let library = format!("{}DDImage.{}", dll_prefix(platform), dll_suffix(platform));

    tokio::fs::rename(
        installation_path.join(&library),
        target_filepath.join(&library),
    )
    .await?;

    Ok(())
}

fn sources_directory() -> PathBuf {
    PathBuf::from(env!("TARGET_DIRECTORY"))
        .join("nuke")
        .join("deps")
}

pub fn nuke_source_directory(version: &str) -> PathBuf {
    sources_directory().join(version)
}

async fn patch_headers(directory: &PathBuf) -> Result<()> {
    let allocator = directory
        .join("include")
        .join("DDImage")
        .join("STLAllocator.h");
    if allocator.is_file() {
        let header = tokio::fs::read_to_string(&allocator).await?;
        let header = header.replace(
            "STLInstanceClassName(const STLInstanceClassName& other)",
            "STLInstanceClassName(STLInstanceClassName& other)",
        );
        tokio::fs::write(allocator, header).await?;
    };
    Ok(())
}
