//! Shared helpers for the xtask tasks: the repository root, the Cargo target directory
//! and the host-wide lock that serialises GPU renders and benchmarks.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Returns the repository root: the `xtask` manifest directory joined with `../`. The
/// path is not canonicalized.
pub fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../")
}

/// Returns the Cargo target directory that `build.rs` read from `cargo metadata` when
/// the binary was compiled.
pub fn target_directory() -> PathBuf {
    PathBuf::from(env!("TARGET_DIRECTORY"))
}

/// Takes the advisory lock that serialises GPU renders and benchmarks on this host. The
/// lock lives in `/tmp` on Unix and in the system temporary directory elsewhere. Keep the
/// returned file alive for the whole run; dropping it releases the lock.
/// Errors when the lock file cannot be opened or another run holds the lock.
pub fn validation_lock() -> Result<fs::File> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(if cfg!(unix) {
            PathBuf::from("/tmp/opendefocus-validation.lock")
        } else {
            std::env::temp_dir().join("opendefocus-validation.lock")
        })?;
    lock.try_lock()
        .context("Another OpenDefocus GPU or benchmark run is active; wait for it to finish")?;
    Ok(lock)
}

/// Returns the lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0xf])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// Returns the lowercase hexadecimal SHA-256 digest of the file at `path`. Fails when the file
/// cannot be read.
pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_hex(&fs::read(path)?))
}

/// Tests for the path helpers.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the digest of the empty input in lowercase hexadecimal.
    #[test]
    fn test_sha256_hex_of_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// Verifies that `target_directory` matches the directory reported by `cargo metadata` and
    /// is absolute.
    #[test]
    fn test_target_directory_matches_cargo_metadata() {
        let metadata = duct::cmd!(
            "cargo",
            "metadata",
            "--no-deps",
            "--format-version=1",
            "--locked"
        )
        .dir(crate_root())
        .read()
        .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert_eq!(
            target_directory(),
            PathBuf::from(metadata["target_directory"].as_str().unwrap())
        );
        assert!(target_directory().is_absolute());
    }
}
