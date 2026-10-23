//! Command workflow for `opendefocus-cli`: validates arguments and paths, loads the inputs,
//! renders the requested frames, saves outputs, and prints the JSON report.

#[cfg(feature = "gpu")]
use crate::adaptive::{adaptive_coc, tile_summary};
use crate::{
    args::{Args, RenderMode},
    image_files::{load, output_format, resolved_path, save, save_report},
    preview::fixed_preview,
    render::{check_cancelled, render_frame},
    settings::settings,
};
use anyhow::{Context, Result, ensure};
use image::ImageFormat;
use ndarray::s;
use opendefocus::{
    OpenDefocusRenderer,
    datamodel::{UVector2, Vector2, defocus::DefocusMode, render::FilterMode},
};
use serde_json::json;
use std::{path::Path, time::Instant};

/// Runs the CLI without GPU support: prints settings for `--dump-settings`, otherwise fails.
///
/// # Errors
///
/// Returns an error when rendering is requested, because this build has no GPU backend.
#[cfg(not(feature = "gpu"))]
pub async fn run(args: Args) -> Result<()> {
    let settings = settings(&args)?;
    if args.dump_settings {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }
    anyhow::bail!("GPU unavailable: CLI was built without the gpu feature");
}

/// Runs one render from validated arguments: loads inputs, renders `--repeat` frames,
/// writes the output, optional preview and classifier overlay, and prints the JSON report.
///
/// Input, output, report, overlay and preview paths must all be distinct, and the source
/// files are never written to.
///
/// # Errors
///
/// Returns an error if settings are invalid, paths collide, inputs fail to load, rendering
/// is cancelled or fails, or any output cannot be saved.
#[cfg(feature = "gpu")]
pub async fn run(args: Args) -> Result<()> {
    let total = Instant::now();
    let mut settings = settings(&args)?;
    if args.dump_settings {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }
    if args.classifier_overlay.is_some() {
        ensure!(
            cfg!(feature = "gpu"),
            "Classifier diagnostics require a GPU-enabled build"
        );
        #[cfg(not(feature = "gpu"))]
        anyhow::bail!("Classifier diagnostics require a CLI build with the gpu feature");
    }
    let input = args.input.as_ref().context("Input is required")?;
    let output_path = args.output.as_ref().context("Output is required")?;
    validate_paths(&args, input, output_path)?;
    let format = output_format(output_path)?;
    let load_start = Instant::now();
    let source = load(input)?;
    let (height, width, _) = source.dim();
    let depth = args
        .depth
        .as_ref()
        .map(|path| -> Result<_> {
            let depth = load(path)?.slice(s![.., .., 0]).to_owned();
            ensure!(
                depth.dim() == (height, width),
                "Depth must have the same dimensions as color"
            );
            Ok(depth)
        })
        .transpose()?;
    let filter = args.bokeh.as_ref().map(|path| load(path)).transpose()?;
    ensure!(
        settings.defocus.defocus_mode() != DefocusMode::Depth || depth.is_some(),
        "Depth mode requires --depth"
    );
    ensure!(
        (settings.render.filter.preview
            && settings.render.filter.mode() == FilterMode::BokehCreator)
            || settings.render.filter.mode() != FilterMode::Image
            || filter.is_some(),
        "Image filter mode requires --bokeh"
    );
    settings.render.resolution = UVector2 {
        x: width as u32,
        y: height as u32,
    };
    // Lens effects are centred on the image unless the settings file names another centre.
    if settings.render.center == Vector2::default() {
        settings.render.center = Vector2 {
            x: width as f32 * 0.5,
            y: height as f32 * 0.5,
        };
    }
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let init_start = Instant::now();
    let renderer = OpenDefocusRenderer::new(&mut settings).await?;
    let initialization_ms = init_start.elapsed().as_secs_f64() * 1000.0;
    let stripe_height = match args.render_mode {
        RenderMode::Full => None,
        RenderMode::Striped => Some(
            args.stripe_height
                .unwrap_or_else(|| renderer.stripe_height(&settings)),
        ),
    };
    eprintln!(
        "Rendering {width}x{height} on {}",
        settings
            .render
            .device_name
            .as_deref()
            .unwrap_or("unknown device")
    );
    let mut frames = Vec::new();
    let mut last = None;
    #[cfg(feature = "gpu")]
    let (classifier_features, classifier_ms) = if args.classifier_overlay.is_some() {
        let coc = adaptive_coc(depth.as_ref(), &settings, (height, width))?;
        let start = Instant::now();
        let features = renderer
            .classify_adaptive_tiles(&source, &coc, [0, 0], settings.defocus.get_max_size())
            .await?;
        (Some(features), start.elapsed().as_secs_f64() * 1000.0)
    } else {
        (None, 0.0)
    };
    #[cfg(not(feature = "gpu"))]
    let classifier_ms = 0.0;
    for _ in 0..args.repeat {
        check_cancelled()?;
        let (output, wall, calls) = render_frame(
            &renderer,
            &settings,
            &source,
            depth.as_ref(),
            filter.as_ref(),
            stripe_height,
        )
        .await?;
        let frame_ms = wall.as_secs_f64() * 1000.0;
        frames.push(json!({"frame_ms": frame_ms,
            "renderer_calls_ms": calls.as_secs_f64() * 1000.0}));
        last = Some(output);
    }
    check_cancelled()?;
    #[cfg(feature = "gpu")]
    if let (Some(path), Some(features)) = (&args.classifier_overlay, &classifier_features) {
        let overlay = opendefocus::adaptive::classifier_overlay(
            &source,
            features,
            [width.div_ceil(16), height.div_ceil(16)],
            [0, 0],
        );
        save(path, ImageFormat::Png, overlay)?;
    }
    let save_start = Instant::now();
    let final_output = last.as_ref().context("No rendered frame")?;
    if let Some(path) = &args.preview {
        save(path, ImageFormat::Png, fixed_preview(final_output))?;
    }
    save(output_path, format, last.context("No rendered frame")?)?;
    #[cfg(feature = "gpu")]
    let adaptive_summary = classifier_features
        .as_ref()
        .map(|features| tile_summary(features));
    #[cfg(not(feature = "gpu"))]
    let adaptive_summary: Option<serde_json::Value> = None;
    let report = json!({"version": env!("CARGO_PKG_VERSION"), "backend": "gpu",
        "algorithm": "adaptive",
        "rendered_algorithm": "adaptive",
        "adaptive_tile_summary": adaptive_summary,
        "classifier_ms": classifier_ms,
        "classifier_diagnostic": args.classifier_overlay.is_some(),
        "preview": args.preview,
        "device": settings.render.device_name, "input": input, "depth": args.depth, "bokeh": args.bokeh,
        "output": output_path, "width": width, "height": height, "stripe_height": stripe_height,
        "load_ms": load_ms, "initialization_ms": initialization_ms, "frames": frames,
        "save_ms": save_start.elapsed().as_secs_f64() * 1000.0,
        "end_to_end_ms": total.elapsed().as_secs_f64() * 1000.0, "settings": settings});
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = args.report {
        save_report(&path, &text)?;
    }
    println!("{text}");
    Ok(())
}

