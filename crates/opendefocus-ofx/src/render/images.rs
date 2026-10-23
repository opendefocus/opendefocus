//! Image fetching, checking and copying for the render path. Images are read from and written to the
//! host buffers through their row strides, and each fetched image is released by a guard.

use super::{
    Array3, Axis, BIT_DEPTH_FLOAT, COMPONENTS_ALPHA, COMPONENTS_RGBA, Failure, HostSuites,
    OFX_IMAGE_EFFECT_PROP_COMPONENTS, OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH, OFX_IMAGE_PROP_BOUNDS,
    OFX_IMAGE_PROP_DATA, OFX_IMAGE_PROP_REGION_OF_DEFINITION, OFX_IMAGE_PROP_ROW_BYTES,
    OFX_STAT_OK, OfxImageClipHandle, OfxImageEffectHandle, OfxPropertySetHandle, OfxTime, Outcome,
    Props, Rect, fail, read_ints, rect_text,
};

/// Releases a fetched image when dropped, so every path out of a render releases it once.
pub(super) struct FetchedImage<'a> {
    /// Host suites used to release the image.
    pub(super) suites: &'a HostSuites,
    /// Image property set returned by `clipGetImage`.
    pub(super) handle: OfxPropertySetHandle,
}

impl Drop for FetchedImage<'_> {
    fn drop(&mut self) {
        // SAFETY: `handle` was returned by a successful clipGetImage in this action, and it is
        // released exactly once here. The host requires release before the action returns.
        let status = unsafe { (self.suites.image_effect.clipReleaseImage)(self.handle) };
        if status != OFX_STAT_OK {
            crate::suites::diagnostic(&format!("clipReleaseImage failed with status {status}"));
        }
    }
}

/// A fetched image, its layout, and the guard that releases it.
pub(super) struct Image<'a> {
    /// Releases the image when the `Image` is dropped.
    pub(super) _guard: FetchedImage<'a>,
    /// Address of the lower-left pixel of `bounds`, per kOfxImagePropData.
    pub(super) data: *mut u8,
    /// Byte distance between consecutive rows; negative when the host stores rows in reverse.
    pub(super) row_bytes: isize,
    /// Pixel rectangle of the data the host allocated for this image.
    pub(super) bounds: Rect,
    /// Region the clip defines, in pixels.
    pub(super) region_of_definition: Rect,
    /// OFX pixel depth name, such as `OfxBitDepthFloat`.
    pub(super) pixel_depth: String,
    /// OFX component name, such as `OfxImageComponentRGBA`.
    pub(super) components: String,
}
/// Asks the host whether the render should stop. Returns `Err(Failure::Aborted)` when it should.
pub(super) fn check_abort(suites: &HostSuites, effect: OfxImageEffectHandle) -> Outcome<()> {
    // SAFETY: `effect` is the handle the host passed to this render action.
    if unsafe { (suites.image_effect.abort)(effect) } != 0 {
        return Err(Failure::Aborted);
    }
    Ok(())
}

/// Fetches a clip's image at the render time. The returned guard releases it.
pub(super) fn fetch_image<'a>(
    suites: &'a HostSuites,
    clip: OfxImageClipHandle,
    time: OfxTime,
    what: &str,
) -> Outcome<Image<'a>> {
    let mut image: OfxPropertySetHandle = core::ptr::null_mut();
    // SAFETY: `clip` came from clipGetHandle in this action; a null region means the whole
    // clip, and `image` is a valid out pointer.
    let status = unsafe {
        (suites.image_effect.clipGetImage)(clip, time, core::ptr::null(), &raw mut image)
    };
    if status != OFX_STAT_OK || image.is_null() {
        return fail(format!("{what} clip has no image (status {status})"));
    }
    // The guard exists from here on, so every later failure releases the image.
    let guard = FetchedImage {
        suites,
        handle: image,
    };
    let props = Props::new(image, suites.property);
    let data = props.get_pointer(OFX_IMAGE_PROP_DATA, 0)?;
    let row_bytes = props.get_int(OFX_IMAGE_PROP_ROW_BYTES, 0)?;
    let bounds = Rect::from_ints(read_ints(&props, OFX_IMAGE_PROP_BOUNDS)?);
    let region_of_definition =
        Rect::from_ints(read_ints(&props, OFX_IMAGE_PROP_REGION_OF_DEFINITION)?);
    Ok(Image {
        _guard: guard,
        data: data.cast::<u8>(),
        row_bytes: row_bytes as isize,
        bounds,
        region_of_definition,
        pixel_depth: props.get_string(OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH, 0)?,
        components: props.get_string(OFX_IMAGE_EFFECT_PROP_COMPONENTS, 0)?,
    })
}
/// Checks pixel depth, components and that the image covers `required`.
/// Returns the number of channels stored per pixel.
pub(super) fn check_image(
    image: &Image<'_>,
    what: &str,
    required: Rect,
    allowed: &[&str],
) -> Outcome<usize> {
    if image.pixel_depth != BIT_DEPTH_FLOAT {
        return fail(format!(
            "{what} clip must be float, found {}",
            image.pixel_depth
        ));
    }
    if !allowed.contains(&image.components.as_str()) {
        return fail(format!(
            "{what} clip has unsupported components {}",
            image.components
        ));
    }
    let channels = if image.components == COMPONENTS_ALPHA {
        1
    } else {
        4
    };
    if !image.bounds.contains(required) {
        return fail(format!(
            "{what} image bounds {} do not cover the requested region {}",
            rect_text(image.bounds),
            rect_text(required)
        ));
    }
    Ok(channels)
}

