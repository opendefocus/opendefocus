//! Mock host object and the `fetchSuite` entry point that hands the plugin the suites it asks for.

use super::*;

/// `fetchSuite`: returns version 1 of the requested suites that are not omitted by the test.
pub(super) unsafe extern "C" fn fetch_suite(
    _host: OfxPropertySetHandle,
    name: *const c_char,
    version: c_int,
) -> *const c_void {
    if version != 1 || name.is_null() {
        return ptr::null();
    }
    // SAFETY: the plugin passes a NUL-terminated suite name.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    if with(|s| s.omitted.contains(&name)) {
        return ptr::null();
    }
    let p: *const c_void = if name == OFX_PROPERTY_SUITE.to_string_lossy() {
        std::ptr::from_ref(property_suite()).cast::<c_void>()
    } else if name == OFX_IMAGE_EFFECT_SUITE.to_string_lossy() {
        std::ptr::from_ref(image_effect_suite()).cast::<c_void>()
    } else if name == OFX_PARAMETER_SUITE.to_string_lossy() {
        std::ptr::from_ref(parameter_suite()).cast::<c_void>()
    } else if name == OFX_MEMORY_SUITE.to_string_lossy() {
        std::ptr::from_ref(memory_suite()).cast::<c_void>()
    } else if name == OFX_MULTI_THREAD_SUITE.to_string_lossy() {
        std::ptr::from_ref(multi_thread_suite()).cast::<c_void>()
    } else if name == OFX_MESSAGE_SUITE.to_string_lossy() {
        std::ptr::from_ref(message_suite()).cast::<c_void>()
    } else if name == NUKE_OFX_CAMERA_SUITE.to_string_lossy() && with(|s| s.nuke_camera.is_some()) {
        std::ptr::from_ref(camera_suite()).cast::<c_void>()
    } else {
        ptr::null()
    };
    p
}

pub(super) fn new_host() -> OfxHost {
    OfxHost {
        host: ptr::null_mut(),
        fetchSuite: fetch_suite,
    }
}
