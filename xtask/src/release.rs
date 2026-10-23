//! Release tasks: builds the OpenFX bundle, archives the Nuke package and uploads it with
//! its metadata to GitHub, and generates the documentation and downloads page.
use crate::consts::REPOSITORY;
use crate::{
    license::fetch_licenses,
    util::{crate_root, target_directory},
};
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use duct::cmd;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zip::{ZipWriter, write::SimpleFileOptions};
/// Archives the `package/` folder. When `target_archive_path` is given the archive is
/// written there and nothing is uploaded. Otherwise it is written to a versioned zip in the
/// temporary directory and uploaded to the GitHub release of this version, with
/// `metadata.json` uploaded last so the download page only sees complete packages. The
/// upload needs `NUKE_VERSIONS` and `GITHUB_TOKEN`. Fails when the license cannot be
/// written, the archive cannot be created, the release is missing, or an upload is
/// rejected.
pub async fn release_package(target_archive_path: Option<PathBuf>) -> Result<()> {
    let target_file = if let Some(target_path) = target_archive_path.clone() {
        target_path
    } else {
        let tmp_dir = std::env::temp_dir();
        if !tmp_dir.exists() {
            tokio::fs::create_dir_all(&tmp_dir).await?;
        }
        tmp_dir.join(format!(
            "opendefocus-nuke-{}.zip",
            env!("CARGO_PKG_VERSION")
        ))
    };
    let package_path = crate_root().join("package");
    fetch_licenses(package_path.join("license.md")).await?;
    create_archive(&target_file, &package_path).await?;
    let filename = target_file
        .file_name()
        .context("Archive path must name a file")?
        .to_str()
        .context("Archive filename must be UTF-8")?;
    if target_archive_path.is_some() {
        return Ok(());
    }
    let metadata = ReleaseData {
        changelog: None,
        date: None,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        nuke: NukeData::from_env(filename)?,
    };
    let client = github_client()?;
    let release: GithubRelease = client
        .get(format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/tags/v{}",
            env!("CARGO_PKG_VERSION")
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    // Publish metadata last so the download page only sees complete packages.
    let data = tokio::fs::read(&target_file).await?;
    upload_release_asset(&client, release.id, filename, "application/zip", data).await?;
    upload_release_asset(
        &client,
        release.id,
        "metadata.json",
        "application/json",
        serde_json::to_vec(&metadata)?,
    )
    .await?;
    Ok(())
}

/// Builds the `OpenFX` plugin for the host platform as `OpenDefocus.ofx.bundle` in the target
/// directory. The bundle layout follows the `OpenFX` packaging rules: the binary sits in
/// `Contents/<arch>/OpenDefocus.ofx`, and macOS bundles also carry `Contents/Info.plist`.
pub fn bundle_ofx() -> Result<PathBuf> {
    cmd(
        "cargo",
        [
            "build",
            "--release",
            "--locked",
            "--package",
            "opendefocus-ofx",
        ],
    )
    .dir(crate_root())
    .run()?;
    let built = target_directory().join("release").join(format!(
        "{}opendefocus_ofx{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let arch = ofx_arch_folder()?;
    let bundle = target_directory()
        .join("ofx")
        .join("OpenDefocus.ofx.bundle");
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle)
            .with_context(|| format!("Removing old bundle {}", bundle.display()))?;
    }
    let contents = bundle.join("Contents");
    let binary_dir = contents.join(arch);
    std::fs::create_dir_all(&binary_dir)?;
    std::fs::copy(&built, binary_dir.join("OpenDefocus.ofx"))
        .with_context(|| format!("Copying {}", built.display()))?;
    // Hosts look for `<plugin identifier>.png` in Resources to show the plugin icon.
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&resources)?;
    std::fs::copy(
        crate_root()
            .join("packaging")
            .join("ofx")
            .join("org.opendefocus.OpenDefocus.png"),
        resources.join("org.opendefocus.OpenDefocus.png"),
    )
    .context("Copying the plugin icon")?;
    if std::env::consts::OS == "macos" {
        std::fs::write(contents.join("Info.plist"), info_plist())?;
    }
    log::info!("OpenFX bundle written to {}", bundle.display());
    Ok(bundle)
}

