//! Reads the parameter values from the host into render [`Settings`], and the defaults that the
//! same table provides.

use super::{
    CameraData, NukeCamera, OfxParamSetHandle, OfxPropertySetHandle, OfxTime, PARAMS, ParamKind,
    ParamValue, Result, Settings, c_string, check, param_get_bool, param_get_f64, param_get_i32,
    stored_suites,
};

/// Name of the camera toggle row. A connected Nuke camera forces it on (see
/// [`read_settings_with_camera`]).
pub(super) const CAMERA_TOGGLE: &str = "cameraEnabled";

/// The camera data, present only while the camera is enabled. Camera rows use this, so
/// they do nothing while `cameraEnabled` is off.
pub(super) fn camera_mut(settings: &mut Settings) -> Option<&mut CameraData> {
    settings.defocus.circle_of_confusion.camera_data.as_mut()
}

/// Returns `Settings::default()` with every table default applied.
#[must_use]
pub fn default_settings() -> Settings {
    let mut settings = Settings::default();
    for def in PARAMS {
        (def.apply)(&mut settings, def.default_value());
    }
    settings
}

/// Reads every parameter at `time` and builds the render settings, as the CLI does for a
/// render: `Settings::default()`, parameters applied on top, background (farm) quality, GPU.
///
/// The caller sets `settings.render.resolution` from the image size.
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load or the host rejects a parameter
/// lookup or read.
///
/// # Safety
///
/// `param_set` must be the parameter set of the effect instance, valid for the call.
pub unsafe fn read_settings(param_set: OfxParamSetHandle, time: OfxTime) -> Result<Settings> {
    // SAFETY: forwarded from this function's contract.
    unsafe { read_settings_with_camera(param_set, time, None) }
}

/// Like [`read_settings`], with an optional connected Nuke camera.
///
/// With `camera` set, the camera toggle is forced on and the camera data takes the focal length
/// and filmback from the camera. Focal length and filmback in the plain rows are then ignored;
/// f-stop and world unit still come from their rows. Without a camera the result is exactly
/// [`read_settings`].
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load or the host rejects a parameter
/// lookup or read.
///
/// # Panics
///
/// Panics if a name in the static parameter tables contains a NUL byte. The tables are fixed
/// at compile time and contain none.
///
/// # Safety
///
/// Same contract as [`read_settings`].
pub unsafe fn read_settings_with_camera(
    param_set: OfxParamSetHandle,
    time: OfxTime,
    camera: Option<NukeCamera>,
) -> Result<Settings> {
    let suites = stored_suites()?;
    let mut settings = Settings::default();
    settings.render.gui = false;
    settings.render.use_gpu_if_available = true;
    for def in PARAMS {
        let name = c_string(def.name)?;
        let mut param = core::ptr::null_mut();
        let mut props: OfxPropertySetHandle = core::ptr::null_mut();
        // SAFETY: `param_set` is valid per this function's contract; the name and out
        // pointers live for the call.
        check(unsafe {
            (suites.parameter.paramGetHandle)(
                param_set,
                name.as_ptr(),
                &raw mut param,
                &raw mut props,
            )
        })?;
        let value = match def.kind {
            _ if camera.is_some() && def.name == CAMERA_TOGGLE => ParamValue::Bool(true),
            ParamKind::Double { .. } => ParamValue::Double(param_get_f64(param, time)?),
            ParamKind::Int { .. } => ParamValue::Int(param_get_i32(param, time)?),
            ParamKind::Bool { .. } => ParamValue::Bool(param_get_bool(param, time)?),
            ParamKind::Choice { .. } => ParamValue::Choice(param_get_i32(param, time)?),
        };
        (def.apply)(&mut settings, value);
    }
    if let Some(camera) = camera {
        let data = settings
            .defocus
            .circle_of_confusion
            .camera_data
            .get_or_insert_with(CameraData::default);
        data.focal_length = camera.focal_length as f32;
        data.filmback.width = camera.haperture as f32;
        data.filmback.height = camera.vaperture as f32;
    }
    Ok(settings)
}
