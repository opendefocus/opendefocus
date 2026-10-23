//! Core OFX handle, struct and function-pointer types from `ofxCore.h`, plus the plugin entry point and host structs.

use core::ffi::{c_char, c_int, c_uint, c_void};

/// `typedef int OfxStatus;` (ofxCore.h:39)
pub type OfxStatus = c_int;

/// `typedef double OfxTime;` (ofxCore.h:897)
pub type OfxTime = f64;

/// Opaque `struct OfxPropertySetStruct` (ofxCore.h:36).
pub enum OfxPropertySetStruct {}
/// `typedef struct OfxPropertySetStruct *OfxPropertySetHandle;` (ofxCore.h:36)
pub type OfxPropertySetHandle = *mut OfxPropertySetStruct;

/// Opaque `struct OfxImageEffectStruct` (ofxImageEffect.h:34).
pub enum OfxImageEffectStruct {}
/// `typedef struct OfxImageEffectStruct *OfxImageEffectHandle;` (ofxImageEffect.h:34)
pub type OfxImageEffectHandle = *mut OfxImageEffectStruct;

/// Opaque `struct OfxImageClipStruct` (ofxImageEffect.h:38).
pub enum OfxImageClipStruct {}
/// `typedef struct OfxImageClipStruct *OfxImageClipHandle;` (ofxImageEffect.h:38)
pub type OfxImageClipHandle = *mut OfxImageClipStruct;

/// Opaque `struct OfxImageMemoryStruct` (ofxImageEffect.h:41).
pub enum OfxImageMemoryStruct {}
/// `typedef struct OfxImageMemoryStruct *OfxImageMemoryHandle;` (ofxImageEffect.h:41)
pub type OfxImageMemoryHandle = *mut OfxImageMemoryStruct;

/// Opaque `struct OfxParamSetStruct` (ofxParam.h:31).
pub enum OfxParamSetStruct {}
/// `typedef struct OfxParamSetStruct *OfxParamSetHandle;` (ofxParam.h:31)
pub type OfxParamSetHandle = *mut OfxParamSetStruct;

/// Opaque `struct OfxParamStruct` (ofxParam.h:26).
pub enum OfxParamStruct {}
/// `typedef struct OfxParamStruct *OfxParamHandle;` (ofxParam.h:26)
pub type OfxParamHandle = *mut OfxParamStruct;

/// Opaque `struct OfxMutex` (ofxMultiThread.h:23).
pub enum OfxMutex {}
/// `typedef struct OfxMutex *OfxMutexHandle;` (ofxMultiThread.h:23)
pub type OfxMutexHandle = *mut OfxMutex;

/// `typedef struct OfxRangeD { double min, max; } OfxRangeD;` (ofxCore.h:905-907)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OfxRangeD {
    /// Lower bound of the range, inclusive.
    pub min: f64,
    /// Upper bound of the range, inclusive.
    pub max: f64,
}

/// `typedef struct OfxPointD { double x, y; } OfxPointD;` (ofxCore.h:915-917)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OfxPointD {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

/// `typedef struct OfxRectI { int x1, y1, x2, y2; } OfxRectI;` (ofxCore.h:942-944)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfxRectI {
    /// Left edge of the rectangle, inclusive.
    pub x1: c_int,
    /// Bottom edge of the rectangle, inclusive.
    pub y1: c_int,
    /// Right edge of the rectangle, exclusive.
    pub x2: c_int,
    /// Top edge of the rectangle, exclusive.
    pub y2: c_int,
}

/// `typedef struct OfxRectD { double x1, y1, x2, y2; } OfxRectD;` (ofxCore.h:957-959)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OfxRectD {
    /// Left edge of the rectangle, inclusive.
    pub x1: f64,
    /// Bottom edge of the rectangle, inclusive.
    pub y1: f64,
    /// Right edge of the rectangle, exclusive.
    pub x2: f64,
    /// Top edge of the rectangle, exclusive.
    pub y2: f64,
}

/// `typedef struct OfxHost { ... } OfxHost;` (ofxCore.h:46-70)
#[repr(C)]
pub struct OfxHost {
    /// `OfxPropertySetHandle host;` (ofxCore.h, member of `OfxHost`)
    pub host: OfxPropertySetHandle,
    /// `const void *(*fetchSuite)(OfxPropertySetHandle host, const char *suiteName, int suiteVersion);`
    pub fetchSuite: unsafe extern "C" fn(
        host: OfxPropertySetHandle,
        suiteName: *const c_char,
        suiteVersion: c_int,
    ) -> *const c_void,
}

/// Entry point type, `typedef OfxStatus (OfxPluginEntryPoint)(...)` (ofxCore.h:86).
pub type OfxPluginEntryPoint = unsafe extern "C" fn(
    action: *const c_char,
    handle: *const c_void,
    inArgs: OfxPropertySetHandle,
    outArgs: OfxPropertySetHandle,
) -> OfxStatus;

/// `typedef struct OfxPlugin { ... } OfxPlugin;` (ofxCore.h:98-153)
#[repr(C)]
pub struct OfxPlugin {
    /// `const char *pluginApi;`
    pub pluginApi: *const c_char,
    /// `int apiVersion;`
    pub apiVersion: c_int,
    /// `const char *pluginIdentifier;`
    pub pluginIdentifier: *const c_char,
    /// `unsigned int pluginVersionMajor;`
    pub pluginVersionMajor: c_uint,
    /// `unsigned int pluginVersionMinor;`
    pub pluginVersionMinor: c_uint,
    /// `void (*setHost)(OfxHost *host);`
    pub setHost: unsafe extern "C" fn(host: *mut OfxHost),
    /// `OfxPluginEntryPoint *mainEntry;`
    pub mainEntry: OfxPluginEntryPoint,
}

/// `typedef void (OfxThreadFunctionV1)(unsigned int threadIndex, unsigned int threadMax, void *customArg);` (ofxMultiThread.h:33-35)
pub type OfxThreadFunctionV1 =
    unsafe extern "C" fn(threadIndex: c_uint, threadMax: c_uint, customArg: *mut c_void);
