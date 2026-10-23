//! Nuke's camera suite, a Foundry extension to OFX used to read camera focal length and aperture.

use super::core_types::{OfxImageEffectHandle, OfxPropertySetHandle, OfxStatus};
use core::ffi::CStr;
use core::ffi::{c_char, c_int};

/// `typedef struct NukeOfxCameraStruct *NukeOfxCameraHandle;` (nukeOfxCamera.h). Opaque.
pub enum NukeOfxCameraStruct {}
/// `typedef struct NukeOfxCameraStruct *NukeOfxCameraHandle;` (nukeOfxCamera.h)
pub type NukeOfxCameraHandle = *mut NukeOfxCameraStruct;

/// `#define kNukeOfxCameraSuite "NukeOfxCameraSuite"` (nukeOfxCamera.h). Fetched at version 1.
pub const NUKE_OFX_CAMERA_SUITE: &CStr = c"NukeOfxCameraSuite";

/// `#define kNukeOfxCameraParamFocalLength "focal"` (nukeOfxCamera.h). Millimetres.
pub const NUKE_OFX_CAMERA_PARAM_FOCAL_LENGTH: &CStr = c"focal";
/// `#define kNukeOfxCameraParamHorizontalAperture "haperture"` (nukeOfxCamera.h). Millimetres.
pub const NUKE_OFX_CAMERA_PARAM_HORIZONTAL_APERTURE: &CStr = c"haperture";
/// `#define kNukeOfxCameraParamVerticalAperture "vaperture"` (nukeOfxCamera.h). Millimetres.
pub const NUKE_OFX_CAMERA_PARAM_VERTICAL_APERTURE: &CStr = c"vaperture";

/// `typedef struct NukeOfxCameraSuiteV1 { ... } NukeOfxCameraSuiteV1;` (nukeOfxCamera.h).
/// Field order and signatures are those of the header.
#[repr(C)]
pub struct NukeOfxCameraSuiteV1 {
    /// nukeOfxCamera.h: `OfxStatus (*cameraDefine)(OfxImageEffectHandle pluginHandle,
    /// const char *name, OfxPropertySetHandle *propertySet);`
    pub cameraDefine: unsafe extern "C" fn(
        pluginHandle: OfxImageEffectHandle,
        name: *const c_char,
        propertySet: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// nukeOfxCamera.h: `OfxStatus (*cameraGetHandle)(OfxImageEffectHandle pluginHandle,
    /// const char *name, NukeOfxCameraHandle *camera, OfxPropertySetHandle *propertySet);`
    pub cameraGetHandle: unsafe extern "C" fn(
        pluginHandle: OfxImageEffectHandle,
        name: *const c_char,
        camera: *mut NukeOfxCameraHandle,
        propertySet: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// nukeOfxCamera.h: `OfxStatus (*cameraGetPropertySet)(NukeOfxCameraHandle camera,
    /// OfxPropertySetHandle *propHandle);`
    pub cameraGetPropertySet: unsafe extern "C" fn(
        camera: NukeOfxCameraHandle,
        propHandle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// nukeOfxCamera.h: `OfxStatus (*cameraGetParameter)(NukeOfxCameraHandle camera,
    /// const char* paramName, double time, int view, double* baseReturnAddress, int returnSize);`
    pub cameraGetParameter: unsafe extern "C" fn(
        camera: NukeOfxCameraHandle,
        paramName: *const c_char,
        time: f64,
        view: c_int,
        baseReturnAddress: *mut f64,
        returnSize: c_int,
    ) -> OfxStatus,
}
