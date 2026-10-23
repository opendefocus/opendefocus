//! Host handle, suite fetching at Load, and typed property access.

use core::ffi::{CStr, c_char, c_double, c_int, c_void};
use core::ptr;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::ofx_sys::{
    NUKE_OFX_CAMERA_SUITE, NukeOfxCameraSuiteV1, OFX_IMAGE_EFFECT_SUITE, OFX_MEMORY_SUITE,
    OFX_MESSAGE_ERROR, OFX_MESSAGE_SUITE, OFX_MULTI_THREAD_SUITE, OFX_PARAMETER_SUITE,
    OFX_PROPERTY_SUITE, OFX_STAT_ERR_MISSING_HOST_FEATURE, OFX_STAT_ERR_VALUE, OFX_STAT_OK,
    OfxHost, OfxImageEffectHandle, OfxImageEffectSuiteV1, OfxMemorySuiteV1, OfxMessageSuiteV1,
    OfxMultiThreadSuiteV1, OfxParamHandle, OfxParameterSuiteV1, OfxPropertySetHandle,
    OfxPropertySuiteV1, OfxStatus, OfxTime,
};

/// Message id passed with [`message_error`]. Hosts use it to identify the message.
const MESSAGE_ID: &CStr = c"org.opendefocus.OpenDefocus";
/// Format string used by [`message_error`]; the text is passed as the single variadic argument.
const MESSAGE_FORMAT: &CStr = c"%s";

/// Error carrying the non-OK `OfxStatus` returned by a host call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfxError(pub OfxStatus);

impl core::fmt::Display for OfxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "OFX host call failed with status {}", self.0)
    }
}

impl std::error::Error for OfxError {}

/// Result type for host calls.
pub type Result<T> = core::result::Result<T, OfxError>;

/// Writes a diagnostic line to stderr, prefixed with the plugin name.
#[allow(
    clippy::print_stderr,
    reason = "stderr is the only diagnostic channel when the host has no message suite or the failure happens in Drop or at the FFI boundary"
)]
pub fn diagnostic(text: &str) {
    eprintln!("opendefocus-ofx: {text}");
}

/// Converts `text` to a C string for a host call.
///
/// # Errors
///
/// Returns [`OfxError`] with `kOfxStatErrValue` when `text` contains a NUL byte.
pub fn c_string(text: &str) -> Result<std::ffi::CString> {
    std::ffi::CString::new(text).map_err(|_| OfxError(OFX_STAT_ERR_VALUE))
}

/// Maps an `OfxStatus` to `Ok(())` for `kOfxStatOK` and `Err` otherwise.
///
/// # Errors
///
/// Returns [`OfxError`] carrying `status` when it is not `kOfxStatOK`.
pub fn check(status: OfxStatus) -> Result<()> {
    if status == OFX_STAT_OK {
        Ok(())
    } else {
        Err(OfxError(status))
    }
}

/// Suites fetched from the host at Load. Pointers are valid until Unload.
///
/// The property, image effect and parameter suites are mandatory. Memory, multi-thread,
/// message and Nuke camera suites are optional in the host; callers must handle `None`.
#[derive(Clone, Copy)]
pub struct HostSuites {
    /// Property-set suite: reads and writes OFX property sets.
    pub property: &'static OfxPropertySuiteV1,
    /// Image-effect suite: effect property sets, clips and images.
    pub image_effect: &'static OfxImageEffectSuiteV1,
    /// Parameter suite: defines, reads and writes parameters.
    pub parameter: &'static OfxParameterSuiteV1,
    /// Memory suite, used for host-allocated buffers when present.
    pub memory: Option<&'static OfxMemorySuiteV1>,
    /// Multi-thread suite, used for host-managed threading when present.
    pub multi_thread: Option<&'static OfxMultiThreadSuiteV1>,
    /// Message suite, used to report errors to the user when present.
    pub message: Option<&'static OfxMessageSuiteV1>,
    /// Nuke's optional camera input suite. Present only in Nuke.
    pub camera: Option<&'static NukeOfxCameraSuiteV1>,
}

