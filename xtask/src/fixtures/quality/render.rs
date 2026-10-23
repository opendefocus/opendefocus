//! Renders one analytic case through a CLI, then reads back the EXR result and writes its
//! preview and QA metadata.
use std::{fs, path::Path};

use anyhow::Result;
use serde_json::{Value, json};

use super::{
    context::Context,
    exr::read_exr,
    image::{Rgba, min_max},
    preview::write_preview,
    process::{run_checked, sha256_file},
    settings::overlay,
};

/// Renderer to run: the adaptive CLI or the pinned legacy CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    /// The adaptive renderer under test.
    Adaptive,
    /// The pinned legacy renderer, used as a reference.
    Legacy,
}

/// Everything needed to run one render: the depth mode and settings, the output label, the lens,
/// the renderer and the image size.
pub struct RenderSpec<'a> {
    /// Depth mode: `real_depth`, `inverse_depth` or `direct_coc`.
    pub mode: &'a str,
    /// Focal plane of the render.
    pub focal: f64,
    /// Size value passed to the renderer.
    pub size: f64,
    /// Maximum circle-of-confusion radius in pixels.
    pub max_size: f64,
    /// Label that names the output files of this render.
    pub label: String,
    /// Lens settings applied to the render, if any.
    pub lens: Option<&'a Value>,
    /// Renderer to use.
    pub algorithm: Algorithm,
    /// Image size as `(width, height)` in pixels.
    pub dimensions: (usize, usize),
    /// CoC formula code that overrides the mode default, if set.
    pub coc_math: Option<i64>,
}

/// A finished render: its report and its decoded RGBA image.
pub struct Rendered {
    /// Report JSON of the CLI, with a `qa` section added.
    pub report: Value,
    /// Decoded RGBA image.
    pub rgba: Rgba,
}

/// Render one analytic case through the CLI, then read back its EXR and write its preview.
pub fn run_render(
    ctx: &Context,
    image: &Path,
    depth: &Path,
    spec: &RenderSpec,
) -> Result<Rendered> {
    let output = &ctx.output;
    let (width, height) = spec.dimensions;
    let center = ctx
        .center_override()
        .unwrap_or((width as f64 / 2.0, height as f64 / 2.0));
    let config = output.join(format!("{}.settings.json", spec.label));
    let settings = overlay(
        spec.mode,
        spec.focal,
        spec.size,
        spec.max_size,
        spec.lens,
        center,
        spec.coc_math,
    );
    fs::write(&config, pretty(&settings)?)?;
    let image_out = output.join(format!("{}.exr", spec.label));
    let report_out = output.join(format!("{}.json", spec.label));
    let executable = match spec.algorithm {
        Algorithm::Legacy => ctx.legacy_cli().unwrap_or(&ctx.args.cli),
        Algorithm::Adaptive => &ctx.args.cli,
    };
    let mut arguments = vec![
        image.display().to_string(),
        "--output".to_owned(),
        image_out.display().to_string(),
        "--depth".to_owned(),
        depth.display().to_string(),
        "--settings".to_owned(),
        config.display().to_string(),
        "--quality".to_owned(),
        ctx.args.quality.name().to_owned(),
        "--repeat".to_owned(),
        ctx.args.repeat.to_string(),
        "--report".to_owned(),
        report_out.display().to_string(),
    ];
    if spec.algorithm == Algorithm::Legacy && ctx.legacy_algorithm {
        arguments.extend(["--algorithm".to_owned(), "legacy".to_owned()]);
    }
    run_checked(executable, &arguments)?;
    let mut report: Value = serde_json::from_str(&fs::read_to_string(&report_out)?)?;
    let channel_prefix = output.join(format!("{}.channel", spec.label));
    let rgba = read_exr(&ctx.args.exr_dump, &image_out, &channel_prefix)?;
    let preview = output.join(format!("{}.preview.png", spec.label));
    write_preview(&preview, &rgba)?;
    let (alpha_min, alpha_max) = channel_range(&rgba, 3);
    let (rgb_min, rgb_max) = rgb_range(&rgba);
    let qa = json!({
        "shape": [rgba.height, rgba.width, 4],
        "alpha_range": [alpha_min, alpha_max],
        "rgb_range": [rgb_min, rgb_max],
        "output_sha256": sha256_file(&image_out)?,
        "preview": preview.display().to_string(),
        "render_center": [center.0, center.1],
    });
    if let Value::Object(object) = &mut report {
        object.insert("qa".to_owned(), qa);
    }
    fs::write(&report_out, pretty(&report)?)?;
    Ok(Rendered { report, rgba })
}

/// Formats a JSON value with two-space indentation and a trailing newline.
fn pretty(value: &Value) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

/// Returns the minimum and maximum of one channel of `image`.
fn channel_range(image: &Rgba, channel: usize) -> (f64, f64) {
    let values: Vec<f32> = image.channel(channel).data;
    let (low, high) = min_max(&values);
    (f64::from(low), f64::from(high))
}

/// Returns the minimum and maximum of the red, green and blue values of `image`, ignoring
/// alpha.
fn rgb_range(image: &Rgba) -> (f64, f64) {
    let values: Vec<f32> = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| pixel[..3].iter().copied())
        .collect();
    let (low, high) = min_max(&values);
    (f64::from(low), f64::from(high))
}
