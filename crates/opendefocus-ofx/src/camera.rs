//! Optional Nuke camera input (the Foundry `NukeOfxCameraSuite` extension).
//!
//! When the host provides the suite, `DescribeInContext` defines an optional input named
//! [`CAMERA_INPUT`]. At render time a connected camera supplies the focal length and filmback
//! in place of the plain camera parameters. Without the suite, or without a connected camera,
//! nothing here is used and the plain parameters apply.

use core::ffi::{CStr, c_int};
use core::ptr;

use crate::ofx_sys::{
    NUKE_OFX_CAMERA_PARAM_FOCAL_LENGTH, NUKE_OFX_CAMERA_PARAM_HORIZONTAL_APERTURE,
    NUKE_OFX_CAMERA_PARAM_VERTICAL_APERTURE, NukeOfxCameraHandle, OFX_IMAGE_CLIP_PROP_CONNECTED,
    OFX_IMAGE_CLIP_PROP_OPTIONAL, OFX_STAT_ERR_BAD_HANDLE, OfxImageEffectHandle,
    OfxPropertySetHandle, OfxTime,
};
use crate::suites::{HostSuites, OfxError, Props, Result, check};

/// Name of the optional camera input defined in `DescribeInContext`.
pub const CAMERA_INPUT: &CStr = c"Camera";

/// Instance property holding the Nuke view index, passed in every action's in-args.
///
/// UNVERIFIED: nukeOfxCamera.h only says the view is "a custom nuke property" and does not name
/// it. The name is `kFnOfxImageEffectPropView` from Foundry's fnPublicOfxExtensions.h, recalled
/// from memory (that header was not kept); check it against the SDK. While it is absent the view is 0,
/// which is the only view of a mono clip.
const NUKE_VIEW_PROPERTY: &CStr = c"FnOfxImageEffectPropView";

/// Camera values read from a connected Nuke camera, in millimetres as Nuke reports them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NukeCamera {
    /// `focal`: focal length in millimetres.
    pub focal_length: f64,
    /// `haperture`: horizontal aperture (filmback width) in millimetres.
    pub haperture: f64,
    /// `vaperture`: vertical aperture (filmback height) in millimetres.
    pub vaperture: f64,
}

/// Defines the optional Camera input on the descriptor, when the host has the camera suite.
///
/// # Errors
///
/// Returns [`OfxError`] if the host rejects the camera definition or returns no property set.
///
/// # Safety
///
/// `effect` must be the descriptor handle the host passed to `DescribeInContext`, valid for that
/// action.
pub unsafe fn define_camera_input(suites: &HostSuites, effect: OfxImageEffectHandle) -> Result<()> {
    let Some(camera_suite) = suites.camera else {
        return Ok(());
    };
    let mut props: OfxPropertySetHandle = ptr::null_mut();
    // SAFETY: `effect` is valid for this action per the caller contract; the name is a static
    // NUL-terminated string that lives for the call; `props` is a valid out pointer.
    check(unsafe { (camera_suite.cameraDefine)(effect, CAMERA_INPUT.as_ptr(), &raw mut props) })?;
    if props.is_null() {
        return Err(OfxError(OFX_STAT_ERR_BAD_HANDLE));
    }
    // The input is optional: an unconnected camera must not stop the effect from rendering.
    Props::new(props, suites.property).set_int(OFX_IMAGE_CLIP_PROP_OPTIONAL, 0, 1)
}

/// Reads the connected camera's focal length and aperture at `time`, for the view named in
/// `in_props`. Returns `None` when the host has no camera suite or the input is not connected.
///
/// # Errors
///
/// Returns [`OfxError`] if a host camera call or property read fails.
///
/// # Safety
///
/// `effect` must be the instance handle the host passed to the current action, and `in_props`
/// must be that action's in-args, both valid for the call.
pub unsafe fn read_connected_camera(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
    in_props: &Props<'_>,
    time: OfxTime,
) -> Result<Option<NukeCamera>> {
    let Some(camera_suite) = suites.camera else {
        return Ok(None);
    };
    let mut camera: NukeOfxCameraHandle = ptr::null_mut();
    let mut descriptor: OfxPropertySetHandle = ptr::null_mut();
    // SAFETY: `effect` is valid for this action per the caller contract; the name is static and
    // NUL-terminated; both out pointers are valid.
    // A host that fails the lookup of an unconnected optional camera means "no camera".
    let status = unsafe {
        (camera_suite.cameraGetHandle)(
            effect,
            CAMERA_INPUT.as_ptr(),
            &raw mut camera,
            &raw mut descriptor,
        )
    };
    if status != crate::ofx_sys::OFX_STAT_OK || camera.is_null() {
        return Ok(None);
    }
    let mut instance: OfxPropertySetHandle = ptr::null_mut();
    // SAFETY: `camera` came from cameraGetHandle just above; `instance` is a valid out pointer.
    check(unsafe { (camera_suite.cameraGetPropertySet)(camera, &raw mut instance) })?;
    let connected =
        Props::new(instance, suites.property).get_int(OFX_IMAGE_CLIP_PROP_CONNECTED, 0)? != 0;
    if !connected {
        return Ok(None);
    }
    // A host that does not send the view property is treated as view 0.
    let view = in_props.get_int(NUKE_VIEW_PROPERTY, 0).unwrap_or(0);
    // SAFETY: `camera` is valid as above and the view and time come from this action.
    let focal_length = unsafe {
        camera_value(
            camera_suite,
            camera,
            NUKE_OFX_CAMERA_PARAM_FOCAL_LENGTH,
            time,
            view,
        )
    }?;
    // SAFETY: as above.
    let haperture = unsafe {
        camera_value(
            camera_suite,
            camera,
            NUKE_OFX_CAMERA_PARAM_HORIZONTAL_APERTURE,
            time,
            view,
        )
    }?;
    // SAFETY: as above.
    let vaperture = unsafe {
        camera_value(
            camera_suite,
            camera,
            NUKE_OFX_CAMERA_PARAM_VERTICAL_APERTURE,
            time,
            view,
        )
    }?;
    Ok(Some(NukeCamera {
        focal_length,
        haperture,
        vaperture,
    }))
}

/// Reads one camera parameter as a single double.
///
/// # Safety
///
/// `camera` must be a handle from cameraGetHandle in the current action, and `name` a
/// NUL-terminated parameter name.
unsafe fn camera_value(
    suite: &crate::ofx_sys::NukeOfxCameraSuiteV1,
    camera: NukeOfxCameraHandle,
    name: &CStr,
    time: OfxTime,
    view: c_int,
) -> Result<f64> {
    let mut value: f64 = 0.0;
    // SAFETY: `value` is a valid out pointer for exactly one double, matching `returnSize` 1.
    check(unsafe {
        (suite.cameraGetParameter)(camera, name.as_ptr(), time, view, &raw mut value, 1)
    })?;
    Ok(value)
}
