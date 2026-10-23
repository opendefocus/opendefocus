//! The `GetRegionsOfInterest` action: maps the host's requested output window to the padded Source
//! (and Depth) window the renderer needs, in canonical coordinates.

use super::{
    CLIP_DEPTH, CLIP_SOURCE, Failure, OFX_IMAGE_CLIP_PROP_CONNECTED,
    OFX_IMAGE_CLIP_PROP_ROI_PREFIX, OFX_IMAGE_EFFECT_PROP_REGION_OF_INTEREST, OFX_PROP_TIME,
    OFX_STAT_FAILED, OFX_STAT_OK, OfxImageEffectHandle, OfxPropertySetHandle, OfxStatus, Outcome,
    PixelMap, Props, Rect, c_string, clip_handles, clip_pixel_aspect, clip_region_of_definition,
    fail, instance_mut, read_doubles, read_effect_settings, reads_remote_source, render_scale,
    report, set_frame, source_request, stored_suites,
};

/// Writes one clip's `RoI` into the `GetRegionsOfInterest` out-args, in canonical coordinates.
pub(super) fn set_roi(out: &Props<'_>, clip: &str, map: PixelMap, rect: Rect) -> Outcome<()> {
    let name = c_string(&format!("{OFX_IMAGE_CLIP_PROP_ROI_PREFIX}{clip}"))?;
    for (i, value) in map.to_canonical(rect).into_iter().enumerate() {
        out.set_double(&name, i as i32, value)?;
    }
    Ok(())
}
/// The `GetRegionsOfInterest` action. Source and Depth both request the same padded window (see
/// [`source_request`]). Returns `kOfxStatFailed` on any error; never unwinds.
pub fn regions_of_interest(
    effect: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    match try_regions_of_interest(effect, in_args, out_args) {
        Ok(()) => OFX_STAT_OK,
        Err(Failure::Aborted) => OFX_STAT_FAILED,
        Err(Failure::Message(text)) => {
            report(effect, &text);
            OFX_STAT_FAILED
        }
    }
}

/// Computes the Source and Depth regions of interest for one call and writes them to the out-args. A zero-size request leaves the host's default in place.
pub(super) fn try_regions_of_interest(
    effect: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> Outcome<()> {
    if in_args.is_null() || out_args.is_null() {
        return fail("regions of interest arguments are missing");
    }
    let suites = stored_suites()?;
    let in_props = Props::new(in_args, suites.property);
    let out_props = Props::new(out_args, suites.property);
    let time = in_props.get_double(OFX_PROP_TIME, 0)?;
    let (source_clip, source_props) = clip_handles(&suites, effect, CLIP_SOURCE)?;
    let (scale_x, scale_y) = render_scale(&in_props);
    let map = PixelMap {
        scale_x,
        scale_y,
        par: clip_pixel_aspect(source_props, &suites),
    };
    let roi = map.to_pixels(read_doubles(
        &in_props,
        OFX_IMAGE_EFFECT_PROP_REGION_OF_INTEREST,
    )?);
    if roi.width() == 0 || roi.height() == 0 {
        return Ok(());
    }
    let instance = instance_mut(&suites, effect)?;
    let rod = map.to_pixels(clip_region_of_definition(&suites, source_clip, time)?);
    // SAFETY: `effect` and `in_props` are this action's handles, valid for the call.
    let mut settings = unsafe { read_effect_settings(&suites, effect, &in_props, time) }?;
    set_frame(&mut settings, rod);
    let renderer = instance.renderer(&settings)?;
    let request = source_request(
        roi,
        rod,
        renderer.padding(&settings),
        reads_remote_source(&settings),
    );
    set_roi(&out_props, "Source", map, request)?;
    let (_, depth_props) = clip_handles(&suites, effect, CLIP_DEPTH)?;
    if Props::new(depth_props, suites.property).get_int(OFX_IMAGE_CLIP_PROP_CONNECTED, 0)? != 0 {
        set_roi(&out_props, "Depth", map, request)?;
    }
    Ok(())
}
