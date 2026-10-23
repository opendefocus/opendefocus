//! Packs prepared near and far layers into the integral-tree samples that the exact Accurate gather walks.
use super::{AdaptiveLayerPixel, PreparedLayers};
use crate::kernel::IntegralPixel;

/// Build GPU-ready integral payloads directly, releasing each tree's build
/// state before starting the next layer.
///
/// # Errors
/// Returns `InvalidDimensions` or `SizeLimitExceeded` when the prepared layer size is invalid for the integral trees.
pub(crate) fn build_gather_integral_nodes(
    prepared: &PreparedLayers,
) -> crate::error::Result<(Vec<[u32; 16]>, Vec<[u32; 16]>)> {
    let (width, height) = integral_tree_dimensions(prepared)?;
    let near = crate::kernel::build_integral_tree_packed(width, height, |position| {
        let pixel = prepared.pixels[(position.y * width + position.x) as usize];
        gather_integral_pixel(pixel, true)
    });
    let far = crate::kernel::build_integral_tree_packed(width, height, |position| {
        let pixel = prepared.pixels[(position.y * width + position.x) as usize];
        gather_integral_pixel(pixel, false)
    });
    Ok((
        if near.is_empty() { vec![[0; 16]] } else { near },
        if far.is_empty() { vec![[0; 16]] } else { far },
    ))
}

/// Validates the prepared layer size and returns it as `(width, height)` for the integral tree builder.
/// # Errors
/// Returns `InvalidDimensions` when a side is zero or the pixel buffer length does not match, and `SizeLimitExceeded` when a side does not fit in `u32` or the pixel count is 2^31 or more.
fn integral_tree_dimensions(prepared: &PreparedLayers) -> crate::error::Result<(u32, u32)> {
    let width =
        u32::try_from(prepared.width).map_err(|_| crate::error::Error::SizeLimitExceeded)?;
    let height =
        u32::try_from(prepared.height).map_err(|_| crate::error::Error::SizeLimitExceeded)?;
    let pixel_count = u64::from(width) * u64::from(height);
    if pixel_count >= (1u64 << 31) {
        return Err(crate::error::Error::SizeLimitExceeded);
    }
    if width == 0 || height == 0 || usize::try_from(pixel_count).ok() != Some(prepared.pixels.len())
    {
        return Err(crate::error::Error::InvalidDimensions);
    }
    Ok((width, height))
}

/// Converts one prepared layer pixel into an integral-tree sample.
/// The near layer's CoC is clamped to be non-negative, and its validity is faded in by a smoothstep over the first pixel of CoC.
/// The far layer uses the absolute value of its CoC. Premultiplied colour is divided by validity to un-premultiply it.
pub(super) fn gather_integral_pixel(pixel: AdaptiveLayerPixel, near: bool) -> IntegralPixel {
    if near {
        let coc = pixel.near_coc.max(0.0);
        let transition = (coc - 1.0).clamp(0.0, 1.0);
        let front_fraction = transition * transition * (3.0 - 2.0 * transition);
        let valid = pixel.near_valid * front_fraction;
        let premul = if pixel.near_valid > 0.0 {
            glam::Vec4::from_array(pixel.near_rgba) / pixel.near_valid
        } else {
            glam::Vec4::ZERO
        };
        IntegralPixel {
            premul,
            coc,
            valid,
            occupied: valid,
        }
    } else {
        let valid = pixel.far_valid;
        let premul = if valid > 0.0 {
            glam::Vec4::from_array(pixel.far_rgba) / valid
        } else {
            glam::Vec4::ZERO
        };
        IntegralPixel {
            premul,
            coc: pixel.far_coc.abs(),
            valid,
            occupied: valid,
        }
    }
}
