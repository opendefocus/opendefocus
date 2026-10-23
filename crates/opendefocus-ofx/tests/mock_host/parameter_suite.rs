//! Mock parameter suite: parameter definition, lookup, property access and value reads, including the variadic get-value entry points.

use super::*;

/// Records a parameter definition with its type and name, and returns its property set.
pub(super) unsafe extern "C" fn param_define(
    set: OfxParamSetHandle,
    kind: *const c_char,
    name: *const c_char,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    if set as usize != with(|s| s.param_set) {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: the plugin passes valid NUL-terminated strings.
    let (kind, name) = unsafe {
        (
            CStr::from_ptr(kind).to_string_lossy().into_owned(),
            CStr::from_ptr(name).to_string_lossy().into_owned(),
        )
    };
    let props = with(|s| {
        if s.params.iter().any(|p| p.name == name) {
            return None;
        }
        let handle = s.alloc();
        let props = s.alloc();
        s.params.push(ParamRec {
            name,
            kind,
            handle,
            props,
        });
        Some(props)
    });
    match props {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Some(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        None => OFX_STAT_ERR_EXISTS,
    }
}

/// Returns the stored parameter index and its property set for a name, or `None` if it is not defined.
pub(super) fn param_by_name(name: &str) -> Option<(usize, usize)> {
    with(|s| {
        s.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| (p.handle, p.props))
    })
}

/// Returns the handle and property set of a defined parameter by name.
pub(super) unsafe extern "C" fn param_get_handle(
    set: OfxParamSetHandle,
    name: *const c_char,
    param_out: *mut OfxParamHandle,
    props_out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    if set as usize != with(|s| s.param_set) {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: valid NUL-terminated name per the plugin's call.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    let Some((handle, props)) = param_by_name(&name) else {
        return OFX_STAT_ERR_UNKNOWN;
    };
    // SAFETY: both out pointers are valid per the host contract.
    unsafe {
        let status = put(param_out, handle as OfxParamHandle);
        if status != OFX_STAT_OK {
            return status;
        }
        put(props_out, props as OfxPropertySetHandle)
    }
}

/// Returns the property set of a parameter set handle.
pub(super) unsafe extern "C" fn param_set_get_property_set(
    set: OfxParamSetHandle,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let props = with(|s| (set as usize == s.param_set).then_some(s.param_set_props));
    match props {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Some(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

/// Returns the property set of a parameter handle.
pub(super) unsafe extern "C" fn param_get_property_set(
    param: OfxParamHandle,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let props = with(|s| {
        s.params
            .iter()
            .find(|p| p.handle == param as usize)
            .map(|p| p.props)
    });
    match props {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Some(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

/// Property name under which a test stores a parameter value (not an OFX property).
pub(super) const MOCK_VALUE: &str = "mock.value";

/// Writes the parameter's current value through `out`, typed by the stored value.
///
/// The mock's current value is the table default the plugin stored in
/// `kOfxParamPropDefault`. A double is written as `f64`, an int or choice as `c_int`.
///
/// # Safety
///
/// `out` must be a valid pointer for the type the stored value has.
pub(super) unsafe fn read_param(param: OfxParamHandle, out: *mut c_void) -> OfxStatus {
    // A string is handed out as the address of its stored buffer, which outlives this call.
    let stored = |v: &Value| match v {
        Value::Str(c) => Value::Ptr(c.as_ptr() as usize),
        other => other.clone(),
    };
    let value = with(|s| {
        let rec = s.params.iter().find(|p| p.handle == param as usize)?;
        // A value set by the test wins over the table default.
        let overridden = s
            .props
            .get(&(rec.props, MOCK_VALUE.to_string()))
            .and_then(|values| values.first().map(stored));
        overridden.or_else(|| {
            s.props
                .get(&(
                    rec.props,
                    OFX_PARAM_PROP_DEFAULT.to_string_lossy().into_owned(),
                ))
                .and_then(|values| values.first().map(stored))
        })
    });
    match value {
        Some(Value::Double(d)) if !out.is_null() => {
            // SAFETY: the plugin passes a `*mut f64` for double parameters.
            unsafe { out.cast::<f64>().write(d) };
            OFX_STAT_OK
        }
        Some(Value::Int(i)) if !out.is_null() => {
            // SAFETY: the plugin passes a `*mut c_int` for integer and choice parameters.
            unsafe { out.cast::<c_int>().write(i) };
            OFX_STAT_OK
        }
        Some(Value::Ptr(p)) if !out.is_null() => {
            // SAFETY: the plugin passes a `*const c_char *` for string parameters. The address
            // is the buffer of a stored string, which lives until the test ends.
            unsafe { out.cast::<*const c_char>().write(p as *const c_char) };
            OFX_STAT_OK
        }
        Some(Value::NullStr) if !out.is_null() => {
            // SAFETY: as above; a null string is what the host reports for an empty value.
            unsafe { out.cast::<*const c_char>().write(ptr::null()) };
            OFX_STAT_OK
        }
        Some(_) => OFX_STAT_ERR_VALUE,
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

/// `paramGetValueAtTime`: `(OfxParamHandle, OfxTime, ...)`, one trailing pointer.
#[cfg(not(all(target_vendor = "apple", target_arch = "aarch64")))]
pub(super) unsafe extern "C" fn get_value_at_time(
    param: OfxParamHandle,
    _time: OfxTime,
    out: *mut c_void,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: forwarded from the host contract.
    unsafe { read_param(param, out) }
}

/// Apple arm64: the trailing pointer is the first stack argument. Seven integer parameters
/// fill `x1..x7` so that `out` is the ninth integer argument.
#[cfg(all(target_vendor = "apple", target_arch = "aarch64"))]
pub(super) unsafe extern "C" fn get_value_at_time(
    param: OfxParamHandle,
    _time: OfxTime,
    _x1: usize,
    _x2: usize,
    _x3: usize,
    _x4: usize,
    _x5: usize,
    _x6: usize,
    _x7: usize,
    out: *mut c_void,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: forwarded from the host contract.
    unsafe { read_param(param, out) }
}

/// `paramGetValue`: `(OfxParamHandle, ...)`, one trailing pointer, no time.
#[cfg(not(all(target_vendor = "apple", target_arch = "aarch64")))]
pub(super) unsafe extern "C" fn get_value(param: OfxParamHandle, out: *mut c_void) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: forwarded from the host contract.
    unsafe { read_param(param, out) }
}

/// Apple arm64 variant of `paramGetValue`; see [`get_value_at_time`].
#[cfg(all(target_vendor = "apple", target_arch = "aarch64"))]
pub(super) unsafe extern "C" fn get_value(
    param: OfxParamHandle,
    _x1: usize,
    _x2: usize,
    _x3: usize,
    _x4: usize,
    _x5: usize,
    _x6: usize,
    _x7: usize,
    out: *mut c_void,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // SAFETY: forwarded from the host contract.
    unsafe { read_param(param, out) }
}

/// Variadic slots the plugin never calls. The stub returns `kOfxStatFailed` and reads nothing.
pub(super) unsafe extern "C" fn variadic_failed(_param: OfxParamHandle) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    OFX_STAT_FAILED
}

/// Returns the mock parameter suite table, built once and shared.
pub(super) fn parameter_suite() -> &'static OfxParameterSuiteV1 {
    static SUITE: OnceLock<OfxParameterSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| {
        // SAFETY: each slot is a variadic function pointer type. The stored functions are
        // either the receivers above, which read exactly the trailing pointer the caller
        // passes (see the module documentation), or stubs that read nothing. Pointers to
        // functions have the same size as each other, so the transmutes are size-correct.
        unsafe {
            OfxParameterSuiteV1 {
                paramDefine: param_define,
                paramGetHandle: param_get_handle,
                paramSetGetPropertySet: param_set_get_property_set,
                paramGetPropertySet: param_get_property_set,
                paramGetValue: transmute::<*const (), _>(get_value as *const ()),
                paramGetValueAtTime: transmute::<*const (), _>(get_value_at_time as *const ()),
                paramGetDerivative: transmute::<*const (), _>(variadic_failed as *const ()),
                paramGetIntegral: transmute::<*const (), _>(variadic_failed as *const ()),
                paramSetValue: transmute::<*const (), _>(variadic_failed as *const ()),
                paramSetValueAtTime: transmute::<*const (), _>(variadic_failed as *const ()),
                paramGetNumKeys: param_num_keys,
                paramGetKeyTime: param_key_time,
                paramGetKeyIndex: param_key_index,
                paramDeleteKey: param_delete_key,
                paramDeleteAllKeys: param_delete_all_keys,
                paramCopy: param_copy,
                paramEditBegin: param_edit_begin,
                paramEditEnd: param_edit_end,
            }
        }
    })
}
