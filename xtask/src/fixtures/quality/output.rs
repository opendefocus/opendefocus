//! Creates the run output directory outside the repository, and turns labels into file names
//! that are safe on every platform.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};

use crate::util::crate_root;

/// Create an empty run directory outside the checkout and return its canonical path.
pub fn prepare(path: &Path) -> Result<PathBuf> {
    let repo = crate_root().canonicalize()?;
    let requested = std::path::absolute(path)?;
    if requested.starts_with(&repo) {
        bail!(
            "Run output must be outside the repository: {}",
            requested.display()
        );
    }
    if requested.exists() && fs::read_dir(&requested)?.next().is_some() {
        bail!(
            "Run output directory must be empty: {}",
            requested.display()
        );
    }
    fs::create_dir_all(&requested)?;
    Ok(requested.canonicalize()?)
}

/// Make a label safe for file names: runs of other characters become one dash.
pub fn safe_name(value: &str) -> String {
    let mut name = String::new();
    let mut in_gap = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            name.push(character);
            in_gap = false;
        } else if !in_gap {
            name.push('-');
            in_gap = true;
        }
    }
    let trimmed = name.trim_matches('-');
    if trimmed.is_empty() {
        "case".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Tests for file-name sanitising.
#[cfg(test)]
mod tests {
    use super::safe_name;

    /// Verifies that labels keep ASCII file-name characters, collapse other runs into one dash,
    /// and that a label with nothing left becomes `case`.
    #[test]
    fn test_safe_name_collapses_gaps_and_trims_dashes() {
        assert_eq!(safe_name("psf near/r8 (HDR)"), "psf-near-r8-HDR");
        assert_eq!(safe_name("__"), "__");
        assert_eq!(safe_name("!!"), "case");
        assert_eq!(safe_name("a é b"), "a-b");
    }
}
