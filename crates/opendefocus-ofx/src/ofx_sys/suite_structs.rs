//! `#[repr(C)]` suite tables the host fills in, with field order and signatures copied from the OFX headers.

use super::core_types::{
    OfxImageClipHandle, OfxImageEffectHandle, OfxImageMemoryHandle, OfxMutexHandle, OfxParamHandle,
    OfxParamSetHandle, OfxPropertySetHandle, OfxRangeD, OfxRectD, OfxStatus, OfxThreadFunctionV1,
    OfxTime,
};
use core::ffi::{c_char, c_int, c_uint, c_void};

/// `typedef struct OfxPropertySuiteV1 { ... } OfxPropertySuiteV1;` (ofxProperty.h:23-302)
#[repr(C)]
pub struct OfxPropertySuiteV1 {
    /// ofxProperty.h:38
    pub propSetPointer: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut c_void,
    ) -> OfxStatus,
    /// ofxProperty.h:54
    pub propSetString: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *const c_char,
    ) -> OfxStatus,
    /// ofxProperty.h:70
    pub propSetDouble: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: f64,
    ) -> OfxStatus,
    /// ofxProperty.h:86
    pub propSetInt: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: c_int,
    ) -> OfxStatus,
    /// ofxProperty.h:102
    pub propSetPointerN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const *mut c_void,
    ) -> OfxStatus,
    /// ofxProperty.h:118
    pub propSetStringN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const *const c_char,
    ) -> OfxStatus,
    /// ofxProperty.h:135
    pub propSetDoubleN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const f64,
    ) -> OfxStatus,
    /// ofxProperty.h:152
    pub propSetIntN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *const c_int,
    ) -> OfxStatus,
    /// ofxProperty.h:167
    pub propGetPointer: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut *mut c_void,
    ) -> OfxStatus,
    /// ofxProperty.h:182
    pub propGetString: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut *mut c_char,
    ) -> OfxStatus,
    /// ofxProperty.h:199
    pub propGetDouble: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut f64,
    ) -> OfxStatus,
    /// ofxProperty.h:214
    pub propGetInt: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        index: c_int,
        value: *mut c_int,
    ) -> OfxStatus,
    /// ofxProperty.h:229
    pub propGetPointerN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut *mut c_void,
    ) -> OfxStatus,
    /// ofxProperty.h:246
    pub propGetStringN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut *mut c_char,
    ) -> OfxStatus,
    /// ofxProperty.h:261
    pub propGetDoubleN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut f64,
    ) -> OfxStatus,
    /// ofxProperty.h:276
    pub propGetIntN: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: c_int,
        value: *mut c_int,
    ) -> OfxStatus,
    /// ofxProperty.h:288
    pub propReset: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
    ) -> OfxStatus,
    /// ofxProperty.h:301
    pub propGetDimension: unsafe extern "C" fn(
        properties: OfxPropertySetHandle,
        property: *const c_char,
        count: *mut c_int,
    ) -> OfxStatus,
}