/// Copies a window of an OFX image into a top-down `(height, width, channels)` array.
///
/// # Safety
///
/// `image.data` must address the image pixels described by its bounds and row bytes, the
/// image must still be fetched, and `window` must lie inside `image.bounds` (checked by the
/// caller with [`check_image`]).
pub(super) unsafe fn read_window(image: &Image<'_>, window: Rect, channels: usize) -> Array3<f32> {
    let (height, width) = (window.height(), window.width());
    let bytes_per_pixel = channels * core::mem::size_of::<f32>();
    let row_len = width * bytes_per_pixel;
    let x_offset = (window.x1 - image.bounds.x1) as isize * bytes_per_pixel as isize;
    let mut out = Array3::<f32>::zeros((height, width, channels));
    for top_row in 0..height {
        // Top-down row 0 is the highest OFX row, so it is OFX row height - 1 above the window
        // bottom.
        let ofx_row = (height - 1 - top_row) as isize + (window.y1 - image.bounds.y1) as isize;
        // SAFETY: the offset addresses the first byte of this row inside the host buffer, which
        // the bounds check guarantees for every row of the window.
        let row_start = unsafe { image.data.offset(ofx_row * image.row_bytes + x_offset) };
        // SAFETY: `row_len` bytes from `row_start` are inside the image, per the contract above.
        // A byte slice has no alignment requirement, so unaligned row starts are fine.
        let bytes = unsafe { core::slice::from_raw_parts(row_start.cast_const(), row_len) };
        let row = out.index_axis_mut(Axis(0), top_row);
        for (value, chunk) in row.into_iter().zip(bytes.as_chunks::<4>().0.iter()) {
            *value = f32::from_ne_bytes(*chunk);
        }
    }
    out
}

/// Writes a top-down `(height, width, channels)` array into a window of an OFX image.
///
/// # Safety
///
/// Same contract as [`read_window`]; the image must be writable by the host for this action.
pub(super) unsafe fn write_window(image: &mut Image<'_>, window: Rect, values: &Array3<f32>) {
    let (height, width, channels) = values.dim();
    let bytes_per_pixel = channels * core::mem::size_of::<f32>();
    let row_len = width * bytes_per_pixel;
    let x_offset = (window.x1 - image.bounds.x1) as isize * bytes_per_pixel as isize;
    for top_row in 0..height {
        let ofx_row = (height - 1 - top_row) as isize + (window.y1 - image.bounds.y1) as isize;
        // SAFETY: as in `read_window`, the row lies inside the host buffer.
        let row_start = unsafe { image.data.offset(ofx_row * image.row_bytes + x_offset) };
        // SAFETY: `row_len` writable bytes start at `row_start`, per the contract above.
        let bytes = unsafe { core::slice::from_raw_parts_mut(row_start, row_len) };
        let row = values.index_axis(Axis(0), top_row);
        for (value, chunk) in row.iter().zip(bytes.as_chunks_mut::<4>().0.iter_mut()) {
            *chunk = value.to_ne_bytes();
        }
    }
}
/// Writes zeros into the window of the output, so a failed render never leaves host memory
/// uninitialised. Does nothing unless the output covers the window and is RGBA.
pub(super) fn clear_window(output: &mut Image<'_>, window: Rect) {
    if output.components != COMPONENTS_RGBA || !output.bounds.contains(window) {
        return;
    }
    let zeros = Array3::<f32>::zeros((window.height(), window.width(), 4));
    // SAFETY: the output image was fetched in this action, and the bounds check above shows
    // it covers `window`; the host allows writes to a render's output.
    unsafe { write_window(output, window, &zeros) };
}
