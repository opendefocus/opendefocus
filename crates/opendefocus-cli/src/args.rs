//! Command-line argument definitions for `opendefocus-cli`, built with clap.

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// How the frame is rendered: as one pass, or in horizontal stripes.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RenderMode {
    /// Render the whole frame in one call.
    Full,
    /// Render horizontal stripes with padding, to bound GPU memory use.
    Striped,
}

/// Quality preset selected with `--quality`.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum QualityArg {
    /// Sampled gather: fastest, approximate.
    Fast,
    /// Exact source-pixel integration: slowest, highest precision.
    Accurate,
}

/// Parsed command-line options for a render, including timing and report output.
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Render images with OpenDefocus and report reproducible timings"
)]
pub struct Args {
    /// Color input (PNG, JPEG, TIFF, EXR, HDR)
    #[arg(required_unless_present = "dump_settings")]
    pub input: Option<PathBuf>,
    /// Output format follows extension: EXR/TIFF f32 RGBA, HDR RGBE, PNG RGBA16, JPEG RGB8
    #[arg(short, long, required_unless_present = "dump_settings")]
    pub output: Option<PathBuf>,
    /// Optional GPU diagnostic tile-class overlay; does not affect rendering
    #[arg(long)]
    pub classifier_overlay: Option<PathBuf>,
    /// Depth input; red channel is used verbatim, with the same dimensions as color
    #[arg(long)]
    pub depth: Option<PathBuf>,
    /// Custom bokeh image, cropped to its bounds
    #[arg(long)]
    pub bokeh: Option<PathBuf>,
    /// 2D radius in pixels (or depth size parameter); overrides settings
    #[arg(long)]
    pub radius: Option<f32>,
    /// Overrides both interactive and farm quality; default is renderer farm quality
    #[arg(long, value_enum)]
    pub quality: Option<QualityArg>,
    /// JSON settings overlay on renderer defaults; unknown fields are errors
    #[arg(long)]
    pub settings: Option<PathBuf>,
    /// Print complete default/effective settings as JSON and exit
    #[arg(long)]
    pub dump_settings: bool,
    /// Single-pass full frame, or striped rendering with padded bands
    #[arg(long, value_enum, default_value = "full")]
    pub render_mode: RenderMode,
    /// Override renderer stripe height (only with --render-mode striped)
    #[arg(long)]
    pub stripe_height: Option<u32>,
    /// Repeat from the immutable source, reusing initialization; save the final frame
    #[arg(long, default_value = "1", value_parser = clap::value_parser!(u32).range(1..))]
    pub repeat: u32,
    /// Also save the stdout JSON timing/settings report to this file
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Save a fixed-checker, Reinhard/sRGB PNG preview of the final float result
    #[arg(long)]
    pub preview: Option<PathBuf>,
}