/// `typedef struct OfxImageEffectSuiteV1 { ... } OfxImageEffectSuiteV1;` (ofxImageEffect.h:1780-2039)
#[repr(C)]
pub struct OfxImageEffectSuiteV1 {
    /// ofxImageEffect.h:1793
    pub getPropertySet: unsafe extern "C" fn(
        imageEffect: OfxImageEffectHandle,
        propHandle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1808
    pub getParamSet: unsafe extern "C" fn(
        imageEffect: OfxImageEffectHandle,
        paramSet: *mut OfxParamSetHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1826
    pub clipDefine: unsafe extern "C" fn(
        imageEffect: OfxImageEffectHandle,
        name: *const c_char,
        propertySet: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1854
    pub clipGetHandle: unsafe extern "C" fn(
        imageEffect: OfxImageEffectHandle,
        name: *const c_char,
        clip: *mut OfxImageClipHandle,
        propertySet: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1871
    pub clipGetPropertySet: unsafe extern "C" fn(
        clip: OfxImageClipHandle,
        propHandle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1903
    pub clipGetImage: unsafe extern "C" fn(
        clip: OfxImageClipHandle,
        time: OfxTime,
        region: *const OfxRectD,
        imageHandle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1921
    pub clipReleaseImage: unsafe extern "C" fn(imageHandle: OfxPropertySetHandle) -> OfxStatus,
    /// ofxImageEffect.h:1941
    pub clipGetRegionOfDefinition: unsafe extern "C" fn(
        clip: OfxImageClipHandle,
        time: OfxTime,
        bounds: *mut OfxRectD,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1958
    pub abort: unsafe extern "C" fn(imageEffect: OfxImageEffectHandle) -> c_int,
    /// ofxImageEffect.h:1976
    pub imageMemoryAlloc: unsafe extern "C" fn(
        instanceHandle: OfxImageEffectHandle,
        nBytes: usize,
        memoryHandle: *mut OfxImageMemoryHandle,
    ) -> OfxStatus,
    /// ofxImageEffect.h:1994
    pub imageMemoryFree: unsafe extern "C" fn(memoryHandle: OfxImageMemoryHandle) -> OfxStatus,
    /// ofxImageEffect.h:2016
    pub imageMemoryLock: unsafe extern "C" fn(
        memoryHandle: OfxImageMemoryHandle,
        returnedPtr: *mut *mut c_void,
    ) -> OfxStatus,
    /// ofxImageEffect.h:2037
    pub imageMemoryUnlock: unsafe extern "C" fn(memoryHandle: OfxImageMemoryHandle) -> OfxStatus,
}

/// `typedef struct OfxParameterSuiteV1 { ... } OfxParameterSuiteV1;` (ofxParam.h:1033-1404)
#[repr(C)]
pub struct OfxParameterSuiteV1 {
    /// ofxParam.h:1055
    pub paramDefine: unsafe extern "C" fn(
        paramSet: OfxParamSetHandle,
        paramType: *const c_char,
        name: *const c_char,
        propertySet: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxParam.h:1074
    pub paramGetHandle: unsafe extern "C" fn(
        paramSet: OfxParamSetHandle,
        name: *const c_char,
        param: *mut OfxParamHandle,
        propertySet: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxParam.h:1091
    pub paramSetGetPropertySet: unsafe extern "C" fn(
        paramSet: OfxParamSetHandle,
        propHandle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxParam.h:1106
    pub paramGetPropertySet: unsafe extern "C" fn(
        param: OfxParamHandle,
        propHandle: *mut OfxPropertySetHandle,
    ) -> OfxStatus,
    /// ofxParam.h:1134. `(OfxParamHandle paramHandle, ...)`
    pub paramGetValue: unsafe extern "C" fn(OfxParamHandle, ...) -> OfxStatus,
    /// ofxParam.h:1152. `(OfxParamHandle paramHandle, OfxTime time, ...)`
    pub paramGetValueAtTime: unsafe extern "C" fn(OfxParamHandle, OfxTime, ...) -> OfxStatus,
    /// ofxParam.h:1174. `(OfxParamHandle paramHandle, OfxTime time, ...)`
    pub paramGetDerivative: unsafe extern "C" fn(OfxParamHandle, OfxTime, ...) -> OfxStatus,
    /// ofxParam.h:1197. `(OfxParamHandle paramHandle, OfxTime time1, OfxTime time2, ...)`
    pub paramGetIntegral: unsafe extern "C" fn(OfxParamHandle, OfxTime, OfxTime, ...) -> OfxStatus,
    /// ofxParam.h:1220. `(OfxParamHandle paramHandle, ...)`
    pub paramSetValue: unsafe extern "C" fn(OfxParamHandle, ...) -> OfxStatus,
    /// ofxParam.h:1241. `(OfxParamHandle paramHandle, OfxTime time, ...)`
    pub paramSetValueAtTime: unsafe extern "C" fn(OfxParamHandle, OfxTime, ...) -> OfxStatus,
    /// ofxParam.h:1274
    pub paramGetNumKeys:
        unsafe extern "C" fn(paramHandle: OfxParamHandle, numberOfKeys: *mut c_uint) -> OfxStatus,
    /// ofxParam.h:1288
    pub paramGetKeyTime: unsafe extern "C" fn(
        paramHandle: OfxParamHandle,
        nthKey: c_uint,
        time: *mut OfxTime,
    ) -> OfxStatus,
    /// ofxParam.h:1308
    pub paramGetKeyIndex: unsafe extern "C" fn(
        paramHandle: OfxParamHandle,
        time: OfxTime,
        direction: c_int,
        index: *mut c_int,
    ) -> OfxStatus,
    /// ofxParam.h:1323
    pub paramDeleteKey:
        unsafe extern "C" fn(paramHandle: OfxParamHandle, time: OfxTime) -> OfxStatus,
    /// ofxParam.h:1338
    pub paramDeleteAllKeys: unsafe extern "C" fn(paramHandle: OfxParamHandle) -> OfxStatus,
    /// ofxParam.h:1363
    pub paramCopy: unsafe extern "C" fn(
        paramTo: OfxParamHandle,
        paramFrom: OfxParamHandle,
        dstOffset: OfxTime,
        frameRange: *const OfxRangeD,
    ) -> OfxStatus,
    /// ofxParam.h:1384
    pub paramEditBegin:
        unsafe extern "C" fn(paramSet: OfxParamSetHandle, name: *const c_char) -> OfxStatus,
    /// ofxParam.h:1403
    pub paramEditEnd: unsafe extern "C" fn(paramSet: OfxParamSetHandle) -> OfxStatus,
}

/// `typedef struct OfxMemorySuiteV1 { ... } OfxMemorySuiteV1;` (ofxMemory.h:22-53)
#[repr(C)]
pub struct OfxMemorySuiteV1 {
    /// ofxMemory.h:37
    pub memoryAlloc: unsafe extern "C" fn(
        handle: *mut c_void,
        nBytes: usize,
        allocatedData: *mut *mut c_void,
    ) -> OfxStatus,
    /// ofxMemory.h:52
    pub memoryFree: unsafe extern "C" fn(allocatedData: *mut c_void) -> OfxStatus,
}

/// `typedef struct OfxMultiThreadSuiteV1 { ... } OfxMultiThreadSuiteV1;` (ofxMultiThread.h:39-159)
#[repr(C)]
pub struct OfxMultiThreadSuiteV1 {
    /// ofxMultiThread.h:64
    pub multiThread: unsafe extern "C" fn(
        func: OfxThreadFunctionV1,
        nThreads: c_uint,
        customArg: *mut c_void,
    ) -> OfxStatus,
    /// ofxMultiThread.h:78
    pub multiThreadNumCPUs: unsafe extern "C" fn(nCPUs: *mut c_uint) -> OfxStatus,
    /// ofxMultiThread.h:92
    pub multiThreadIndex: unsafe extern "C" fn(threadIndex: *mut c_uint) -> OfxStatus,
    /// ofxMultiThread.h:100
    pub multiThreadIsSpawnedThread: unsafe extern "C" fn() -> c_int,
    /// ofxMultiThread.h:112
    pub mutexCreate:
        unsafe extern "C" fn(mutex: *mut OfxMutexHandle, lockCount: c_int) -> OfxStatus,
    /// ofxMultiThread.h:122
    pub mutexDestroy: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
    /// ofxMultiThread.h:134
    pub mutexLock: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
    /// ofxMultiThread.h:144
    pub mutexUnLock: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
    /// ofxMultiThread.h:157
    pub mutexTryLock: unsafe extern "C" fn(mutex: OfxMutexHandle) -> OfxStatus,
}

/// `typedef struct OfxMessageSuiteV1 { ... } OfxMessageSuiteV1;` (ofxMessage.h:64-87)
#[repr(C)]
pub struct OfxMessageSuiteV1 {
    /// ofxMessage.h:81. `(void *handle, const char *messageType, const char *messageId, const char *format, ...)`
    pub message: unsafe extern "C" fn(
        handle: *mut c_void,
        messageType: *const c_char,
        messageId: *const c_char,
        format: *const c_char,
        ...
    ) -> OfxStatus,
}