/// Architecture folder inside `Contents` for the host, per <https://openfx.readthedocs.io>
/// (Packaging): `MacOS` for macOS universal binaries, `Linux-${uname -m}` on Linux, `Win64`
/// for 64-bit Windows.
fn ofx_arch_folder() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => Ok("MacOS"),
        ("linux", "x86_64") => Ok("Linux-x86-64"),
        ("linux", "aarch64") => Ok("Linux-aarch64"),
        ("windows", "x86_64") => Ok("Win64"),
        (os, arch) => bail!("No OpenFX bundle layout for {os}-{arch}"),
    }
}

/// `Info.plist` for the macOS bundle. The executable is the renamed cdylib.
fn info_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>OpenDefocus.ofx</string>
    <key>CFBundleIdentifier</key>
    <string>org.opendefocus.OpenDefocus</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>OpenDefocus</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
</dict>
</plist>
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

/// Builds the HTTP client for the GitHub REST API. It sends the API version header and,
/// when `GITHUB_TOKEN` is set, a bearer token marked as sensitive. Fails when the token
/// is not a valid header value.
fn github_client() -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "x-github-api-version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    Ok(reqwest::Client::builder()
        .user_agent("OpenDefocus xtask")
        .default_headers(headers)
        .build()?)
}

/// Subset of a GitHub release response: its id, notes and attached files.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    /// Release id used to build the asset upload URL.
    id: u64,
    /// Release notes as written on GitHub, if any.
    body: Option<String>,
    /// Files attached to the release.
    assets: Vec<GithubAsset>,
}

/// A file attached to a GitHub release.
#[derive(Debug, Deserialize)]
struct GithubAsset {
    /// File name of the asset.
    name: String,
    /// Public URL the asset can be downloaded from.
    browser_download_url: String,
    /// Upload time of the asset, used as the release date.
    created_at: DateTime<Utc>,
}

/// Metadata uploaded as `metadata.json` next to each Nuke archive. It describes one
/// published version and is read back to build the downloads page.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct ReleaseData {
    /// Plugin version without the `v` prefix.
    version: String,
    /// Nuke compatibility and the archive it applies to.
    nuke: NukeData,
    /// Release date, taken from the archive upload time when the downloads page is built.
    date: Option<DateTime<Utc>>,
    /// Release notes, taken from the GitHub release body when the downloads page is built.
    changelog: Option<String>,
}

/// Nuke versions supported by an archive, with the archive file name and download URL.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct NukeData {
    /// Supported Nuke versions, such as `16.0`.
    versions: Vec<f32>,
    /// File name of the archive asset on the release.
    filename: String,
    /// Download URL of the archive. It is `None` in the uploaded metadata and filled in from
    /// GitHub when the downloads page is built.
    url: Option<String>,
}

/// Renders the downloads page as Markdown: a table with one row per release, followed by
/// the changelog of each release.
/// Releases without a date, download URL or changelog are skipped; `get_releases` fills all
/// three before the page is rendered.
fn release_data_to_markdown(release_data: &[ReleaseData]) -> String {
    let mut table: Vec<String> =
        vec!["| Version | Date | Nuke Versions | Download | Changelog | Source |".to_owned()];
    table.push("| - | - | - | - | - | - |".to_owned());
    let mut changelogs: Vec<String> = Vec::new();
    for release in release_data {
        let (Some(date), Some(url), Some(changelog)) = (
            release.date,
            release.nuke.url.as_ref(),
            release.changelog.as_ref(),
        ) else {
            continue;
        };
        let versions: String = release
            .nuke
            .versions
            .iter()
            .rev()
            .map(|f| {
                let version = format!("{f}");
                let version = if version.contains('.') {
                    version
                } else {
                    format!("{version}.0")
                };
                format!("`{version}`")
            })
            .collect::<Vec<String>>()
            .join(", ");

        table.push(format!(
            "| v{} | {} | {} | {} | {} | {}",
            release.version,
            date.format("%d-%m-%Y"),
            versions,
            format_args!("[{}]({})", release.nuke.filename, url),
            format_args!("[Changelog](#v{})", release.version.replace('.', "")),
            format_args!(
                "[Source](https://github.com/{REPOSITORY}/tree/v{})",
                release.version
            )
        ));
        changelogs.push(format!("## v{}\n{}", release.version, changelog));
    }
    let table: String = table.iter().flat_map(|s| [s.as_str(), "\n"]).collect();
    let changelogs: String = changelogs.iter().flat_map(|s| [s.as_str(), "\n"]).collect();
    format!("{table}\n{changelogs}")
}

