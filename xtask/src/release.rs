use crate::consts::{EMAIL, REPOSITORY, USERNAME};
use crate::{license::fetch_licenses, util::crate_root};
use anyhow::Result;
use chrono::{DateTime, Utc};
use duct::cmd;
use futures_util::StreamExt;
use git2::{Repository, Signature};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use zip::{ZipWriter, write::SimpleFileOptions};
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
    if target_file.exists() {
        tokio::fs::remove_file(&target_file).await?;
    }

    let package_path = crate_root().join("package");
    fetch_licenses(package_path.join("license.md")).await?;
    create_archive(&target_file, &package_path).await?;
    let release_id = latest_release().await?;
    let filename = target_file.file_name().unwrap().to_str().unwrap();
    if target_archive_path.is_some() {
        return Ok(());
    };
    let metadata = ReleaseData {
        changelog: None,
        date: None,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        nuke: NukeData::from_env(filename)?,
    };
    upload_metadata_to_release(metadata, release_id).await?;
    upload_codeberg_release(&target_file, release_id).await?;
    trigger_docs_release().await?;
    Ok(())
}

async fn trigger_docs_release() -> Result<()> {
    let client = reqwest::Client::builder().build()?;
    let response = client
        .post(format!("https://ci.codeberg.org/api/repos/15835/pipelines"))
        .bearer_auth(std::env::var("WOODPECKER_ACCESS_TOKEN")?)
        .json(&json!({"branch": "main"}))
        .send()
        .await?
        .error_for_status()?;
    println!("{}", response.text().await?);
    Ok(())
}

