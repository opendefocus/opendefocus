use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
/// Return crate root path of opendefocus repo
pub fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../")
        .to_path_buf()
}

/// Return target directory path
pub fn target_directory() -> PathBuf {
    PathBuf::from(env!("TARGET_DIRECTORY")).to_path_buf()
}

/// Just some simple wrapper to handle the unwrap with result
pub fn path_to_string(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .context("Could not convert path to String")?
        .to_string())
}