/// Host pointer recorded at `setHost` and cleared at Unload.
static HOST_HANDLE: Mutex<Option<HostHandle>> = Mutex::new(None);
/// Suites fetched at Load and cleared at Unload.
static HOST_SUITES: Mutex<Option<HostSuites>> = Mutex::new(None);

/// Raw pointer to the host structure passed to `setHost`, stored as an address so the
/// global can be `Send`. The host keeps it valid while the binary is loaded.
#[derive(Clone, Copy)]
struct HostHandle(usize);

/// Locks a global, recovering from poisoning so a panic in one action cannot block later ones.
fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Records the host pointer given to `setHost`.
///
/// # Safety
///
/// `host` must be null or point to an `OfxHost` that stays valid while the plugin is loaded.
pub unsafe fn set_host(host: *mut OfxHost) {
    *lock(&HOST_HANDLE) = Some(HostHandle(host as usize));
}

/// Fetches all suites the plugin uses from the host recorded by [`set_host`].
///
/// Returns `kOfxStatErrMissingHostFeature` if no host was set or a mandatory suite
/// (property, image effect, parameter) is missing. Optional suites that the host does not
/// provide are `None`.
///
/// # Errors
///
/// Returns [`OfxError`] with `kOfxStatErrMissingHostFeature` when no host was recorded or a
/// mandatory suite is missing.
///
/// # Safety
///
/// The host recorded by [`set_host`] must still be valid, which the host guarantees
/// between `setHost` and plugin unload.
pub unsafe fn load_suites() -> Result<HostSuites> {
    let host_ptr = lock(&HOST_HANDLE)
        .map(|h| h.0 as *mut OfxHost)
        .unwrap_or(ptr::null_mut());
    if host_ptr.is_null() {
        return Err(OfxError(OFX_STAT_ERR_MISSING_HOST_FEATURE));
    }
    // SAFETY: the host pointer is non-null and, per the caller contract, valid while loaded.
    let host = unsafe { &*host_ptr };
    // SAFETY: `host` is valid per the caller contract; each `fetch` call passes the NUL
    // terminated suite name and the version 1 struct type that the header defines for it.
    unsafe {
        Ok(HostSuites {
            property: fetch::<OfxPropertySuiteV1>(host, OFX_PROPERTY_SUITE)?,
            image_effect: fetch::<OfxImageEffectSuiteV1>(host, OFX_IMAGE_EFFECT_SUITE)?,
            parameter: fetch::<OfxParameterSuiteV1>(host, OFX_PARAMETER_SUITE)?,
            memory: fetch_optional::<OfxMemorySuiteV1>(host, OFX_MEMORY_SUITE),
            multi_thread: fetch_optional::<OfxMultiThreadSuiteV1>(host, OFX_MULTI_THREAD_SUITE),
            message: fetch_optional::<OfxMessageSuiteV1>(host, OFX_MESSAGE_SUITE),
            camera: fetch_optional::<NukeOfxCameraSuiteV1>(host, NUKE_OFX_CAMERA_SUITE),
        })
    }
}

/// Asks the host for version 1 of a suite; a null result is an error.
///
/// # Safety
///
/// `host.fetchSuite` must be callable for the current host, and `T` must be the struct the
/// header defines for `name` at version 1.
unsafe fn fetch<T>(host: &OfxHost, name: &CStr) -> Result<&'static T> {
    // SAFETY: forwarded from this function's contract; `name` is NUL terminated.
    unsafe { fetch_optional::<T>(host, name) }.ok_or(OfxError(OFX_STAT_ERR_MISSING_HOST_FEATURE))
}

/// Asks the host for version 1 of an optional suite; `None` if the host returns null.
///
/// # Safety
///
/// Same contract as [`fetch`].
unsafe fn fetch_optional<T>(host: &OfxHost, name: &CStr) -> Option<&'static T> {
    // SAFETY: `host.host` is the handle the host requires; `name` is NUL terminated and
    // outlives the call.
    let p = unsafe { (host.fetchSuite)(host.host, name.as_ptr(), 1) };
    if p.is_null() {
        None
    } else {
        // SAFETY: the host returned a non-null suite pointer for this name at version 1,
        // which the caller contract says has type `T` and lives until Unload.
        Some(unsafe { &*p.cast::<T>() })
    }
}