/// Fails with `message` when `first` and `second` resolve to the same file.
fn ensure_distinct(first: &Path, second: &Path, message: &str) -> Result<()> {
    ensure!(resolved_path(first)? != resolved_path(second)?, "{message}");
    Ok(())
}

/// Checks that the output, report, classifier overlay and preview paths are distinct from each other and
/// from the input files, and that overlay and preview are PNG.
///
/// # Errors
///
/// Returns an error naming the colliding paths or the unsupported format.
fn validate_paths(args: &Args, input: &Path, output: &Path) -> Result<()> {
    let protected: Vec<&Path> = [
        Some(input),
        args.depth.as_deref(),
        args.bokeh.as_deref(),
        args.settings.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let report = args.report.as_deref();
    for path in &protected {
        ensure_distinct(
            path,
            output,
            "Output must differ from input and settings paths",
        )?;
        if let Some(report) = report {
            ensure_distinct(
                path,
                report,
                "Report must differ from input and settings paths",
            )?;
        }
    }
    if let Some(report) = report {
        ensure_distinct(report, output, "Report must differ from output path")?;
    }
    if let Some(overlay) = args.classifier_overlay.as_deref() {
        for path in &protected {
            ensure_distinct(
                overlay,
                path,
                "Classifier overlay must differ from input and settings paths",
            )?;
        }
        ensure_distinct(
            overlay,
            output,
            "Classifier overlay must differ from output",
        )?;
        if let Some(report) = report {
            ensure_distinct(
                overlay,
                report,
                "Classifier overlay must differ from report",
            )?;
        }
        ensure!(
            output_format(overlay)? == ImageFormat::Png,
            "Classifier overlay must use PNG format"
        );
    }
    if let Some(preview) = args.preview.as_deref() {
        ensure!(
            output_format(preview)? == ImageFormat::Png,
            "Image preview must use PNG format"
        );
        for path in &protected {
            ensure_distinct(
                preview,
                path,
                "Image preview must differ from input and settings paths",
            )?;
        }
        ensure_distinct(preview, output, "Image preview must differ from output")?;
        if let Some(report) = report {
            ensure_distinct(preview, report, "Image preview must differ from report")?;
        }
        if let Some(overlay) = args.classifier_overlay.as_deref() {
            ensure_distinct(
                preview,
                overlay,
                "Image preview must differ from classifier overlay",
            )?;
        }
    }
    Ok(())
}
