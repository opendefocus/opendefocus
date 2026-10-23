//! Mock property suite: the typed property getters and setters the plugin uses, and stubs for the other property-suite entries.

use super::*;

pub(super) unsafe extern "C" fn prop_set_pointer(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut c_void,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: the plugin passes a valid NUL-terminated name.
    let key = unsafe { key_of(props as usize, name) };
    store(key, index, Value::Ptr(value as usize))
}

pub(super) unsafe extern "C" fn prop_set_string(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *const c_char,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: the plugin passes valid NUL-terminated strings.
    let (key, value) = unsafe {
        (
            key_of(props as usize, name),
            CStr::from_ptr(value).to_owned(),
        )
    };
    store(key, index, Value::Str(value))
}

pub(super) unsafe extern "C" fn prop_set_double(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: f64,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: the plugin passes a valid NUL-terminated name.
    let key = unsafe { key_of(props as usize, name) };
    store(key, index, Value::Double(value))
}

pub(super) unsafe extern "C" fn prop_set_int(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: c_int,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: the plugin passes a valid NUL-terminated name.
    let key = unsafe { key_of(props as usize, name) };
    store(key, index, Value::Int(value))
}

pub(super) unsafe extern "C" fn prop_get_pointer(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    out: *mut *mut c_void,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: valid name per the plugin's call; `out` checked by `put`.
    let key = unsafe { key_of(props as usize, name) };
    match read(&key, index, |v| match v {
        Value::Ptr(p) => Some(*p as *mut c_void),
        _ => None,
    }) {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Ok(p) => unsafe { put(out, p) },
        Err(status) => status,
    }
}

pub(super) unsafe extern "C" fn prop_get_string(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    out: *mut *mut c_char,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: valid name per the plugin's call.
    let key = unsafe { key_of(props as usize, name) };
    match read(&key, index, |v| match v {
        // The pointer stays valid until the property is replaced; the plugin copies it.
        Value::Str(s) => Some(s.as_ptr().cast_mut()),
        Value::NullStr => Some(ptr::null_mut()),
        _ => None,
    }) {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Ok(p) => unsafe { put(out, p) },
        Err(status) => status,
    }
}

pub(super) unsafe extern "C" fn prop_get_double(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    out: *mut f64,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: valid name per the plugin's call.
    let key = unsafe { key_of(props as usize, name) };
    match read(&key, index, |v| match v {
        Value::Double(d) => Some(*d),
        _ => None,
    }) {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Ok(d) => unsafe { put(out, d) },
        Err(status) => status,
    }
}

pub(super) unsafe extern "C" fn prop_get_int(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    out: *mut c_int,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: valid name per the plugin's call.
    let key = unsafe { key_of(props as usize, name) };
    match read(&key, index, |v| match v {
        Value::Int(i) => Some(*i),
        _ => None,
    }) {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Ok(i) => unsafe { put(out, i) },
        Err(status) => status,
    }
}

pub(super) unsafe extern "C" fn prop_reset(
    props: OfxPropertySetHandle,
    name: *const c_char,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: valid name per the plugin's call.
    let key = unsafe { key_of(props as usize, name) };
    with(|s| s.props.remove(&key));
    OFX_STAT_OK
}

pub(super) unsafe extern "C" fn prop_get_dimension(
    props: OfxPropertySetHandle,
    name: *const c_char,
    out: *mut c_int,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: valid name per the plugin's call.
    let key = unsafe { key_of(props as usize, name) };
    let count = with(|s| s.props.get(&key).map_or(0, Vec::len));
    // SAFETY: `out` is a valid out pointer per the host contract.
    unsafe { put(out, count as c_int) }
}

macro_rules! failed_fns {
    ($( fn $name:ident($($arg:ident : $ty:ty),* $(,)?) ; )*) => {
        $(
            #[allow(unused_variables, reason = "the failing stubs ignore their arguments")]
            pub(super) unsafe extern "C" fn $name($($arg: $ty),*) -> OfxStatus {
                OFX_STAT_FAILED
            }
        )*
    };
}

failed_fns! {
    fn prop_set_pointer_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *const *mut c_void);
    fn prop_set_string_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *const *const c_char);
    fn prop_set_double_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *const f64);
    fn prop_set_int_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *const c_int);
    fn prop_get_pointer_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *mut *mut c_void);
    fn prop_get_string_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *mut *mut c_char);
    fn prop_get_double_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *mut f64);
    fn prop_get_int_n(p: OfxPropertySetHandle, name: *const c_char, count: c_int, v: *mut c_int);
    fn image_memory_alloc(instance: OfxImageEffectHandle, n: usize, handle: *mut OfxImageMemoryHandle);
    fn image_memory_free(handle: OfxImageMemoryHandle);
    fn image_memory_lock(handle: OfxImageMemoryHandle, ptr: *mut *mut c_void);
    fn image_memory_unlock(handle: OfxImageMemoryHandle);
    fn param_num_keys(param: OfxParamHandle, n: *mut c_uint);
    fn param_key_time(param: OfxParamHandle, nth: c_uint, time: *mut OfxTime);
    fn param_key_index(param: OfxParamHandle, time: OfxTime, direction: c_int, index: *mut c_int);
    fn param_delete_key(param: OfxParamHandle, time: OfxTime);
    fn param_delete_all_keys(param: OfxParamHandle);
    fn param_copy(to: OfxParamHandle, from: OfxParamHandle, offset: OfxTime, range: *const OfxRangeD);
    fn param_edit_begin(set: OfxParamSetHandle, name: *const c_char);
    fn param_edit_end(set: OfxParamSetHandle);
    fn mutex_destroy(mutex: *mut OfxMutex);
    fn mutex_lock(mutex: *mut OfxMutex);
    fn mutex_unlock(mutex: *mut OfxMutex);
    fn mutex_try_lock(mutex: *mut OfxMutex);
}

/// Returns the mock property suite table, built once and shared.
pub(super) fn property_suite() -> &'static OfxPropertySuiteV1 {
    static SUITE: OnceLock<OfxPropertySuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxPropertySuiteV1 {
        propSetPointer: prop_set_pointer,
        propSetString: prop_set_string,
        propSetDouble: prop_set_double,
        propSetInt: prop_set_int,
        propSetPointerN: prop_set_pointer_n,
        propSetStringN: prop_set_string_n,
        propSetDoubleN: prop_set_double_n,
        propSetIntN: prop_set_int_n,
        propGetPointer: prop_get_pointer,
        propGetString: prop_get_string,
        propGetDouble: prop_get_double,
        propGetInt: prop_get_int,
        propGetPointerN: prop_get_pointer_n,
        propGetStringN: prop_get_string_n,
        propGetDoubleN: prop_get_double_n,
        propGetIntN: prop_get_int_n,
        propReset: prop_reset,
        propGetDimension: prop_get_dimension,
    })
}
