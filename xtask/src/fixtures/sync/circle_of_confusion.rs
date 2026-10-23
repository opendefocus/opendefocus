//! Checks that the packaged circle-of-confusion fixture matches the canonical one, and copies
//! the canonical file into the package when staging is requested.
use std::{fs, path::Path};

use anyhow::{Result, ensure};

/// Compares the canonical circle-of-confusion `cases.json` with the packaged copy. With `stage`
/// set, the canonical file is copied over the packaged one instead. Fails when the copies differ
/// without staging, or when a file cannot be read or copied.
pub fn sync(root: &Path, stage: bool) -> Result<()> {
    let canonical = root.join("test/images/circle-of-confusion/cases.json");
    let packaged = root.join("crates/circle-of-confusion/test/cases.json");
    if stage {
        fs::copy(&canonical, &packaged)?;
        return Ok(());
    }
    ensure!(
        fs::read(&packaged)? == fs::read(&canonical)?,
        "CoC fixture snapshot differs; run `cargo xtask fixtures sync --stage`"
    );
    Ok(())
}
