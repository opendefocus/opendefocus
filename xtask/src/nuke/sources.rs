use crate::TargetPlatform;
use anyhow::{Error, Result};
use async_compression::tokio::bufread::GzipDecoder;
use duct::cmd;
use futures_util::TryStreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use serde_json::Value;
use std::{
    fmt::Write,
    path::{Path, PathBuf},
};
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
            if let Ok(target) = get_target(&version, platform).await {
                targets.push(target)
            }
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
        for (i, target) in targets.into_iter().enumerate() {
            fetch_nuke_source(target, progressbars[i].clone()).await?;
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
    pub full_version: String,
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
            let retrieved_url = if let Some(retrieved_url) =
                minor_versions[full_version]["installer"][target_installer].as_str()
            {
                retrieved_url.to_string()
            } else {
                log::warn!("Skipping {version} for {platform} as it is not found.");
                continue;
            };

            let file_size = reqwest::get(&retrieved_url)
                .await?
                .content_length()
                .unwrap();

            return Ok(NukeTarget {
                version: version.to_string(),
                platform,
                package_size: file_size,
                url: Url::parse(&retrieved_url)?,
                full_version: full_version.to_owned(),
            });
        }
    }
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
            "tgz" => extract_tar(file, &installer_directory, &progressbar).await?,
            "zip" => extract_zip(file, &installer_directory, &progressbar).await?,
            "dmg" => {
                extract_dmg(
                    &target,
                    &compressed_installer,
                    &installer_directory,
                    &progressbar,
                )
                .await?
            }
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
    install_required_files(major, &installer, &sources_directory, &target, &progressbar).await?;
    tokio::fs::remove_dir_all(installer_directory).await?;
    progressbar.set_message("Patching headers...");
    patch_headers(&sources_directory).await?;
    progressbar.finish_with_message(format!("Finished collection for '{}'", &target.version));

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

async fn extract_tar(
    compressed_installer: File,
    installer_directory: &Path,
    progressbar: &ProgressBar,
) -> Result<PathBuf> {
    let buffer = BufReader::new(compressed_installer);
    progressbar.set_message("Decoding archive...");
    let decoder = GzipDecoder::new(buffer);
    let mut archive = Archive::new(decoder);
    progressbar.set_message("Reading archive...");
    archive.unpack(installer_directory).await?;
    progressbar.set_message("Archive extracted");
    let mut entries = tokio::fs::read_dir(installer_directory).await?;
    progressbar.set_message("Scanning archive for installer");

    while let Some(entry) = entries.next_entry().await? {
        let filename = entry.file_name().into_string().unwrap();
        let filepath = installer_directory.join(PathBuf::from(&filename));
        if let Some(extension) = filepath.extension() {
            if extension == "run" {
                return Ok(filepath);
            }
        }
        if filename.contains("installer")
            && !filename.ends_with("tgz")
            && !filename.ends_with("zip")
        {
            return Ok(filepath);
        }
    }

    Err(Error::msg("No installer found in tar"))
}

async fn extract_zip(
    compressed_installer: File,
    installer_directory: &Path,
    progressbar: &ProgressBar,
) -> Result<PathBuf> {
    let mut archive = ZipArchive::new(compressed_installer.try_into_std().unwrap())?;
    progressbar.set_message("Reading archive...");
    archive.extract(installer_directory)?;
    progressbar.set_message("Archive extracted");

    let mut entries = tokio::fs::read_dir(installer_directory).await?;
    progressbar.set_message("Scanning archive for installer");

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
    target: &NukeTarget,
    compressed_installer: &Path,
    installer_directory: &Path,
    progressbar: &ProgressBar,
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
    progressbar.set_message("Archive extracted...");

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
    let filepath = installer_directory
        .join(format!("Nuke-{}-mac-x86-64-installer", target.full_version))
        .join(format!("Nuke{}", target.full_version))
        .join(format!("Nuke{}.app", target.full_version))
        .join("Contents")
        .join("MacOS");
    if filepath.is_dir() {
        return Ok(filepath);
    };

    let legacy_path = installer_directory
        .join(format!("Nuke{}", target.full_version))
        .join(format!(
            "Nuke{}-mac-x86-release-64.pkg",
            target.full_version
        ))
        .join("Contents")
        .join("Archive.pax.gz");
    if legacy_path.is_file() {
        return Ok(legacy_path);
    }
    Err(Error::msg("No installer found in dmg"))
}

