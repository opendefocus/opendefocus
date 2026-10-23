//! Deterministic analytic scenes in `test/images/scenes`, written or verified byte for byte.

pub(super) mod analytic;
mod artifacts;
pub(super) mod exr;
pub(super) mod json;
mod png;

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::util::crate_root;

/// Options of `cargo xtask fixtures scenes`.
#[derive(clap::Args, Debug, Clone, Copy)]
pub struct Args {
    /// Regenerate in memory and compare every committed byte instead of writing
    #[arg(long)]
    check: bool,
}

/// Regenerates the committed scene assets, or with `--check` compares every committed file
/// with what would be generated, without writing anything. Fails on the first difference.
pub fn run(args: Args) -> Result<()> {
    let root = crate_root().join("test/images/scenes");
    let assets = artifacts::artifacts()?;
    let total_bytes: usize = assets.iter().map(|asset| asset.bytes.len()).sum();
    for asset in &assets {
        let path = root.join(&asset.path);
        if args.check {
            verify(&path, &asset.bytes)?;
        } else {
            write(&path, &asset.bytes)?;
        }
    }
    let verb = if args.check { "Validated" } else { "Generated" };
    println!(
        "{verb} {} deterministic assets ({total_bytes} bytes)",
        assets.len()
    );
    Ok(())
}

/// Checks that the file at `path` matches `expected`. PNG files are compared by decoded
/// pixels; every other file is compared byte for byte.
fn verify(path: &Path, expected: &[u8]) -> Result<()> {
    let actual = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let matches = if path.extension().is_some_and(|extension| extension == "png") {
        png::same_pixels(&actual, expected)?
    } else {
        actual == expected
    };
    if matches {
        Ok(())
    } else {
        bail!("Fixture differs: {}", path.display())
    }
}

/// Writes `bytes` to `path`, creating parent folders first.
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
