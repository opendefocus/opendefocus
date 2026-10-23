//! Verifies and stages the `bokeh-creator` reference images against the SHA-256 manifest
//! kept with the canonical fixtures in `test/images/bokeh-creator`.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use super::sha256_file;

/// Number of reference images the manifest must list.
const REFERENCE_COUNT: usize = 11;

/// Parsed `SHA256SUMS` manifest with one entry per reference image.
pub struct Manifest {
    /// Pairs of SHA-256 digest and file name, in manifest order.
    entries: Vec<(String, String)>,
}

impl Manifest {
    /// Parses the manifest at `path`. Each line must be `digest  name` with two spaces between
    /// them. Fails when the file cannot be read, a line is malformed, or the number of entries
    /// or names differs from `REFERENCE_COUNT`.
    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("Could not read {}", path.display()))?;
        let entries = text
            .lines()
            .map(|line| {
                line.split_once("  ")
                    .map(|(digest, name)| (digest.to_owned(), name.to_owned()))
                    .with_context(|| format!("Malformed manifest line: {line}"))
            })
            .collect::<Result<Vec<_>>>()?;
        let names: BTreeSet<&str> = entries.iter().map(|(_, name)| name.as_str()).collect();
        ensure!(
            entries.len() == REFERENCE_COUNT && names.len() == REFERENCE_COUNT,
            "Expected all {REFERENCE_COUNT} upstream references"
        );
        Ok(Self { entries })
    }

    /// Returns the set of file names listed in the manifest.
    fn names(&self) -> BTreeSet<String> {
        self.entries.iter().map(|(_, name)| name.clone()).collect()
    }

    /// Checks that `directory` holds exactly the listed JPEG files and that each one matches its
    /// digest. `label` names the set in error messages. Fails at the first mismatch.
    pub fn verify_files(&self, directory: &Path, label: &str) -> Result<()> {
        let jpgs = jpg_names(directory)?;
        ensure!(jpgs == self.names(), "{label} fixture set differs");
        for (digest, name) in &self.entries {
            ensure!(
                sha256_file(&directory.join(name))? == *digest,
                "{label} hash mismatch: {name}"
            );
        }
        Ok(())
    }
}

/// Returns the names of the `.jpg` files directly inside `directory`.
fn jpg_names(directory: &Path) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let path: PathBuf = entry?.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "jpg") {
            names.insert(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    Ok(names)
}

/// Verifies the canonical references. With `stage` set, copies them and the manifest into
/// `crates/bokeh-creator/fixtures` first. Then checks that the packaged manifest matches
/// and that the packaged files verify. Fails on any mismatch.
pub fn sync(root: &Path, stage: bool) -> Result<()> {
    let canonical = root.join("test/images/bokeh-creator");
    let packaged = root.join("crates/bokeh-creator/fixtures");
    let manifest_path = canonical.join("SHA256SUMS");
    let manifest = Manifest::read(&manifest_path)?;

    manifest.verify_files(&canonical, "Canonical")?;
    if stage {
        fs::create_dir_all(&packaged)?;
        for (_, name) in &manifest.entries {
            fs::copy(canonical.join(name), packaged.join(name))?;
        }
        fs::copy(&manifest_path, packaged.join("SHA256SUMS"))?;
    }

    ensure!(
        fs::read(packaged.join("SHA256SUMS"))? == fs::read(&manifest_path)?,
        "Staged manifest differs"
    );
    manifest.verify_files(&packaged, "Packaged")?;
    println!("Verified {REFERENCE_COUNT} canonical and packaged bokeh references");
    Ok(())
}
