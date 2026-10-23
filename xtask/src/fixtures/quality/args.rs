//! Command-line arguments of `cargo xtask fixtures quality`: the CLIs and manifest to use,
//! the output directory, and the case, sequence and render selections.
use std::path::PathBuf;

/// Options of `cargo xtask fixtures quality`.
#[derive(clap::Args, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each bool is an independent command-line flag"
)]
pub struct Args {
    /// Unified adaptive CLI
    #[arg(long)]
    pub cli: PathBuf,
    /// Optional pinned legacy CLI for comparison
    #[arg(long)]
    pub legacy_cli: Option<PathBuf>,
    /// Git revision corresponding to the adaptive binary
    #[arg(long)]
    pub source_revision: Option<String>,
    /// EXR dumper that writes the RGBA channels as raw float files
    #[arg(long)]
    pub exr_dump: PathBuf,
    /// Fixture manifest written by `cargo xtask fixtures adaptive`
    #[arg(long)]
    pub manifest: PathBuf,
    /// New run directory outside the repo
    #[arg(long)]
    pub output_dir: PathBuf,
    /// Reuse verified Legacy EXR outputs from a prior QA run's summary.json
    #[arg(long)]
    pub cached_reference_dir: Option<PathBuf>,
    /// Limit to named case(s); may be repeated
    #[arg(long = "case")]
    pub case: Vec<String>,
    /// Run the synchronized moving silhouette sequence
    #[arg(long)]
    pub motion: bool,
    /// Run fractional HDR point motion across a 16px tile boundary
    #[arg(long)]
    pub motion_points: bool,
    /// Run fractional HDR point motion at radius 64 across a 16px tile boundary
    #[arg(long)]
    pub motion_large_points: bool,
    /// Run independent RGB point tracks with different CoC radii/signs
    #[arg(long)]
    pub motion_colored_grid: bool,
    /// Run a signed direct-CoC sweep through focus
    #[arg(long)]
    pub focus_ramp: bool,
    /// Check focused opaque surroundings occlude far-sample spread
    #[arg(long)]
    pub focus_occlusion: bool,
    /// Override physical-depth focal planes
    #[arg(long, num_args = 1..)]
    pub focal_planes: Option<Vec<f64>>,
    /// Override physical-depth size values
    #[arg(long, num_args = 1..)]
    pub sizes: Option<Vec<f64>>,
    /// Maximum circle-of-confusion radius
    #[arg(long, default_value_t = 102.0)]
    pub max_size: f64,
    /// Optional maximum-radius sweep
    #[arg(long, num_args = 1..)]
    pub max_sizes: Option<Vec<f64>>,
    /// Render quality passed to the CLI
    #[arg(long, value_enum, default_value_t = Quality::Accurate)]
    pub quality: Quality,
    /// Number of timed repeats passed to the CLI
    #[arg(long, default_value_t = 1)]
    pub repeat: u32,
    /// Run every static lens variant for the cases that request it
    #[arg(long)]
    pub lens_sweep: bool,
    /// Render each analytic output twice in separate CLI runs and compare float pixels
    #[arg(long)]
    pub determinism: bool,
    /// Override the render lens center; default is the image center
    #[arg(long, num_args = 2, value_names = ["X", "Y"])]
    pub center: Option<Vec<f64>>,
}

/// Render quality preset passed to the CLI.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// Quick preview-grade approximation of Accurate.
    Fast,
    /// Exact source-pixel integration.
    Accurate,
}

impl Quality {
    /// Lowercase name of the preset, as the CLI expects it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Accurate => "accurate",
        }
    }

    /// Numeric level the CLI stores in its settings, matching `render::Quality`.
    pub fn level(self) -> i64 {
        match self {
            Self::Fast => 0,
            Self::Accurate => 1,
        }
    }
}