/// Stores the suites fetched at Load.
pub fn store_suites(suites: HostSuites) {
    *lock(&HOST_SUITES) = Some(suites);
}

/// Returns the suites stored at Load, or `kOfxStatErrMissingHostFeature` if Load did not run.
///
/// # Errors
///
/// Returns [`OfxError`] with `kOfxStatErrMissingHostFeature` if no suites were stored.
pub fn stored_suites() -> Result<HostSuites> {
    lock(&HOST_SUITES).ok_or(OfxError(OFX_STAT_ERR_MISSING_HOST_FEATURE))
}

/// Clears the stored host and suites at Unload.
pub fn clear() {
    *lock(&HOST_HANDLE) = None;
    *lock(&HOST_SUITES) = None;
}

/// A property set handle with the property suite it belongs to.
#[derive(Clone, Copy)]
pub struct Props<'a> {
    /// Property set the host returned.
    handle: OfxPropertySetHandle,
    /// Property suite used to read and write `handle`.
    suite: &'a OfxPropertySuiteV1,
}

impl<'a> Props<'a> {
    /// Wraps a property set handle returned by the host.
    pub fn new(handle: OfxPropertySetHandle, suite: &'a OfxPropertySuiteV1) -> Self {
        Self { handle, suite }
    }

    /// Sets a string property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn set_string(&self, name: &CStr, index: c_int, value: &CStr) -> Result<()> {
        // SAFETY: the handle came from the host and is valid for this action; both strings
        // are NUL-terminated and live for the call.
        check(unsafe {
            (self.suite.propSetString)(self.handle, name.as_ptr(), index, value.as_ptr())
        })
    }

    /// Sets an integer property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn set_int(&self, name: &CStr, index: c_int, value: c_int) -> Result<()> {
        // SAFETY: the handle came from the host and is valid for this action.
        check(unsafe { (self.suite.propSetInt)(self.handle, name.as_ptr(), index, value) })
    }

    /// Sets a double property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn set_double(&self, name: &CStr, index: c_int, value: c_double) -> Result<()> {
        // SAFETY: the handle came from the host and is valid for this action.
        check(unsafe { (self.suite.propSetDouble)(self.handle, name.as_ptr(), index, value) })
    }

    /// Sets a pointer property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    #[allow(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "opaque host handles are only passed back to the host"
    )]
    pub fn set_pointer(&self, name: &CStr, index: c_int, value: *mut c_void) -> Result<()> {
        // SAFETY: the handle came from the host and is valid for this action.
        check(unsafe { (self.suite.propSetPointer)(self.handle, name.as_ptr(), index, value) })
    }

    /// Reads a string property at `index` as an owned, lossy UTF-8 string.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn get_string(&self, name: &CStr, index: c_int) -> Result<String> {
        let mut value: *mut c_char = ptr::null_mut();
        // SAFETY: `value` is a valid out pointer; the host returns a string owned by the
        // property set, which is copied before the set can change.
        check(unsafe {
            (self.suite.propGetString)(self.handle, name.as_ptr(), index, &raw mut value)
        })?;
        if value.is_null() {
            return Ok(String::new());
        }
        // SAFETY: the host returned a non-null NUL-terminated string for this property.
        Ok(unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned())
    }

    /// Reads an integer property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn get_int(&self, name: &CStr, index: c_int) -> Result<c_int> {
        let mut value = 0;
        // SAFETY: `value` is a valid out pointer for the duration of the call.
        check(unsafe {
            (self.suite.propGetInt)(self.handle, name.as_ptr(), index, &raw mut value)
        })?;
        Ok(value)
    }

    /// Reads a double property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn get_double(&self, name: &CStr, index: c_int) -> Result<c_double> {
        let mut value = 0.0;
        // SAFETY: `value` is a valid out pointer for the duration of the call.
        check(unsafe {
            (self.suite.propGetDouble)(self.handle, name.as_ptr(), index, &raw mut value)
        })?;
        Ok(value)
    }

    /// Reads a pointer property at `index`.
    ///
    /// # Errors
    ///
    /// Returns the host's non-OK status as an [`OfxError`].
    pub fn get_pointer(&self, name: &CStr, index: c_int) -> Result<*mut c_void> {
        let mut value: *mut c_void = ptr::null_mut();
        // SAFETY: `value` is a valid out pointer for the duration of the call.
        check(unsafe {
            (self.suite.propGetPointer)(self.handle, name.as_ptr(), index, &raw mut value)
        })?;
        Ok(value)
    }
}

