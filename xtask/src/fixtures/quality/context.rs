//! Shared state of one QA run: the parsed arguments, the output directory and the checks
//! that run before any render starts.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Result, bail};

use super::args::Args;

/// Shared state of one QA run: the parsed arguments, the output directory, and whether the
/// legacy CLI accepts `--algorithm`.
pub struct Context {
    /// Parsed command-line arguments.
    pub args: Args,
    /// Output directory of this run.
    pub output: PathBuf,
    /// Whether the legacy CLI accepts `--algorithm`, as its `--help` text reports.
    pub legacy_algorithm: bool,
}

impl Context {
    /// Checks that the CLI and EXR dumper exist and are executable, and that the legacy CLI, when
    /// given, exists too. Then records whether it supports `--algorithm`. Fails when an
    /// executable is missing or the legacy CLI cannot print its help.
    pub fn new(args: Args, output: PathBuf) -> Result<Self> {
        let mut executables = vec![args.cli.clone(), args.exr_dump.clone()];
        executables.extend(args.legacy_cli.clone());
        for executable in &executables {
            if !is_executable(executable) {
                bail!(
                    "Executable is missing/not executable: {}",
                    executable.display()
                );
            }
        }
        let legacy_algorithm = match &args.legacy_cli {
            Some(legacy) => {
                let help = Command::new(legacy).arg("--help").output()?;
                String::from_utf8_lossy(&help.stdout).contains("--algorithm")
            }
            None => false,
        };
        Ok(Self {
            args,
            output,
            legacy_algorithm,
        })
    }

    /// Path of the legacy CLI, when one was given.
    pub fn legacy_cli(&self) -> Option<&Path> {
        self.args.legacy_cli.as_deref()
    }

    /// The `--center` override as `(x, y)`, when one was given.
    pub fn center_override(&self) -> Option<(f64, f64)> {
        self.args
            .center
            .as_ref()
            .map(|center| (center[0], center[1]))
    }

    /// Number of temporal sequence flags that are set. A run may use at most one.
    pub fn temporal_mode_count(&self) -> usize {
        [
            self.args.motion,
            self.args.motion_points,
            self.args.motion_large_points,
            self.args.motion_colored_grid,
            self.args.focus_ramp,
            self.args.focus_occlusion,
        ]
        .into_iter()
        .filter(|enabled| *enabled)
        .count()
    }
}

/// Whether `path` is a regular file with an execute bit set.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

/// Whether `path` is a regular file. Execute bits do not exist on this platform.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
