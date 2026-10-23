//! Entry point for the `opendefocus-cli` binary: parses arguments, installs the
//! interrupt handler, and hands the parsed arguments to the render pipeline.

/// GPU adaptive classification and CoC helpers, available with the `gpu` feature.
#[cfg(feature = "gpu")]
mod adaptive;
/// Command-line argument definitions.
mod args;
/// Reading and writing images, depth maps and reports.
mod image_files;
/// Fixed preview conversion of a float result for PNG output.
mod preview;
/// Frame rendering, including striped rendering with padded bands.
mod render;
/// Full CLI workflow: validation, rendering, and report generation.
mod run;
/// Settings file loading and quality overrides.
mod settings;

use anyhow::Result;
use args::Args;
use clap::Parser;
use opendefocus::abort;
use run::run;

/// Parses the command line, marks the render as aborted on Ctrl-C, and runs it.
///
/// # Errors
///
/// Returns an error if the signal handler cannot be registered or the render fails.
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    // Register synchronously before loading or rendering; polling a ctrl_c future
    // in a spawned task can leave a startup window with the default signal action.
    #[cfg(unix)]
    let mut interrupts = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    #[cfg(windows)]
    let mut interrupts = tokio::signal::windows::ctrl_c()?;
    #[cfg(any(unix, windows))]
    tokio::spawn(async move {
        if interrupts.recv().await.is_some() {
            abort::set_aborted(true);
        }
    });
    run(args).await
}
