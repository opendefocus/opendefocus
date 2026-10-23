//! Settings validation for the adaptive renderer. Rejects unsupported or invalid combinations before any GPU work is recorded.
use crate::datamodel;
use crate::datamodel::{
    defocus::DefocusMode,
    render::{FilterMode, Quality},
};
use crate::error::Error;
use crate::error::Result;
use crate::{MAX_EFFECTIVE_RADIUS, adaptive_filter_resolution, is_bokeh_filter_preview};

/// Builds the rejection error for an unsupported setting.
fn reject<T>(reason: &'static str) -> Result<T> {
    Err(Error::AdaptiveSettingsUnsupported(reason))
}

/// Validate the supported adaptive settings before preparing GPU resources.
/// Input depth, frame dimensions, and hardware availability are validated separately.
///
/// # Errors
///
/// Returns [`Error::AdaptiveSettingsUnsupported`] when a setting is non-finite, out of range, or uses an unknown enum value.
#[cfg(feature = "wgpu")]
pub fn validate_adaptive_settings(settings: &datamodel::Settings) -> Result<()> {
    if is_bokeh_filter_preview(settings) {
        if !settings.bokeh.aspect_ratio.is_finite()
            || settings.bokeh.aspect_ratio < 0.0
            || settings.render.filter.resolution == 0
        {
            return Err(Error::AdaptiveSettingsUnsupported(
                "filter resolution must be positive and aspect ratio nonnegative and finite",
            ));
        }
        return Ok(());
    }
    let defocus = &settings.defocus;
    let coc = &defocus.circle_of_confusion;
    let Ok(mode @ (DefocusMode::Depth | DefocusMode::Twod)) =
        DefocusMode::try_from(defocus.defocus_mode)
    else {
        return reject("only Depth and 2D defocus modes are supported");
    };
    let is_2d = mode == DefocusMode::Twod;
    if datamodel::render::ResultMode::try_from(settings.render.result_mode).is_err() {
        return reject("unknown render result mode");
    }
    if FilterMode::try_from(settings.render.filter.mode).is_err() {
        return reject("unknown aperture/filter mode");
    }
    let quality_value = if settings.render.gui {
        settings.render.quality
    } else {
        settings.render.farm_quality
    };
    if Quality::try_from(quality_value).is_err() {
        return reject("unknown renderer quality");
    }
    if !coc.pixel_aspect.is_finite() || coc.pixel_aspect <= 0.0 {
        return reject("pixel and aperture aspect ratios must be positive and finite");
    }
    let filter_mode = FilterMode::try_from(settings.render.filter.mode).ok();
    if filter_mode == Some(FilterMode::Simple)
        && (settings.non_uniform.inverse_foreground || settings.non_uniform.axial_aberration.enable)
        && (!settings.bokeh.aspect_ratio.is_finite()
            || settings.bokeh.aspect_ratio < 0.0
            || settings.render.filter.resolution == 0
            || adaptive_filter_resolution(settings).min_element() == 0)
    {
        return reject("simple-filter phase resolution must be positive");
    }
    if filter_mode == Some(FilterMode::BokehCreator)
        && (!settings.bokeh.aspect_ratio.is_finite()
            || settings.bokeh.aspect_ratio < 0.0
            || settings.render.filter.resolution == 0)
    {
        return reject("filter resolution must be positive");
    }
    let effective_radius = if is_2d {
        defocus.get_size()
    } else {
        defocus.get_max_size()
    };
    if !coc.focal_plane.is_finite()
        || !coc.protect.is_finite()
        || coc.protect < 0.0
        || !effective_radius.is_finite()
        || effective_radius < 0.0
        || !defocus.size_multiplier.is_finite()
        || defocus.size_multiplier < 0.0
        || !defocus.proxy_scale.unwrap_or(1.0).is_finite()
        || defocus.proxy_scale.unwrap_or(1.0) <= 0.0
    {
        return reject("focus and effective radii must be finite and nonnegative");
    }
    // Beyond this the GPU work grows into minutes to hours; fail fast with a readable message.
    if effective_radius > MAX_EFFECTIVE_RADIUS {
        return reject(
            "effective defocus radius (max size x multiplier) is too large (limit 16384 px)",
        );
    }
    if (!is_2d && !defocus.use_direct_math && (!coc.size.is_finite() || coc.size < 0.0))
        || (is_2d && (!defocus.get_size().is_finite() || defocus.get_size() < 0.0))
    {
        return reject("CoC size must be finite and nonnegative");
    }
    if !is_2d && !defocus.use_direct_math {
        validate_camera_coc(defocus, coc)?;
    }
    if !settings.render.center.x.is_finite() || !settings.render.center.y.is_finite() {
        return reject("lens center must be finite");
    }
    validate_lens_effects(&settings.non_uniform)
}

