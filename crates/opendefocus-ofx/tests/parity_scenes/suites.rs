//! Mock suites for the parity-scene host: property, image-effect and parameter entries that serve the scene's clip images and values.

use super::*;

pub(super) unsafe extern "C" fn prop_set_pointer(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut c_void,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    with(|h| h.store(props as usize, name, index, Stored::Ptr(value as usize)))
}

pub(super) unsafe extern "C" fn prop_set_string(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *const c_char,
) -> OfxStatus {
    // SAFETY: the plugin passes valid NUL-terminated strings.
    let (name, value) = unsafe { (name_of(name), CStr::from_ptr(value).to_owned()) };
    with(|h| h.store(props as usize, name, index, Stored::Str(value)))
}

pub(super) unsafe extern "C" fn prop_set_double(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: f64,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    with(|h| h.store(props as usize, name, index, Stored::Double(value)))
}

pub(super) unsafe extern "C" fn prop_set_int(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: c_int,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    with(|h| h.store(props as usize, name, index, Stored::Int(value)))
}

pub(super) unsafe extern "C" fn prop_get_pointer(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut *mut c_void,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    if value.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    match with(|h| h.fetch(props as usize, &name, index).cloned()) {
        Some(Stored::Ptr(p)) => {
            // SAFETY: `value` is a valid out pointer for a pointer, per the host contract.
            unsafe { *value = p as *mut c_void };
            OFX_STAT_OK
        }
        _ => OFX_STAT_ERR_VALUE,
    }
}

pub(super) unsafe extern "C" fn prop_get_string(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut *mut c_char,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    if value.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    // The pointer refers into the stored CString, which stays in the map for the whole action.
    let found = with(|h| match h.fetch(props as usize, &name, index) {
        Some(Stored::Str(s)) => Some(s.as_ptr().cast_mut()),
        _ => None,
    });
    match found {
        Some(p) => {
            // SAFETY: `value` is a valid out pointer for a string pointer.
            unsafe { *value = p };
            OFX_STAT_OK
        }
        None => OFX_STAT_ERR_VALUE,
    }
}

pub(super) unsafe extern "C" fn prop_get_double(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut f64,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    if value.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    match with(|h| h.fetch(props as usize, &name, index).cloned()) {
        Some(Stored::Double(d)) => {
            // SAFETY: `value` is a valid out pointer for a double.
            unsafe { *value = d };
            OFX_STAT_OK
        }
        _ => OFX_STAT_ERR_VALUE,
    }
}

pub(super) unsafe extern "C" fn prop_get_int(
    props: OfxPropertySetHandle,
    name: *const c_char,
    index: c_int,
    value: *mut c_int,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid property name.
    let name = unsafe { name_of(name) };
    if value.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    match with(|h| h.fetch(props as usize, &name, index).cloned()) {
        Some(Stored::Int(i)) => {
            // SAFETY: `value` is a valid out pointer for an int.
            unsafe { *value = i };
            OFX_STAT_OK
        }
        _ => OFX_STAT_ERR_VALUE,
    }
}

pub(super) fn property_suite() -> &'static OfxPropertySuiteV1 {
    static SUITE: OnceLock<OfxPropertySuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxPropertySuiteV1 {
        propSetPointer: prop_set_pointer,
        propSetString: prop_set_string,
        propSetDouble: prop_set_double,
        propSetInt: prop_set_int,
        propSetPointerN: stub(),
        propSetStringN: stub(),
        propSetDoubleN: stub(),
        propSetIntN: stub(),
        propGetPointer: prop_get_pointer,
        propGetString: prop_get_string,
        propGetDouble: prop_get_double,
        propGetInt: prop_get_int,
        propGetPointerN: stub(),
        propGetStringN: stub(),
        propGetDoubleN: stub(),
        propGetIntN: stub(),
        propReset: stub(),
        propGetDimension: stub(),
    })
}

pub(super) unsafe extern "C" fn get_property_set(
    effect: OfxImageEffectHandle,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    let props = with(|h| (effect as usize == h.effect).then_some(h.effect_props));
    match (props, out.is_null()) {
        (Some(props), false) => {
            // SAFETY: `out` is a valid out pointer per the host contract.
            unsafe { *out = props as OfxPropertySetHandle };
            OFX_STAT_OK
        }
        (None, _) => OFX_STAT_ERR_BAD_HANDLE,
        _ => OFX_STAT_ERR_VALUE,
    }
}

pub(super) unsafe extern "C" fn get_param_set(
    effect: OfxImageEffectHandle,
    out: *mut OfxParamSetHandle,
) -> OfxStatus {
    let set = with(|h| (effect as usize == h.effect).then_some(h.param_set));
    match (set, out.is_null()) {
        (Some(set), false) => {
            // SAFETY: `out` is a valid out pointer per the host contract.
            unsafe { *out = set as OfxParamSetHandle };
            OFX_STAT_OK
        }
        (None, _) => OFX_STAT_ERR_BAD_HANDLE,
        _ => OFX_STAT_ERR_VALUE,
    }
}