async fn install_required_files(
    major: usize,
    installer: &Path,
    target_filepath: &Path,
    target: &NukeTarget,
    progressbar: &ProgressBar,
) -> Result<()> {
    let install_path = target_filepath.join("extracted");
    tokio::fs::create_dir_all(&install_path).await?;
    match target.platform {
        TargetPlatform::Windows => install_windows(major, installer, &install_path).await?,
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
        _ => install_macos(major, target, installer, &install_path).await?,
    };
    progressbar.set_message("Installed to extraction location, cleaning files...");
    keep_required_files(&install_path, target_filepath, target.platform).await?;
    progressbar.set_message("Cleanup done, removing installer...");
    tokio::fs::remove_dir_all(install_path).await?;
    Ok(())
}

async fn install_windows(major: usize, installer: &Path, install_path: &Path) -> Result<(), Error> {
    if install_path.is_dir() {
        tokio::fs::remove_dir_all(&install_path).await?;
    }
    if major < 14 {
        let _ = cmd!(
            "7z",
            "x",
            installer.file_name().unwrap(),
            "-aoa",
            "-oextracted"
        )
        .dir(installer.parent().unwrap())
        .stdout_null()
        .run()?;
        tokio::fs::rename(installer.parent().unwrap().join("extracted"), install_path).await?;
    } else {
        tokio::fs::create_dir_all(&install_path).await?;
        let installer_name = installer
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split_once("-")
            .unwrap()
            .0;
        #[cfg(not(target_os = "windows"))]
        {
            let install_directory = installer.parent().unwrap().join(installer_name);
            cmd!("msiextract", installer, "-C", installer.parent().unwrap())
                .stdout_null()
                .run()?;
            tokio::fs::rename(install_directory, install_path).await?;
        }
        #[cfg(target_os = "windows")]
        {
            if installer.parent().unwrap().join("extract").is_dir() {
                tokio::fs::remove_dir_all(installer.parent().unwrap().join("extract")).await?;
            };
            let install_directory = installer
                .parent()
                .unwrap()
                .join("extract")
                .join("SourceDir")
                .join(installer_name);
            cmd!("lessmsi", "x", installer.file_name().unwrap(), r"extract\")
                .stdout_null()
                .dir(installer.parent().unwrap())
                .run()?;
            tokio::fs::rename(install_directory, install_path).await?;
        }
    };
    Ok(())
}

async fn install_macos(
    major: usize,
    target: &NukeTarget,
    installer: &Path,
    install_path: &Path,
) -> Result<(), Error> {
    if major < 12 {
        let work_dir = installer.parent().unwrap();

        let _ = cmd!("7z", "x", installer)
            .stdout_null()
            .dir(work_dir)
            .run()?;

        let _ = cmd!("7z", "x", "Archive.pax")
            .dir(work_dir)
            .stdout_null()
            .run()?;

        tokio::fs::rename(
            installer
                .parent()
                .unwrap()
                .join(format!("Nuke{}", target.full_version))
                .join(format!("Nuke{}.app", target.full_version))
                .join("Contents")
                .join("MacOS"),
            install_path,
        )
        .await?;

        return Ok(());
    }
    tokio::fs::rename(installer, install_path).await?;
    Ok(())
}

async fn install_linux(major: usize, file: File, install_path: &Path) -> Result<(), Error> {
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
    installation_path: &Path,
    target_filepath: &Path,
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

    if platform == TargetPlatform::Windows {
        tokio::fs::rename(
            installation_path.join("DDImage.lib"),
            target_filepath.join("DDImage.lib"),
        )
        .await?;
    }
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

async fn patch_headers(directory: &Path) -> Result<()> {
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