/// Rejects an unknown CoC depth math or an invalid physical camera.
///
/// # Errors
/// Returns [`Error::AdaptiveSettingsUnsupported`] naming the rejected setting.
fn validate_camera_coc(
    defocus: &datamodel::defocus::Settings,
    coc: &datamodel::circle_of_confusion::Settings,
) -> Result<()> {
    if datamodel::circle_of_confusion::Math::try_from(coc.math).is_err() {
        return reject("unknown camera CoC depth math");
    }
    if let Some(camera) = &coc.camera_data {
        if !coc.focal_plane.is_finite() || coc.focal_plane <= 0.0 {
            return reject("focal plane must be positive when the physical camera is used");
        }
        if !camera.focal_length.is_finite()
            || camera.focal_length <= 0.0
            || !camera.f_stop.is_finite()
            || camera.f_stop <= 0.0
            || !camera.filmback.width.is_finite()
            || camera.filmback.width <= 0.0
            || !camera.filmback.height.is_finite()
            || camera.filmback.height <= 0.0
            || (!defocus.use_camera_focal
                && (camera.resolution.width == 0 || camera.resolution.height == 0))
        {
            return reject("physical camera dimensions and f-stop must be positive and finite");
        }
        if datamodel::circle_of_confusion::WorldUnit::try_from(camera.world_unit).is_err() {
            return reject("unknown physical camera world unit");
        }
    }
    Ok(())
}

/// Rejects lens effect parameters that are non-finite, negative or otherwise unusable.
///
/// # Errors
/// Returns [`Error::AdaptiveSettingsUnsupported`] naming the rejected setting.
fn validate_lens_effects(lens: &datamodel::non_uniform::Settings) -> Result<()> {
    if lens.catseye.enable
        && [lens.catseye.amount, lens.catseye.softness]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return reject("catseye parameters must be finite and nonnegative");
    }
    if lens.catseye.enable && (!lens.catseye.gamma.is_finite() || lens.catseye.gamma <= 0.0) {
        return reject("catseye gamma must be positive");
    }
    if lens.barndoors.enable
        && [
            lens.barndoors.amount,
            lens.barndoors.top,
            lens.barndoors.bottom,
            lens.barndoors.left,
            lens.barndoors.right,
        ]
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return reject("barndoor parameters must be finite and nonnegative");
    }
    if lens.barndoors.enable && (!lens.barndoors.gamma.is_finite() || lens.barndoors.gamma <= 0.0) {
        return reject("barndoor gamma must be positive");
    }
    if lens.astigmatism.enable
        && (!lens.astigmatism.amount.is_finite() || lens.astigmatism.amount < 0.0)
    {
        return reject("astigmatism amount must be finite and nonnegative");
    }
    if lens.astigmatism.enable
        && (!lens.astigmatism.gamma.is_finite() || lens.astigmatism.gamma <= 0.0)
    {
        return reject("astigmatism gamma must be positive");
    }
    if lens.axial_aberration.enable {
        if !lens.axial_aberration.amount.is_finite() || lens.axial_aberration.amount < 0.0 {
            return reject("axial aberration amount must be finite and nonnegative");
        }
        if !(0..=2).contains(&lens.axial_aberration.color_type) {
            return reject("unknown axial aberration color type");
        }
    }
    Ok(())
}
