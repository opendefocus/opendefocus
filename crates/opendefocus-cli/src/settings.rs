//! Builds the effective render settings from defaults, a JSON overlay file, and
//! command-line overrides, and validates the result before rendering.

use crate::args::{Args, QualityArg, RenderMode};
use anyhow::{Context, Result, ensure};
use opendefocus::datamodel::{
    Settings,
    circle_of_confusion::CameraData,
    defocus::DefocusMode,
    render::{FilterMode, Quality, ResultMode},
};
use serde_json::Value;

/// Merges the JSON `patch` into `base`, recursively, and rejects keys that do not exist.
///
/// The merge reuses the serialized defaults as the schema instead of a second settings
/// model. Unknown keys are therefore errors, which also catches misspellings in nested
/// third-party settings structs. `path` names the current location for error messages.
///
/// # Errors
///
/// Returns an error if the patch names an unknown key or a non-object is merged into an
/// object.
pub fn overlay(base: &mut Value, patch: Value, path: &str) -> Result<()> {
    if let Value::Object(fields) = patch {
        if base.is_null() && path == "settings.defocus.circle_of_confusion.camera_data" {
            *base = serde_json::to_value(CameraData::default())?;
        }
        let object = base
            .as_object_mut()
            .with_context(|| format!("{path} is not an object"))?;
        for (key, value) in fields {
            let child = object
                .get_mut(&key)
                .with_context(|| format!("Unknown setting {path}.{key}"))?;
            overlay(child, value, &format!("{path}.{key}"))?;
        }
    } else {
        *base = patch;
    }
    Ok(())
}

/// Builds the effective settings for a CLI run.
///
/// Starts from the defaults, applies the `--settings` overlay if given, then applies the
/// radius, quality, depth, and bokeh overrides from `args`. GPU use is enabled and the GUI
/// is disabled. The result is validated before it is returned.
///
/// # Errors
///
/// Returns an error if the settings file cannot be read or is not a JSON object, the
/// overlay is invalid, a flag combination is not allowed, or validation fails.
pub fn settings(args: &Args) -> Result<Settings> {
    let mut value = serde_json::to_value(Settings::default())?;
    if let Some(path) = &args.settings {
        let patch: Value = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("Reading settings {}", path.display()))?,
        )?;
        ensure!(patch.is_object(), "Settings must be a JSON object");
        overlay(&mut value, patch, "settings")?;
    }
    let mut settings: Settings = serde_json::from_value(value).context("Invalid settings")?;
    // Background evaluation uses farm quality. Explicit quality sets both to avoid ambiguity.
    settings.render.gui = false;
    if let Some(radius) = args.radius {
        settings.defocus.circle_of_confusion.size = radius;
    }
    if let Some(quality) = args.quality {
        let quality = match quality {
            QualityArg::Fast => Quality::Fast,
            QualityArg::Accurate => Quality::Accurate,
        };
        settings.render.set_quality(quality);
        settings.render.set_farm_quality(quality);
    }
    if args.depth.is_some() {
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
    }
    if args.bokeh.is_some() {
        settings.render.filter.set_mode(FilterMode::Image);
    }
    settings.render.use_gpu_if_available = true;
    validate(&settings)?;
    ensure!(
        args.stripe_height != Some(0),
        "Stripe height must be positive"
    );
    ensure!(
        args.stripe_height.is_none() || matches!(args.render_mode, RenderMode::Striped),
        "--stripe-height requires --render-mode striped"
    );
    Ok(settings)
}

/// Checks that `settings` can be rendered, returning the first violated constraint.
///
/// Radii, gamma and aspect values must be finite and in range, enum values must be known,
/// the effective radius must fit the renderer's coordinate limits, and direct depth math
/// cannot be combined with a physical camera.
///
/// # Errors
///
/// Returns an error describing the first invalid field.
pub fn validate(settings: &Settings) -> Result<()> {
    let d = &settings.defocus;
    let c = &d.circle_of_confusion;
    for (name, value) in [
        ("radius", c.size),
        ("maximum radius", c.max_size),
        ("camera maximum radius", d.camera_max_size),
        ("size multiplier", d.size_multiplier),
        ("protect", c.protect),
    ] {
        ensure!(
            value.is_finite() && value >= 0.0,
            "{name} must be finite and nonnegative"
        );
    }
    ensure!(c.focal_plane.is_finite(), "Focal plane must be finite");
    for (name, value) in [
        ("pixel aspect", c.pixel_aspect),
        ("gamma correction", d.gamma_correction),
        ("proxy scale", d.proxy_scale.unwrap_or(1.0)),
    ] {
        ensure!(
            value.is_finite() && value > 0.0,
            "{name} must be positive and finite"
        );
    }
    ensure!(
        d.get_current_max_size().is_finite() && d.get_current_max_size() < (i32::MAX / 4) as f32,
        "Effective radius exceeds renderer coordinate limits"
    );
    ensure!(
        DefocusMode::try_from(d.defocus_mode).is_ok(),
        "Unknown defocus mode"
    );
    ensure!(
        Quality::try_from(settings.render.farm_quality).is_ok(),
        "Unknown farm quality"
    );
    ensure!(
        FilterMode::try_from(settings.render.filter.mode).is_ok(),
        "Unknown filter mode"
    );
    ensure!(
        ResultMode::try_from(settings.render.result_mode).is_ok(),
        "Unknown result mode"
    );
    ensure!(
        opendefocus::datamodel::bokeh_creator::FilterType::try_from(settings.bokeh.filter_type)
            .is_ok(),
        "Unknown bokeh shape"
    );
    ensure!(
        opendefocus::datamodel::non_uniform::AxialAberrationType::try_from(
            settings.non_uniform.axial_aberration.color_type
        )
        .is_ok(),
        "Unknown axial aberration type"
    );
    ensure!(
        settings.render.filter.resolution > 0,
        "Filter resolution must be positive"
    );
    ensure!(
        settings.bokeh.aspect_ratio.is_finite() && settings.bokeh.aspect_ratio >= 0.0,
        "Bokeh aspect ratio must be nonnegative and finite"
    );
    ensure!(
        settings.bokeh.blades >= 3,
        "Bokeh must have at least three blades"
    );
    if let Some(camera) = &c.camera_data {
        ensure!(
            !d.use_direct_math,
            "Direct depth and physical camera settings cannot be combined"
        );
        for (name, value) in [
            ("focal length", camera.focal_length),
            ("f-stop", camera.f_stop),
            ("filmback width", camera.filmback.width),
            ("filmback height", camera.filmback.height),
        ] {
            ensure!(
                value.is_finite() && value > 0.0,
                "Camera {name} must be positive and finite"
            );
        }
        ensure!(
            opendefocus::datamodel::circle_of_confusion::WorldUnit::try_from(camera.world_unit)
                .is_ok(),
            "Unknown camera world unit"
        );
    }
    ensure!(
        opendefocus::datamodel::circle_of_confusion::Math::try_from(c.math).is_ok(),
        "Unknown depth math"
    );
    Ok(())
}
