//! Reads the render settings for an action from the effect's parameters, including the connected Nuke
//! camera and the pixel scaling that a reduced render scale needs.

use super::{
    HostResult, HostSuites, OFX_PROP_TIME, OFX_STAT_FAILED, OfxError, OfxImageEffectHandle,
    OfxPropertySetHandle, OfxTime, Props, Settings, read_connected_camera,
    read_settings_with_camera, render_scale, stored_suites,
};

/// Reads the settings a Render or `GetRegionsOfInterest` call at `in_args` uses: the parameters at
/// the call's time, with a connected Nuke camera (if the host has one) in place of the plain
/// camera values.
///
/// # Safety
///
/// `effect` and `in_args` must be the handles the host passed to the current action, valid for
/// the call. Calls outside an action are not supported.
///
/// # Errors
///
/// Returns an error if the suites were not stored at Load or the host rejects a property or
/// parameter read.
pub unsafe fn effect_settings(
    effect: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
) -> HostResult<Settings> {
    if in_args.is_null() {
        return Err(OfxError(OFX_STAT_FAILED));
    }
    let suites = stored_suites()?;
    let in_props = Props::new(in_args, suites.property);
    let time = in_props.get_double(OFX_PROP_TIME, 0)?;
    // SAFETY: forwarded from this function's contract.
    unsafe { read_effect_settings(&suites, effect, &in_props, time) }
}

/// Reads the instance settings from the effect's parameters at `time`, and the connected camera.
pub(super) unsafe fn read_effect_settings(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
    in_props: &Props<'_>,
    time: OfxTime,
) -> HostResult<Settings> {
    let mut param_set = core::ptr::null_mut();
    // SAFETY: `effect` is the handle the host passed to this action; `param_set` is an out pointer.
    crate::suites::check(unsafe { (suites.image_effect.getParamSet)(effect, &raw mut param_set) })?;
    // SAFETY: `effect` is the instance handle of this action and `in_props` its in-args, both
    // valid for the call.
    let camera = unsafe { read_connected_camera(suites, effect, in_props, time) }?;
    // SAFETY: `param_set` was just returned by the host for this effect.
    let mut settings = unsafe { read_settings_with_camera(param_set, time, camera) }?;
    scale_pixel_settings(&mut settings, render_scale(in_props).0);
    Ok(settings)
}
/// Scales the settings that are measured in pixels, so a render at scale `s` shows the same
/// picture as the full-scale render, reduced by `s`. Only the circle of confusion sizes are in
/// pixels; the camera max size is too. Barndoor and bokeh positions are relative to the frame.
#[allow(
    clippy::float_cmp,
    reason = "1.0 is the exact unscaled sentinel; skipping the multiply keeps the output bit-identical"
)]
pub(super) fn scale_pixel_settings(settings: &mut Settings, scale: f64) {
    if scale == 1.0 {
        return;
    }
    let scale = scale as f32;
    let coc = &mut settings.defocus.circle_of_confusion;
    coc.size *= scale;
    coc.max_size *= scale;
    settings.defocus.camera_max_size *= scale;
}
