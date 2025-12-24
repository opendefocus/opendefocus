use crate::consts::REPOSITORY;
use crate::{license::fetch_licenses, util::crate_root};
use anyhow::Result;
use duct::cmd;
use futures_util::StreamExt;
use git2::{Repository, Signature};
use std::path::{Path, PathBuf};
use zip::{ZipWriter, write::SimpleFileOptions};
pub async fn release_package(target_archive_path: Option<PathBuf>) -> Result<()> {
    let target_file = if let Some(target_path) = target_archive_path {
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

    create_archive(&target_file, &package_path).await?;
    // upload_github_release(&target_file, "latest").await?;
    Ok(())
}

async fn upload_github_release(release_zip: &Path, release_id: &str) -> Result<()> {
    let client = reqwest::Client::builder().build()?;
    let filename = release_zip.file_name().unwrap().to_str().unwrap();
    client.post(
        format!("https://uploads.github.com/repos/{REPOSITORY}/releases/{release_id}/assets?name={filename}")
    )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .bearer_auth(std::env::var("GITHUB_RELEASE_TOKEN")?)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .body(tokio::fs::read(release_zip).await?)
        .send()
        .await?;

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

/// Documentation url

pub async fn release_docs() -> Result<()> {
    fetch_licenses(crate_root().join("docs").join("licenses.md")).await?;

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
    let username = cmd!("git", "config", "user.name").read()?;
    let email = cmd!("git", "config", "user.email").read()?;
    let signature = Signature::now(&username, &email)?;
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

/// Remove every non git related item from the repository.
async fn move_contents(src: &Path, dst: &Path) -> Result<()> {
    let mut dir = tokio::fs::read_dir(src).await?;

    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name();
        tokio::fs::rename(src.join(&file_name), dst.join(&file_name)).await?;
    }

    Ok(())
}