pub(super) unsafe extern "C" fn clip_get_handle(
    effect: OfxImageEffectHandle,
    name: *const c_char,
    clip_out: *mut OfxImageClipHandle,
    props_out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid clip name.
    let name = unsafe { name_of(name) };
    if effect as usize != with(|h| h.effect) {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    if clip_out.is_null() || props_out.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    let found = with(|h| {
        h.clips
            .iter()
            .find(|clip| clip.name == name)
            .map(|clip| (clip.handle, clip.props))
    });
    match found {
        Some((handle, props)) => {
            // SAFETY: both out pointers are valid per the host contract.
            unsafe {
                *clip_out = handle as OfxImageClipHandle;
                *props_out = props as OfxPropertySetHandle;
            }
            OFX_STAT_OK
        }
        None => OFX_STAT_ERR_UNKNOWN,
    }
}

pub(super) unsafe extern "C" fn clip_get_image(
    clip: OfxImageClipHandle,
    _time: OfxTime,
    _region: *const OfxRectD,
    image_out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if image_out.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    let image = with(|h| {
        let index = h.clips.iter().position(|c| c.handle == clip as usize)?;
        if !h.clips[index].connected {
            return None;
        }
        let (width, height) = (h.clips[index].width, h.clips[index].height);
        let data = h.clips[index].pixels.as_mut_ptr() as usize;
        let props = h.alloc();
        let row_bytes = width * 4 * c_int::try_from(std::mem::size_of::<f32>()).expect("size");
        h.store(
            props,
            OFX_IMAGE_PROP_DATA.to_string_lossy().into_owned(),
            0,
            Stored::Ptr(data),
        );
        let name_row = OFX_IMAGE_PROP_ROW_BYTES.to_string_lossy().into_owned();
        h.store(props, name_row, 0, Stored::Int(row_bytes));
        let bounds = [0, 0, width, height];
        let rod = [0, 0, width, height];
        for (i, (b, r)) in bounds.iter().zip(rod.iter()).enumerate() {
            let index = c_int::try_from(i).expect("index");
            h.store(
                props,
                OFX_IMAGE_PROP_BOUNDS.to_string_lossy().into_owned(),
                index,
                Stored::Int(*b),
            );
            h.store(
                props,
                OFX_IMAGE_PROP_REGION_OF_DEFINITION
                    .to_string_lossy()
                    .into_owned(),
                index,
                Stored::Int(*r),
            );
        }
        let depth = CString::new(OFX_BIT_DEPTH_FLOAT.to_bytes()).expect("depth");
        h.store(
            props,
            OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH
                .to_string_lossy()
                .into_owned(),
            0,
            Stored::Str(depth),
        );
        let components = CString::new(OFX_IMAGE_COMPONENT_RGBA.to_bytes()).expect("components");
        h.store(
            props,
            OFX_IMAGE_EFFECT_PROP_COMPONENTS
                .to_string_lossy()
                .into_owned(),
            0,
            Stored::Str(components),
        );
        Some(props)
    });
    match image {
        Some(props) => {
            // SAFETY: `image_out` is a valid out pointer per the host contract.
            unsafe { *image_out = props as OfxPropertySetHandle };
            OFX_STAT_OK
        }
        None => OFX_STAT_FAILED,
    }
}

pub(super) unsafe extern "C" fn clip_release_image(_image: OfxPropertySetHandle) -> OfxStatus {
    OFX_STAT_OK
}

pub(super) unsafe extern "C" fn clip_get_region_of_definition(
    clip: OfxImageClipHandle,
    _time: OfxTime,
    rod: *mut OfxRectD,
) -> OfxStatus {
    if rod.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    let size = with(|h| {
        h.clips
            .iter()
            .find(|c| c.handle == clip as usize)
            .map(|c| (c.width, c.height))
    });
    match size {
        Some((width, height)) => {
            // SAFETY: `rod` is a valid out pointer per the host contract.
            unsafe {
                *rod = OfxRectD {
                    x1: 0.0,
                    y1: 0.0,
                    x2: f64::from(width),
                    y2: f64::from(height),
                };
            }
            OFX_STAT_OK
        }
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

pub(super) unsafe extern "C" fn abort(_effect: OfxImageEffectHandle) -> c_int {
    0
}

pub(super) fn image_effect_suite() -> &'static OfxImageEffectSuiteV1 {
    static SUITE: OnceLock<OfxImageEffectSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxImageEffectSuiteV1 {
        getPropertySet: get_property_set,
        getParamSet: get_param_set,
        clipDefine: stub(),
        clipGetHandle: clip_get_handle,
        clipGetPropertySet: stub(),
        clipGetImage: clip_get_image,
        clipReleaseImage: clip_release_image,
        clipGetRegionOfDefinition: clip_get_region_of_definition,
        abort,
        imageMemoryAlloc: stub(),
        imageMemoryFree: stub(),
        imageMemoryLock: stub(),
        imageMemoryUnlock: stub(),
    })
}

pub(super) unsafe extern "C" fn param_get_handle(
    set: OfxParamSetHandle,
    name: *const c_char,
    param_out: *mut OfxParamHandle,
    props_out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    // SAFETY: the plugin passes a valid parameter name.
    let name = unsafe { name_of(name) };
    if param_out.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    let Some(def) = PARAMS.iter().find(|def| def.name == name) else {
        return OFX_STAT_ERR_UNKNOWN;
    };
    let handle = with(|h| {
        if set as usize != h.param_set {
            return None;
        }
        let handle = h.alloc();
        h.param_handles.insert(handle, def.name);
        Some((handle, h.alloc()))
    });
    match handle {
        Some((handle, props)) => {
            // SAFETY: the out pointers are valid per the host contract.
            unsafe {
                *param_out = handle as OfxParamHandle;
                if !props_out.is_null() {
                    *props_out = props as OfxPropertySetHandle;
                }
            }
            OFX_STAT_OK
        }
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

/// Writes a parameter value through the variadic pointer. On Apple arm64 the variadic argument
/// is on the stack after x0..x7, so seven unused integer parameters push `value` there.
#[cfg(all(target_vendor = "apple", target_arch = "aarch64"))]
pub(super) unsafe extern "C" fn param_get_value_at_time(
    param: OfxParamHandle,
    time: OfxTime,
    _x1: usize,
    _x2: usize,
    _x3: usize,
    _x4: usize,
    _x5: usize,
    _x6: usize,
    _x7: usize,
    value: *mut c_void,
) -> OfxStatus {
    let _ = time;
    read_param(param, value)
}

/// On x86-64 and other ABIs the variadic argument is passed like a named one.
#[cfg(not(all(target_vendor = "apple", target_arch = "aarch64")))]
pub(super) unsafe extern "C" fn param_get_value_at_time(
    param: OfxParamHandle,
    time: OfxTime,
    value: *mut c_void,
) -> OfxStatus {
    let _ = time;
    read_param(param, value)
}

pub(super) fn read_param(param: OfxParamHandle, value: *mut c_void) -> OfxStatus {
    if value.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    let found = with(|h| {
        let name = h.param_handles.get(&(param as usize))?;
        h.values.get(name).copied()
    });
    let Some(found) = found else {
        return OFX_STAT_ERR_BAD_HANDLE;
    };
    match found {
        // SAFETY: the plugin passes a `double *` for double parameters, per the header.
        ParamValue::Double(d) => unsafe { *value.cast::<f64>() = d },
        // SAFETY: the plugin passes an `int *` for integer and choice parameters.
        ParamValue::Int(i) | ParamValue::Choice(i) => unsafe { *value.cast::<c_int>() = i },
        // SAFETY: booleans are passed as `int`, per the header.
        ParamValue::Bool(b) => unsafe { *value.cast::<c_int>() = c_int::from(b) },
    }
    OFX_STAT_OK
}

pub(super) fn parameter_suite() -> &'static OfxParameterSuiteV1 {
    #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))]
    type ValueAtTime = unsafe extern "C" fn(
        OfxParamHandle,
        OfxTime,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        *mut c_void,
    ) -> OfxStatus;
    #[cfg(not(all(target_vendor = "apple", target_arch = "aarch64")))]
    type ValueAtTime = unsafe extern "C" fn(OfxParamHandle, OfxTime, *mut c_void) -> OfxStatus;

    static SUITE: OnceLock<OfxParameterSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| {
        let value_at_time: ValueAtTime = param_get_value_at_time;
        OfxParameterSuiteV1 {
            paramDefine: stub(),
            paramGetHandle: param_get_handle,
            paramSetGetPropertySet: stub(),
            paramGetPropertySet: stub(),
            paramGetValue: stub(),
            // SAFETY: both types are plain function pointers of the same size. The variadic
            // type is the header's declaration; the receiver reads the one trailing pointer
            // that the plugin passes, which is where the target ABI puts it.
            paramGetValueAtTime: unsafe { std::mem::transmute(value_at_time) },
            paramGetDerivative: stub(),
            paramGetIntegral: stub(),
            paramSetValue: stub(),
            paramSetValueAtTime: stub(),
            paramGetNumKeys: stub(),
            paramGetKeyTime: stub(),
            paramGetKeyIndex: stub(),
            paramDeleteKey: stub(),
            paramDeleteAllKeys: stub(),
            paramCopy: stub(),
            paramEditBegin: stub(),
            paramEditEnd: stub(),
        }
    })
}
