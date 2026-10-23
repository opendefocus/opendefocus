//! Mock implementation of Nuke's camera suite, backed by a fixed [`MockCamera`] so tests can connect a camera with known focal length and filmback.

use super::*;

/// Values of the mock Nuke camera: whether it is connected, and the three parameters it reports.
#[derive(Clone, Copy, Debug)]
pub struct MockCamera {
    pub connected: bool,
    /// Focal length in millimetres.
    pub focal: f64,
    /// Horizontal aperture in millimetres.
    pub haperture: f64,
    /// Vertical aperture in millimetres.
    pub vaperture: f64,
}

/// The mock camera as the host keeps it: one handle and one property set.
pub(super) struct CameraRec {
    pub(super) handle: usize,
    pub(super) props: usize,
    pub(super) connected: bool,
    pub(super) focal: f64,
    pub(super) haperture: f64,
    pub(super) vaperture: f64,
}

/// `cameraDefine`: records the input name and returns the camera's property set.
pub(super) unsafe extern "C" fn camera_define(
    effect: OfxImageEffectHandle,
    name: *const c_char,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    if effect as usize != with(|s| s.effect) {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: the plugin passes a valid NUL-terminated input name.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    let props = with(|s| {
        if s.camera_inputs.contains(&name) {
            return Err(OFX_STAT_ERR_EXISTS);
        }
        let props = s
            .nuke_camera
            .as_ref()
            .map(|c| c.props)
            .ok_or(OFX_STAT_ERR_UNKNOWN)?;
        s.camera_inputs.push(name);
        Ok(props)
    });
    match props {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Ok(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        Err(status) => status,
    }
}

/// `cameraGetHandle`: the camera handle and its descriptor property set, for a defined input.
pub(super) unsafe extern "C" fn camera_get_handle(
    effect: OfxImageEffectHandle,
    name: *const c_char,
    camera_out: *mut NukeOfxCameraHandle,
    props_out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    if effect as usize != with(|s| s.effect) {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: the plugin passes a valid NUL-terminated input name.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    let found = with(|s| {
        if !s.camera_inputs.contains(&name) {
            return None;
        }
        s.nuke_camera.as_ref().map(|c| (c.handle, c.props))
    });
    let Some((handle, props)) = found else {
        return OFX_STAT_ERR_UNKNOWN;
    };
    // SAFETY: both out pointers are valid per the host contract.
    unsafe {
        let status = put(camera_out, handle as NukeOfxCameraHandle);
        if status != OFX_STAT_OK {
            return status;
        }
        put(props_out, props as OfxPropertySetHandle)
    }
}

/// `cameraGetPropertySet`: the property set of a camera handle, where the connection is reported.
pub(super) unsafe extern "C" fn camera_get_property_set(
    camera: NukeOfxCameraHandle,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let props = with(|s| {
        s.nuke_camera
            .as_ref()
            .filter(|c| c.handle == camera as usize)
            .map(|c| c.props)
    });
    match props {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Some(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

/// `cameraGetParameter`: one double for a connected camera. Not connected is `kOfxStatFailed`,
/// as the header documents. The view is accepted and ignored: the mock has one view.
pub(super) unsafe extern "C" fn camera_get_parameter(
    camera: NukeOfxCameraHandle,
    name: *const c_char,
    _time: f64,
    _view: c_int,
    out: *mut f64,
    return_size: c_int,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let rec = with(|s| {
        s.nuke_camera
            .as_ref()
            .filter(|c| c.handle == camera as usize)
            .map(|c| (c.connected, c.focal, c.haperture, c.vaperture))
    });
    let Some((connected, focal, haperture, vaperture)) = rec else {
        return OFX_STAT_ERR_BAD_HANDLE;
    };
    if !connected {
        return OFX_STAT_FAILED;
    }
    // SAFETY: the plugin passes a NUL-terminated parameter name.
    let name = unsafe { CStr::from_ptr(name) };
    let value = if name == NUKE_OFX_CAMERA_PARAM_FOCAL_LENGTH {
        focal
    } else if name == NUKE_OFX_CAMERA_PARAM_HORIZONTAL_APERTURE {
        haperture
    } else if name == NUKE_OFX_CAMERA_PARAM_VERTICAL_APERTURE {
        vaperture
    } else {
        return OFX_STAT_ERR_UNKNOWN;
    };
    if return_size != 1 || out.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    // SAFETY: the plugin passes a valid pointer for exactly one double, as `returnSize` 1 says.
    unsafe { out.write(value) };
    OFX_STAT_OK
}

/// Returns the mock camera suite table, built once and shared.
pub(super) fn camera_suite() -> &'static NukeOfxCameraSuiteV1 {
    static SUITE: OnceLock<NukeOfxCameraSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| NukeOfxCameraSuiteV1 {
        cameraDefine: camera_define,
        cameraGetHandle: camera_get_handle,
        cameraGetPropertySet: camera_get_property_set,
        cameraGetParameter: camera_get_parameter,
    })
}