impl NukeData {
    /// Reads the comma-separated Nuke versions from `NUKE_VERSIONS` and builds the record for
    /// the archive `filename`. Fails when the variable is unset or the versions are invalid.
    pub fn from_env(filename: &str) -> Result<Self> {
        let versions = std::env::var("NUKE_VERSIONS")?;
        Self::from_versions(filename, &versions)
    }

    /// Parses comma-separated Nuke versions such as `15.0,16.0` for the archive `filename`.
    /// Each entry must be `major.minor` and at least one is required. The download URL is left
    /// empty.
    fn from_versions(filename: &str, versions: &str) -> Result<Self> {
        let versions: Vec<f32> = versions
            .split_terminator(',')
            .map(|value| {
                crate::parse_nuke_version(value.trim()).map_err(anyhow::Error::msg)?;
                Ok(value.trim().parse::<f32>()?)
            })
            .collect::<Result<_>>()?;
        ensure!(
            !versions.is_empty(),
            "NUKE_VERSIONS must contain at least one version"
        );
        Ok(Self {
            versions,
            url: None,
            filename: filename.to_owned(),
        })
    }
}

/// Uploads `data` as the asset `filename` of GitHub release `release_id`, authenticated
/// with `GITHUB_TOKEN`. Fails when the token is missing or GitHub rejects the upload.
async fn upload_release_asset(
    client: &reqwest::Client,
    release_id: u64,
    filename: &str,
    content_type: &str,
    data: Vec<u8>,
) -> Result<()> {
    client
        .post(release_asset_url(release_id, filename)?)
        .bearer_auth(std::env::var("GITHUB_TOKEN")?)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(data)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// Builds the upload URL for the asset `filename` of `release_id`, with the name
/// query-encoded.
fn release_asset_url(release_id: u64, filename: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(&format!(
        "https://uploads.github.com/repos/{REPOSITORY}/releases/{release_id}/assets"
    ))?;
    url.query_pairs_mut().append_pair("name", filename);
    Ok(url)
}

/// Writes a zip of every regular file under `package_path` to `target_path`. Git ignore
/// files, `__pycache__` folders and bytecode files are skipped, and entries are sorted with
/// forward slashes. The archive is written to a temporary file and only moved into place
/// on success, so an existing archive survives a failure. Fails when the package is
/// missing, the target lies inside the package, or a file cannot be read.
async fn create_archive(target_path: &Path, package_path: &Path) -> Result<(), anyhow::Error> {
    let package_path = package_path
        .canonicalize()
        .context("Resolving package directory")?;
    let parent = target_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .context("Resolving archive directory")?;
    ensure!(
        !parent.starts_with(&package_path),
        "Archive must be outside the package directory"
    );
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut archive = ZipWriter::new(temporary.as_file().try_clone()?);
    let mut entries = async_walkdir::WalkDir::new(&package_path);
    let mut files = Vec::new();
    while let Some(entry) = entries.next().await {
        let filepath = entry?.path();
        if filepath
            .file_name()
            .is_some_and(|name| name == ".gitignore")
            || filepath
                .components()
                .any(|part| part.as_os_str() == "__pycache__")
            || filepath
                .extension()
                .is_some_and(|extension| extension == "pyc" || extension == "pyo")
        {
            continue;
        }
        if tokio::fs::metadata(&filepath).await?.is_file() {
            files.push(filepath);
        }
    }
    files.sort();
    for file in files {
        let relative_path = file.strip_prefix(&package_path)?;
        let name = relative_path
            .iter()
            .map(|part| part.to_str().context("Package filenames must be UTF-8"))
            .collect::<Result<Vec<_>>>()?
            .join("/");
        archive.start_file(name, SimpleFileOptions::default())?;
        let mut input = std::fs::File::open(&file)
            .with_context(|| format!("Reading package file {}", file.display()))?;
        std::io::copy(&mut input, &mut archive)?;
    }
    archive.finish()?.sync_all()?;
    temporary
        .persist(target_path)
        .context("Saving completed release archive")?;
    Ok(())
}

/// Lists the published releases that have a `metadata.json` asset, with date, download URL
/// and changelog taken from GitHub. Walks every page of results and fails on any API error.
async fn get_releases() -> Result<Vec<ReleaseData>> {
    let client = github_client()?;
    let mut releases = Vec::new();
    for page in 1.. {
        let github_releases: Vec<GithubRelease> = client
            .get(format!(
                "https://api.github.com/repos/{REPOSITORY}/releases?per_page=100&page={page}"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if github_releases.is_empty() {
            break;
        }
        for release in github_releases {
            let Some(metadata) = release.assets.iter().find(|a| a.name == "metadata.json") else {
                continue;
            };
            let mut release_data: ReleaseData = client
                .get(&metadata.browser_download_url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let Some(nuke_asset) = release
                .assets
                .iter()
                .find(|a| a.name == release_data.nuke.filename)
            else {
                continue;
            };
            release_data.date = Some(nuke_asset.created_at);
            release_data.nuke.url = Some(nuke_asset.browser_download_url.clone());
            release_data.changelog = Some(release.body.unwrap_or_default());
            releases.push(release_data);
        }
    }
    Ok(releases)
}

/// Writes `docs/src/downloads.md` from the published releases.
async fn create_downloads_table() -> Result<()> {
    log::info!("Creating downloads markdown...");
    let releases = get_releases().await?;
    let markdown = release_data_to_markdown(&releases);
    tokio::fs::write(
        crate_root().join("docs").join("src").join("downloads.md"),
        markdown,
    )
    .await?;
    log::info!("Downloads markdown written");
    Ok(())
}

/// Builds the documentation: prepares the generated pages, then runs `mdbook build` on the
/// `docs` folder.
pub async fn release_docs() -> Result<()> {
    prepare_docs().await?;
    cmd!("mdbook", "build", crate_root().join("docs")).run()?;
    Ok(())
}

/// Generates the documentation pages that are not kept in Git: the license notice and the
/// downloads table.
pub async fn prepare_docs() -> Result<()> {
    fetch_licenses(crate_root().join("docs").join("src").join("licenses.md")).await?;
    create_downloads_table().await?;
    Ok(())
}

/// Tests for the release archive, metadata and downloads table.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the archive contains the package files and excludes bytecode and
    /// `.gitignore`.
    #[tokio::test]
    async fn test_archive_uses_supplied_package_and_excludes_bytecode() {
        use std::io::Read;
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("package");
        std::fs::create_dir_all(package.join("nested/__pycache__")).unwrap();
        for (name, contents) in [
            ("init.py", "plugin"),
            ("nested/data.txt", "data"),
            ("nested/__pycache__/init.cpython-39.pyc", "bytecode"),
            ("init.pyc", "bytecode"),
            ("init.pyo", "bytecode"),
            (".gitignore", "ignored"),
        ] {
            std::fs::write(package.join(name), contents).unwrap();
        }
        let target = directory.path().join("release.zip");
        create_archive(&target, &package).await.unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(target).unwrap()).unwrap();
        let mut names: Vec<_> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["init.py", "nested/data.txt"]);
        let mut contents = String::new();
        archive
            .by_name("nested/data.txt")
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "data");
    }

    /// Verifies that a missing package leaves an existing archive untouched.
    #[tokio::test]
    async fn test_missing_package_preserves_existing_archive() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("release.zip");
        std::fs::write(&target, "old archive").unwrap();
        assert!(
            create_archive(&target, &directory.path().join("missing"))
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"old archive");
    }

    /// Verifies that a target inside the package folder is rejected and the existing file is
    /// kept.
    #[tokio::test]
    async fn test_archive_inside_package_is_rejected_without_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("release.zip");
        std::fs::write(&target, "old archive").unwrap();
        assert!(create_archive(&target, directory.path()).await.is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"old archive");
    }

    /// Verifies that an unreadable package entry fails the archive and keeps the existing file.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_unreadable_package_entry_preserves_existing_archive() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("package");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("a-valid.txt"), "valid").unwrap();
        symlink(package.join("missing"), package.join("broken.txt")).unwrap();
        let target = directory.path().join("release.zip");
        std::fs::write(&target, "old archive").unwrap();
        assert!(create_archive(&target, &package).await.is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"old archive");
    }

    /// Verifies that archive entries are deflate-compressed and read back unchanged.
    #[test]
    fn test_release_zip_keeps_default_deflate_roundtrip() {
        use std::io::{Cursor, Read, Write};
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("plugin.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"OpenDefocus plugin").unwrap();
        let mut archive = zip::ZipArchive::new(writer.finish().unwrap()).unwrap();
        let mut file = archive.by_name("plugin.txt").unwrap();
        assert_eq!(file.compression(), zip::CompressionMethod::Deflated);
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "OpenDefocus plugin");
    }

    /// Verifies the OpenFX architecture folder and the macOS `Info.plist` keys.
    #[test]
    fn ofx_bundle_layout_matches_openfx_folders() {
        assert!(ofx_arch_folder().is_ok());
        let plist = info_plist();
        assert!(plist.contains("<string>OpenDefocus.ofx</string>"));
        assert!(plist.contains("<string>BNDL</string>"));
    }

    /// Verifies the upload URL and that the asset name is query-encoded.
    #[test]
    fn github_asset_upload_url() {
        let url = release_asset_url(123, "plugin & metadata.zip").unwrap();
        assert_eq!(url.host_str(), Some("uploads.github.com"));
        assert_eq!(
            url.path(),
            "/repos/opendefocus/opendefocus/releases/123/assets"
        );
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![("name".into(), "plugin & metadata.zip".into())]
        );
    }

    /// Verifies that a release response parses and renders into the downloads table.
    #[test]
    fn github_release_downloads() {
        let release: GithubRelease = serde_json::from_value(serde_json::json!({
            "id": 123,
            "body": "Release notes",
            "assets": [{
                "name": "opendefocus-nuke-0.1.10.zip",
                "browser_download_url": "https://github.com/opendefocus/opendefocus/releases/download/v0.1.10/opendefocus-nuke-0.1.10.zip",
                "created_at": "2026-10-08T12:00:00Z"
            }]
        }))
        .unwrap();
        assert_eq!(release.id, 123);
        let asset = &release.assets[0];
        let data = ReleaseData {
            version: "0.1.10".to_owned(),
            date: Some(asset.created_at),
            changelog: release.body,
            nuke: NukeData {
                versions: vec![15.0, 16.0],
                filename: asset.name.clone(),
                url: Some(asset.browser_download_url.clone()),
            },
        };
        let markdown = release_data_to_markdown(&[data]);
        assert!(
            markdown.contains("[Source](https://github.com/opendefocus/opendefocus/tree/v0.1.10)")
        );
        assert!(markdown.contains(&asset.browser_download_url));
        assert!(markdown.contains("08-10-2026"));
        assert!(markdown.contains("`16.0`, `15.0`"));
        assert!(markdown.contains("## v0.1.10\nRelease notes"));
        assert!(!markdown.contains("codeberg"));
    }

    /// Verifies that valid Nuke version lists parse and invalid ones are rejected.
    #[test]
    fn nuke_data_from_env() {
        let nuke_data = NukeData::from_versions("blabla", "15.0,15.1,15.2,16.0").unwrap();
        for invalid in ["", "invalid", "NaN", "16.0,wrong"] {
            assert!(NukeData::from_versions("blabla", invalid).is_err());
        }

        assert_eq!(
            NukeData {
                versions: vec![15.0, 15.1, 15.2, 16.0],
                url: None,
                filename: "blabla".to_owned()
            },
            nuke_data
        );
    }
}
