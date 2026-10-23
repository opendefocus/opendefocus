//! Packages the `bokeh-creator` crate and tests the extracted copy, so its fixtures are
//! verified outside the workspace.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use duct::cmd;
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio_tar::Archive;

use super::bokeh::Manifest;

/// Name of the crate that is packaged and tested.
const PACKAGE: &str = "bokeh-creator";

/// Packages `bokeh-creator`, extracts the `.crate` archive into a temporary directory,
/// verifies its fixture hashes and runs its tests with the `documented` and `serde`
/// features. `root` is the workspace root. Fails when any step fails.
pub fn test(root: &Path) -> Result<()> {
    cmd!("cargo", "package", "-p", PACKAGE, "--locked")
        .dir(root)
        .run()?;
    let metadata: serde_json::Value = serde_json::from_str(
        &cmd!("cargo", "metadata", "--format-version=1", "--no-deps")
            .dir(root)
            .read()?,
    )?;
    let version = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|package| package["name"] == PACKAGE)
        .and_then(|package| package["version"].as_str())
        .context("bokeh-creator is missing from cargo metadata")?;
    let target_directory = PathBuf::from(
        metadata["target_directory"]
            .as_str()
            .context("cargo metadata has no target_directory")?,
    );
    let directory_name = format!("{PACKAGE}-{version}");
    let archive = target_directory
        .join("package")
        .join(format!("{directory_name}.crate"));

    let temporary = tempfile::Builder::new()
        .prefix("bokeh-package-")
        .tempdir()?;
    let destination = temporary.path().canonicalize()?;
    tokio::task::block_in_place(|| Handle::current().block_on(unpack(&archive, &destination)))?;

    let standalone = destination.join(&directory_name);
    let manifest = Manifest::read(&standalone.join("fixtures/SHA256SUMS"))?;
    manifest.verify_files(&standalone.join("fixtures"), "Packaged")?;

    fs::remove_file(standalone.join("Cargo.lock")).or_else(ignore_missing)?;
    cmd!(
        "cargo",
        "test",
        "--release",
        "--features",
        "documented,serde"
    )
    .dir(&standalone)
    .run()?;
    println!("Standalone packaged-crate tests and reference hashes passed");
    Ok(())
}

/// Extracts the gzip-compressed package archive into `destination`. Only regular files and
/// directories are accepted, and each member must stay inside `destination`.
async fn unpack(archive: &Path, destination: &Path) -> Result<()> {
    let reader = tokio::io::BufReader::new(tokio::fs::File::open(archive).await?);
    let mut tar = Archive::new(async_compression::tokio::bufread::GzipDecoder::new(reader));
    let mut entries = tar.entries()?;
    while let Some(entry) = entries.next().await {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        let path = entry.path()?.into_owned();
        ensure!(
            entry_type.is_file() || entry_type.is_dir(),
            "Unexpected package member: {}",
            path.display()
        );
        if entry.unpack_in(destination).await?.is_none() {
            bail!(
                "Package member escapes the extraction root: {}",
                path.display()
            );
        }
    }
    Ok(())
}

/// Treats a `NotFound` error as success, so removing a file that is absent is not an error.
/// Other errors are returned unchanged.
fn ignore_missing(error: std::io::Error) -> std::io::Result<()> {
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error)
    }
}