async fn latest_release() -> Result<usize> {
    let client = reqwest::Client::builder()
        .user_agent("OpenDefocus xtask")
        .build()?;
    let response: Value = client
        .get(format!(
            "https://codeberg.org/api/v1/repos/{REPOSITORY}/releases/latest"
        ))
        .send()
        .await?
        .json()
        .await?;
    Ok(response["id"].as_u64().unwrap() as usize)
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct ReleaseData {
    version: String,
    nuke: NukeData,
    date: Option<DateTime<Utc>>,
    changelog: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct NukeData {
    versions: Vec<f32>,
    filename: String,
    url: Option<String>,
}

fn release_data_to_markdown(release_data: &[ReleaseData]) -> String {
    let mut table: Vec<String> =
        vec!["| Version | Date | Nuke Versions | Download | Changelog | Source |".to_owned()];
    table.push("| - | - | - | - | - | - |".to_owned());
    let mut changelogs: Vec<String> = Vec::new();
    for release in release_data {
        let versions: String = release
            .nuke
            .versions
            .iter()
            .map(|f| {
                let version = format!("{f}");
                let version = if version.contains('.') {
                    version
                } else {
                    format!("{}.0", version)
                };
                format!("`{version}`")
            })
            .collect::<Vec<String>>()
            .join(", ");

        table.push(format!(
            "| v{} | {} | {} | {} | {} | {}",
            release.version,
            release.date.unwrap().format("%d-%m-%Y"),
            versions,
            format!(
                "[{}]({})",
                release.nuke.filename,
                release.nuke.url.as_ref().unwrap()
            ),
            format!("[Changelog](#v{})", release.version.replace(".", "")),
            format!(
                "[Source](https://codeberg.org/{REPOSITORY}/src/tag/v{})",
                release.version
            )
        ));
        changelogs.push(format!(
            "{}\n{}",
            format!("## v{}", release.version),
            release.changelog.as_ref().unwrap().to_owned()
        ));
    }
    let table: String = table.iter().map(|s| format!("{}\n", s)).collect();
    let changelogs: String = changelogs.iter().map(|s| format!("{}\n", s)).collect();
    format!("{table}\n{changelogs}")
}

impl NukeData {
    pub fn from_env(filename: &str) -> Result<Self> {
        let versions = std::env::var("NUKE_VERSIONS")?;
        let versions: Vec<f32> = versions
            .split_terminator(",")
            .map(|f| f.parse::<f32>().unwrap())
            .collect();
        Ok(Self {
            versions,
            url: None,
            filename: filename.to_owned(),
        })
    }
}

async fn upload_metadata_to_release(metadata: ReleaseData, release_id: usize) -> Result<()> {
    let client = reqwest::Client::builder().build()?;
    client.post(
        format!("https://codeberg.org/api/v1/repos/{REPOSITORY}/releases/{release_id}/assets?name=metadata.json")
    )
        .bearer_auth(std::env::var("CODEBERG_RELEASE_TOKEN")?)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&metadata)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
async fn upload_codeberg_release(release_zip: &Path, release_id: usize) -> Result<()> {
    let client = reqwest::Client::builder().build()?;
    let filename = release_zip.file_name().unwrap().to_str().unwrap();
    let mut data = Vec::new();
    File::open(release_zip)
        .await?
        .read_to_end(&mut data)
        .await?;
    client.post(
        format!("https://codeberg.org/api/v1/repos/{REPOSITORY}/releases/{release_id}/assets?name={filename}")
    )
        .bearer_auth(std::env::var("CODEBERG_RELEASE_TOKEN")?)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(data)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

async fn create_archive(target_path: &Path, package_path: &Path) -> Result<(), anyhow::Error> {
    let file = tokio::fs::File::create(&target_path).await?;
    let mut archive = ZipWriter::new(file.try_into_std().unwrap());
    let mut entries = async_walkdir::WalkDir::new(crate_root().join("package"));
    let mut files = Vec::new();
    loop {
        match entries.next().await {
            Some(Ok(entry)) => {
                let filepath = entry.path();
                if filepath.ends_with(".pyc") || filepath.ends_with(".gitignore") {
                    continue;
                }
                if filepath.is_file() {
                    files.push(filepath);
                }
            }
            Some(Err(e)) => {
                log::error!("error: {}", e);
                break;
            }
            None => break,
        }
    }

    for file in files {
        if let Ok(relative_path) = file.strip_prefix(&package_path) {
            let relative_str = relative_path.display().to_string();
            archive
                .start_file(relative_str, SimpleFileOptions::default())
                .unwrap();

            let mut f = std::fs::File::open(file).unwrap();
            let _ = std::io::copy(&mut f, &mut archive);
        }
    }
    archive.finish()?;
    Ok(())
}

async fn get_releases() -> Result<Vec<ReleaseData>> {
    let codeberg_releases: Value = reqwest::get(format!(
        "https://codeberg.org/api/v1/repos/{REPOSITORY}/releases"
    ))
    .await?
    .error_for_status()?
    .json()
    .await?;
    let mut releases = Vec::new();
    for release in codeberg_releases.as_array().unwrap() {
        for asset in release["assets"].as_array().unwrap() {
            if asset["name"] != "metadata.json" {
                continue;
            }
            let mut release_data: ReleaseData =
                reqwest::get(asset["browser_download_url"].as_str().unwrap())
                    .await?
                    .json()
                    .await?;

            let nuke_asset = if let Some(nuke_asset) = release["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"].as_str().unwrap().contains("nuke"))
            {
                nuke_asset
            } else {
                continue;
            };
            let nuke_url = nuke_asset["browser_download_url"]
                .as_str()
                .unwrap()
                .to_string();
            release_data.date =
                Some(DateTime::from_str(nuke_asset["created_at"].as_str().unwrap()).unwrap());
            release_data.nuke.url = Some(nuke_url);
            release_data.changelog = Some(release["body"].as_str().unwrap().to_string());
            releases.push(release_data);
        }
    }
    Ok(releases)
}

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

pub async fn release_docs() -> Result<()> {
    prepare_docs().await?;
    let docs_target = std::env::temp_dir().join("opendefocus_docs");
    if docs_target.exists() {
        tokio::fs::remove_dir_all(&docs_target).await?;
    }
    tokio::fs::create_dir_all(&docs_target).await?;

    let token = std::env::var("PUSH_TOKEN")?;
    let repo = Repository::clone(
        &format!("https://{token}@codeberg.org/opendefocus/pages.git"),
        &docs_target,
    )?;

    clean_directory(&docs_target).await?;
    tokio::fs::create_dir_all(&docs_target).await?;
    cmd!(
        "mdbook",
        "build",
        crate_root().join("docs"),
        "-d",
        &docs_target.join("site")
    )
    .run()?;
    move_contents(&docs_target.join("site"), &docs_target).await?;
    tokio::fs::remove_dir_all(&docs_target.join("site")).await?;

    add_and_commit(
        &repo,
        &format!(
            "Update documentation to latest release: '{}'",
            env!("CARGO_PKG_VERSION")
        ),
    )?;

    let mut remote = repo.find_remote("origin")?;
    remote.push(&["refs/heads/main:refs/heads/main"], None)?;

    Ok(())
}

pub async fn prepare_docs() -> Result<()> {
    fetch_licenses(crate_root().join("docs").join("src").join("licenses.md")).await?;
    create_downloads_table().await?;
    Ok(())
}

/// Get the latest commit in the repo
fn find_last_commit(repo: &'_ git2::Repository) -> Result<git2::Commit<'_>> {
    let obj = repo.head()?.resolve()?.peel(git2::ObjectType::Commit)?;
    obj.into_commit()
        .map_err(|_| anyhow::Error::msg("Couldn't find commit"))
}

/// Remove every non git related item from the repository.
async fn clean_directory(docs_target: &Path) -> Result<()> {
    let mut dir = tokio::fs::read_dir(docs_target).await?;

    while let Some(entry) = dir.next_entry().await? {
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        if filename != ".git"
            && filename != ".gitignore"
            && filename != "README.md"
            && filename != "LICENSE"
        {
            if entry.path().is_dir() {
                tokio::fs::remove_dir_all(entry.path()).await?;
            } else if entry.path().is_file() {
                tokio::fs::remove_file(entry.path()).await?;
            }
        }
    }

    Ok(())
}

fn add_and_commit(repo: &Repository, message: &str) -> Result<()> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;

    let oid = index.write_tree()?;
    let signature = Signature::now(USERNAME, EMAIL)?;
    let parent_commit = find_last_commit(&repo)?;
    let tree = repo.find_tree(oid)?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent_commit],
    )?;
    Ok(())
}

async fn move_contents(src: &Path, dst: &Path) -> Result<()> {
    let mut dir = tokio::fs::read_dir(src).await?;

    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name();
        tokio::fs::rename(src.join(&file_name), dst.join(&file_name)).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nuke_data_from_env() {
        unsafe {
            std::env::set_var("NUKE_VERSIONS", "15.0,15.1,15.2,16.0");
        }

        let nuke_data = NukeData::from_env("blabla").unwrap();

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
