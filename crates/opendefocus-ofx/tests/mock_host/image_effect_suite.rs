//! Mock image-effect suite: effect and clip property sets, clip images, regions of definition, image release and the abort query.

use super::*;

/// Writes the property set of an effect handle through the image-effect suite.
pub(super) unsafe extern "C" fn get_property_set(
    effect: OfxImageEffectHandle,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let (ok, props) = with(|s| (effect as usize == s.effect, s.effect_props));
    if !ok {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: `out` is a valid out pointer per the host contract.
    unsafe { put(out, props as OfxPropertySetHandle) }
}

/// Writes the parameter set of an effect handle through the image-effect suite.
pub(super) unsafe extern "C" fn get_param_set(
    effect: OfxImageEffectHandle,
    out: *mut OfxParamSetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let (ok, set) = with(|s| (effect as usize == s.effect, s.param_set));
    if !ok {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: `out` is a valid out pointer per the host contract.
    unsafe { put(out, set as OfxParamSetHandle) }
}

/// Defines a clip on the effect and returns its property set, recording the clip for later lookup.
pub(super) unsafe extern "C" fn clip_define(
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
    // SAFETY: the plugin passes a valid NUL-terminated clip name.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    let props = with(|s| {
        if s.clips.iter().any(|c| c.name == name) {
            return None;
        }
        let handle = s.alloc();
        let props = s.alloc();
        s.clips.push(Clip {
            name,
            handle,
            props,
            image: None,
        });
        Some(props)
    });
    match props {
        Some(p) => {
            // An unconnected clip reports 0; `set_clip_image` connects it.
            store(
                (
                    p,
                    OFX_IMAGE_CLIP_PROP_CONNECTED.to_string_lossy().into_owned(),
                ),
                0,
                Value::Int(0),
            );
            // SAFETY: `out` is a valid out pointer per the host contract.
            unsafe { put(out, p as OfxPropertySetHandle) }
        }
        None => OFX_STAT_ERR_EXISTS,
    }
}

/// Returns the stored clip index and its property set for a clip name, or `None` if the clip is not defined.
pub(super) fn find_clip_by_name(name: &str) -> Option<(usize, usize)> {
    with(|s| {
        s.clips
            .iter()
            .find(|c| c.name == name)
            .map(|c| (c.handle, c.props))
    })
}

/// Returns the handle and property set of a defined clip by name.
pub(super) unsafe extern "C" fn clip_get_handle(
    effect: OfxImageEffectHandle,
    name: *const c_char,
    clip_out: *mut OfxImageClipHandle,
    props_out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    if effect as usize != with(|s| s.effect) {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: valid NUL-terminated clip name per the plugin's call.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    let Some((handle, props)) = find_clip_by_name(&name) else {
        return OFX_STAT_ERR_UNKNOWN;
    };
    // SAFETY: both out pointers are valid per the host contract.
    unsafe {
        let status = put(clip_out, handle as OfxImageClipHandle);
        if status != OFX_STAT_OK {
            return status;
        }
        put(props_out, props as OfxPropertySetHandle)
    }
}

