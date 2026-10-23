//! Quality gates for the float EXR fixtures. A run renders each manifest case through the
//! adaptive CLI, and optionally the pinned legacy CLI, checks the analytic and lens gates,
//! and writes the summary.
mod args;
mod cache;
mod case;
mod contact;
mod context;
mod exr;
mod fields;
mod format;
mod gates;
mod image;
mod lens;
mod lens_checks;
mod manifest;
mod matrix;
mod metrics;
mod motion;
mod motion_gates;
mod output;
mod preview;
mod process;
mod provenance;
mod render;
mod settings;
mod summary;

use anyhow::{Result, bail};

pub use args::Args;

/// Runs one QA pass: checks the gates, prepares the output directory, renders the selected
/// cases as a static matrix or as one temporal sequence, and writes the summary. Fails when
/// the output is invalid, more than one temporal sequence is selected, or a render or gate
/// step fails.
pub fn run(args: Args) -> Result<()> {
    gates::selfcheck::run()?;
    let output = output::prepare(&args.output_dir)?;
    let context = context::Context::new(args, output)?;
    let (manifest_path, manifest) = manifest::load(&context.args.manifest)?;
    let cached_rows = match &context.args.cached_reference_dir {
        Some(directory) => Some(cache::load_rows(directory)?),
        None => None,
    };
    if context.temporal_mode_count() > 1 {
        bail!("Choose one temporal sequence per run");
    }
    let selection = manifest::select(&context.args, &manifest, &manifest_path)?;
    if context.temporal_mode_count() > 0 {
        let frames = motion::run(&context, &selection.cases)?;
        summary::write_motion(&context, &manifest_path, &manifest, &selection, &frames)
    } else {
        let run = matrix::run(&context, &selection.cases, cached_rows.as_ref())?;
        summary::write_matrix(&context, &manifest_path, &manifest, &selection, run)
    }
}