/// Reads a double parameter value at `time` through `paramGetValueAtTime`.
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load or the host returns a non-OK status.
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "opaque host handles are only passed back to the host"
)]
pub fn param_get_f64(param: OfxParamHandle, time: OfxTime) -> Result<f64> {
    let suites = stored_suites()?;
    let mut value: f64 = 0.0;
    // SAFETY: `paramGetValueAtTime` is declared as `(OfxParamHandle, OfxTime, ...)`. The single
    // variadic argument is a `double *` that the host writes one `double` to.
    check(unsafe { (suites.parameter.paramGetValueAtTime)(param, time, &raw mut value) })?;
    Ok(value)
}

/// Reads an integer or choice parameter value at `time`.
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load or the host returns a non-OK status.
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "opaque host handles are only passed back to the host"
)]
pub fn param_get_i32(param: OfxParamHandle, time: OfxTime) -> Result<i32> {
    let suites = stored_suites()?;
    let mut value: c_int = 0;
    // SAFETY: as in [`param_get_f64`], the variadic argument is an `int *` for integer and
    // choice parameters, which the OFX header defines as `int` values.
    check(unsafe { (suites.parameter.paramGetValueAtTime)(param, time, &raw mut value) })?;
    Ok(value)
}

/// Reads a boolean parameter value at `time`. OFX passes booleans as `int` (0 or 1).
///
/// # Errors
///
/// Returns [`OfxError`] if the parameter value cannot be read; see [`param_get_i32`].
pub fn param_get_bool(param: OfxParamHandle, time: OfxTime) -> Result<bool> {
    Ok(param_get_i32(param, time)? != 0)
}

/// Reads a string parameter value at `time`. The host passes `const char **`; the string is
/// copied into an owned `String` before returning, since the host may free it later.
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load or the host returns a non-OK status.
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "opaque host handles are only passed back to the host"
)]
pub fn param_get_string(param: OfxParamHandle, time: OfxTime) -> Result<String> {
    let suites = stored_suites()?;
    let mut value: *const c_char = ptr::null();
    // SAFETY: the variadic argument is a `const char **` that the host writes one pointer to.
    // The pointed-to string is owned by the host and valid until the parameter changes, so it
    // is copied immediately.
    check(unsafe { (suites.parameter.paramGetValueAtTime)(param, time, &raw mut value) })?;
    if value.is_null() {
        return Ok(String::new());
    }
    // SAFETY: the host returned a non-null NUL-terminated string for this parameter.
    Ok(unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned())
}

/// Reports an error message through the message suite, with `text` as the message body.
///
/// Returns `kOfxStatErrMissingHostFeature` if the host does not provide the message suite.
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load, the message suite is missing, or
/// the host returns a non-OK status.
pub fn message_error(effect: OfxImageEffectHandle, text: &CStr) -> Result<()> {
    let suites = stored_suites()?;
    let message = suites
        .message
        .ok_or(OfxError(OFX_STAT_ERR_MISSING_HOST_FEATURE))?;
    // SAFETY: `message` is declared as `(void *handle, const char *type, const char *id,
    // const char *format, ...)`. The format `%s` consumes exactly one `const char *`, the
    // NUL-terminated `text`, which lives for the call.
    check(unsafe {
        (message.message)(
            effect.cast(),
            OFX_MESSAGE_ERROR.as_ptr(),
            MESSAGE_ID.as_ptr(),
            MESSAGE_FORMAT.as_ptr(),
            text.as_ptr(),
        )
    })
}
