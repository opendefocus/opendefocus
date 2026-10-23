//! Clip-level OFX helpers used by the render path: fetching clip handles, the render window and the
//! Source region of definition, and reading four-value properties.

use super::{
    HostResult, HostSuites, OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW, OFX_STAT_OK, OfxImageClipHandle,
    OfxImageEffectHandle, OfxPropertySetHandle, OfxRectD, OfxTime, Outcome, Props, Rect, fail,
};

/// Reads the render window (four ints) from the in-args.
pub(super) fn render_window(in_props: &Props<'_>) -> HostResult<Rect> {
    let mut values = [0; 4];
    for (i, value) in values.iter_mut().enumerate() {
        *value = in_props.get_int(OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW, i as i32)?;
    }
    Ok(Rect::from_ints(values))
}
/// Reads four integer values from the named property, in index order.
pub(super) fn read_ints(props: &Props<'_>, name: &core::ffi::CStr) -> HostResult<[i32; 4]> {
    let mut values = [0; 4];
    for (i, value) in values.iter_mut().enumerate() {
        *value = props.get_int(name, i as i32)?;
    }
    Ok(values)
}
/// Returns a clip's handle and property set. `name` must be NUL terminated.
pub(super) fn clip_handles(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
    name: &core::ffi::CStr,
) -> Outcome<(OfxImageClipHandle, OfxPropertySetHandle)> {
    let mut clip: OfxImageClipHandle = core::ptr::null_mut();
    let mut props: OfxPropertySetHandle = core::ptr::null_mut();
    // SAFETY: `effect` is valid for this action; the name is NUL terminated; out pointers are valid.
    crate::suites::check(unsafe {
        (suites.image_effect.clipGetHandle)(effect, name.as_ptr(), &raw mut clip, &raw mut props)
    })?;
    Ok((clip, props))
}
/// Reads four double values from the named property, in index order.
pub(super) fn read_doubles(props: &Props<'_>, name: &core::ffi::CStr) -> HostResult<[f64; 4]> {
    let mut values = [0.0; 4];
    for (i, value) in values.iter_mut().enumerate() {
        *value = props.get_double(name, i as i32)?;
    }
    Ok(values)
}

/// The Source region of definition in canonical coordinates, from `clipGetRegionOfDefinition`.
pub(super) fn clip_region_of_definition(
    suites: &HostSuites,
    clip: OfxImageClipHandle,
    time: OfxTime,
) -> Outcome<[f64; 4]> {
    let mut rod = OfxRectD {
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
    };
    // SAFETY: `clip` came from clipGetHandle in this action; `rod` is a valid out pointer.
    let status =
        unsafe { (suites.image_effect.clipGetRegionOfDefinition)(clip, time, &raw mut rod) };
    if status != OFX_STAT_OK {
        return fail(format!(
            "Source region of definition is unavailable (status {status})"
        ));
    }
    Ok([rod.x1, rod.y1, rod.x2, rod.y2])
}
