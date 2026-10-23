//! Runs external render executables for the quality fixtures and re-exports the file digest used by the provenance records.
use std::{
    path::Path,
    process::{Command, Output},
};

use anyhow::{Result, bail};

pub use crate::util::sha256_file;

/// Run an executable to completion and return its captured output.
pub fn capture(executable: &Path, arguments: &[String]) -> Result<Output> {
    Ok(Command::new(executable).args(arguments).output()?)
}

/// Run an executable and fail with its stderr and stdout when it exits unsuccessfully.
pub fn run_checked(executable: &Path, arguments: &[String]) -> Result<Output> {
    let output = capture(executable, arguments)?;
    if !output.status.success() {
        bail!(
            "Render failed (exit {}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
    Ok(output)
}