/// Returns the property set of a clip handle.
pub(super) unsafe extern "C" fn clip_get_property_set(
    clip: OfxImageClipHandle,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let props = with(|s| {
        s.clips
            .iter()
            .find(|c| c.handle == clip as usize)
            .map(|c| c.props)
    });
    match props {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Some(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        None => OFX_STAT_ERR_BAD_HANDLE,
    }
}

/// Returns an image for a clip at a time, or fails if the clip has no image stored.
pub(super) unsafe extern "C" fn clip_get_image(
    clip: OfxImageClipHandle,
    _time: OfxTime,
    _region: *const OfxRectD,
    out: *mut OfxPropertySetHandle,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // The image is a property set with the data pointer, row bytes, bounds, region of
    // definition, pixel depth and components, as the header defines them.
    let image = with(|s| {
        let image_props = {
            let clip = s.clips.iter().find(|c| c.handle == clip as usize)?;
            let image = clip.image.as_ref()?;
            let stride = (image.stride * 4) as c_int;
            let base = image.physical.as_ptr() as usize;
            let height = image.height();
            let (data, row_bytes) = if image.negative_row_bytes {
                (base + (height.max(1) - 1) * stride as usize, -stride)
            } else {
                (base, stride)
            };
            let components = if image.channels == 4 {
                "OfxImageComponentRGBA"
            } else {
                "OfxImageComponentAlpha"
            };
            (data, row_bytes, image.bounds, image.rod, components)
        };
        let (data, row_bytes, bounds, rod, components) = image_props;
        let depth = if s.byte_depth {
            "OfxBitDepthByte"
        } else {
            "OfxBitDepthFloat"
        };
        let props = s.alloc();
        s.images_fetched += 1;
        let mut put_prop = |name: &CStr, values: Vec<Value>| {
            s.props
                .insert((props, name.to_string_lossy().into_owned()), values);
        };
        put_prop(OFX_IMAGE_PROP_DATA, vec![Value::Ptr(data)]);
        put_prop(OFX_IMAGE_PROP_ROW_BYTES, vec![Value::Int(row_bytes)]);
        put_prop(OFX_IMAGE_PROP_BOUNDS, bounds.map(Value::Int).to_vec());
        put_prop(
            OFX_IMAGE_PROP_REGION_OF_DEFINITION,
            rod.map(Value::Int).to_vec(),
        );
        put_prop(
            OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH,
            vec![Value::Str(CString::new(depth).expect("no NUL"))],
        );
        put_prop(
            OFX_IMAGE_EFFECT_PROP_COMPONENTS,
            vec![Value::Str(CString::new(components).expect("no NUL"))],
        );
        Some(props)
    });
    match image {
        // SAFETY: `out` is a valid out pointer per the host contract.
        Some(p) => unsafe { put(out, p as OfxPropertySetHandle) },
        None => OFX_STAT_FAILED,
    }
}

/// Writes the region of definition of a clip image into the caller's rectangle.
pub(super) unsafe extern "C" fn image_clip_region(
    clip: OfxImageClipHandle,
    _time: OfxTime,
    bounds: *mut OfxRectD,
) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    // The image's region of definition is in pixels; the clip's is in canonical coordinates.
    let (scale, par) = with(|s| (s.render_scale, s.pixel_aspect));
    let rod = with(|s| {
        s.clips
            .iter()
            .find(|c| c.handle == clip as usize)
            .and_then(|c| c.image.as_ref())
            .map(|image| image.rod)
    });
    match rod {
        // SAFETY: `bounds` is a valid out pointer per the host contract.
        Some(r) => unsafe {
            put(
                bounds,
                OfxRectD {
                    x1: f64::from(r[0]) * par / scale[0],
                    y1: f64::from(r[1]) / scale[1],
                    x2: f64::from(r[2]) * par / scale[0],
                    y2: f64::from(r[3]) / scale[1],
                },
            )
        },
        None => OFX_STAT_FAILED,
    }
}

/// Releases an image property set that was returned by clipGetImage and counts the release.
pub(super) unsafe extern "C" fn clip_release_image(image: OfxPropertySetHandle) -> OfxStatus {
    if let Some(status) = injected_fault() {
        return status;
    }
    let _ = image;
    with(|s| s.images_released += 1);
    OFX_STAT_OK
}

/// Reports whether the host asks the effect to abort, as set by [`Session::request_abort`].
pub(super) unsafe extern "C" fn abort(_effect: OfxImageEffectHandle) -> c_int {
    c_int::from(with(|s| s.abort_requested))
}

/// Returns the mock image-effect suite table, built once and shared.
pub(super) fn image_effect_suite() -> &'static OfxImageEffectSuiteV1 {
    static SUITE: OnceLock<OfxImageEffectSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxImageEffectSuiteV1 {
        getPropertySet: get_property_set,
        getParamSet: get_param_set,
        clipDefine: clip_define,
        clipGetHandle: clip_get_handle,
        clipGetPropertySet: clip_get_property_set,
        clipGetImage: clip_get_image,
        clipReleaseImage: clip_release_image,
        clipGetRegionOfDefinition: image_clip_region,
        abort,
        imageMemoryAlloc: image_memory_alloc,
        imageMemoryFree: image_memory_free,
        imageMemoryLock: image_memory_lock,
        imageMemoryUnlock: image_memory_unlock,
    })
}
