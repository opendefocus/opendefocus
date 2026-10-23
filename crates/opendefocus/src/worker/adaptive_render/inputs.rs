//! Prepares the inputs of an adaptive render: the bounded CoC from the depth map and the aperture with its mip chain.
use super::plan::FrameGeometry;
use crate::{
    adaptive_coc_from_depth, adaptive_filter_resolution,
    datamodel::{Settings, defocus::DefocusMode, render::FilterMode},
    error::{Error, Result},
    renders::{
        filter::prepare_filter_image,
        resize::{MipmapBuffer, create_mipmaps},
    },
};
use glam::UVec2;
use ndarray::{Array2, Array3, ArrayView3};

/// Smallest padding in pixels around the aperture image.
const MIN_APERTURE_PADDING: u32 = 8;
/// Channel count of the aperture image.
const APERTURE_CHANNELS: usize = 4;

/// Converts an image with 1 to 4 channels to RGBA. Channels that are missing stay zero.
/// # Errors
/// Returns `InvalidChannelCount` when the channel count is 0 or above 4.
pub(super) fn rgba_from_image(image: &ArrayView3<'_, f32>) -> Result<Array3<f32>> {
    let (height, width, channels) = image.dim();
    if channels == 0 || channels > 4 {
        return Err(Error::InvalidChannelCount);
    }
    let mut rgba = Array3::zeros((height, width, 4));
    for y in 0..height {
        for x in 0..width {
            for channel in 0..channels.min(3) {
                rgba[[y, x, channel]] = image[[y, x, channel]];
            }
            rgba[[y, x, 3]] = if channels == 4 { image[[y, x, 3]] } else { 1.0 };
        }
    }
    Ok(rgba)
}

/// Returns the CoC plane for the frame. The depth map is ignored in 2D mode and consumed otherwise;
/// its full-frame allocation is released here, before any streamed source chunks are prepared.
/// A constant CoC from 2D mode is expanded to the full frame.
///
/// # Errors
/// Returns `DepthNotFound` when depth is required but missing, and `InvalidDimensions` when it does not match the frame.
pub(super) fn coc_from_depth(
    depth: Option<Array2<f32>>,
    settings: &Settings,
    frame: &FrameGeometry,
) -> Result<Array2<f32>> {
    let shape = (frame.height, frame.width);
    let depth = match depth {
        _ if settings.defocus.defocus_mode() == DefocusMode::Twod => Array2::zeros(shape),
        Some(depth) if depth.dim() == shape => depth,
        Some(_) => return Err(Error::InvalidDimensions),
        None => return Err(Error::DepthNotFound),
    };
    let coc = adaptive_coc_from_depth(&depth, settings);
    drop(depth);
    let coc = if coc.dim() == (1, 1) && shape != (1, 1) {
        Array2::from_elem(shape, coc[[0, 0]])
    } else {
        coc
    };
    if coc.dim() == shape {
        Ok(coc)
    } else {
        Err(Error::InvalidDimensions)
    }
}

/// Returns the largest CoC radius in pixels the settings allow.
///
/// # Errors
/// Returns `InvalidParameter` when the radius is negative or not finite.
pub(super) fn validated_max_radius(settings: &Settings) -> Result<f32> {
    let radius = settings.defocus.get_current_max_size();
    if radius.is_finite() && radius >= 0.0 {
        Ok(radius)
    } else {
        Err(Error::InvalidParameter)
    }
}

/// Clamps the CoC to the configured reach. Adaptive layer preparation bounds propagation by that radius,
/// so larger values gather identically and would only make the tile classifier request unnecessary work.
pub(super) fn bound_coc(coc: Array2<f32>, max_radius: f32) -> Array2<f32> {
    coc.mapv_into(|value| value.clamp(-max_radius, max_radius))
}

/// Builds the aperture texture and its mips, or `None` for the simple disc, which needs no texture.
///
/// # Errors
/// Returns an error when the filter image is invalid or the mip chain cannot be built.
pub(super) fn build_aperture(
    filter: Option<Array3<f32>>,
    settings: &Settings,
) -> Result<Option<MipmapBuffer>> {
    if FilterMode::try_from(settings.render.filter.mode).ok() == Some(FilterMode::Simple) {
        return Ok(None);
    }
    let padding = settings.defocus.get_padding().max(MIN_APERTURE_PADDING);
    let filter_image = prepare_filter_image(filter, settings, padding, APERTURE_CHANNELS)?;
    let mips = if settings.defocus.defocus_mode() == DefocusMode::Twod {
        vec![filter_image]
    } else {
        create_mipmaps(&filter_image)?
    };
    Ok(Some(MipmapBuffer::from_vec(mips)?))
}

/// Returns the aperture resolution in pixels: that of the texture, or the settings' default for the simple disc.
pub(super) fn aperture_resolution(aperture: Option<&MipmapBuffer>, settings: &Settings) -> UVec2 {
    aperture.map_or_else(
        || adaptive_filter_resolution(settings),
        |aperture| {
            let resolution = aperture.get_resolution();
            UVec2::new(resolution.x as u32, resolution.y as u32)
        },
    )
}

/// Converts a frame pixel count to the `u32` the GPU settings carry.
///
/// # Errors
/// Returns `SizeLimitExceeded` when the pixel count does not fit the `u32` the GPU settings carry.
pub(super) fn gpu_pixel_count(width: usize, height: usize) -> Result<u32> {
    u32::try_from(width * height).map_err(|_| Error::SizeLimitExceeded)
}

#[cfg(test)]
/// Tests for converting caller images and depth maps into renderer inputs.
mod tests {
    use super::*;
    use crate::datamodel::{IVector4, render::RenderSpecs};

    /// Frame geometry for a `width` by `height` image.
    fn frame(width: i32, height: i32) -> FrameGeometry {
        let region = IVector4 {
            x: 0,
            y: 0,
            z: width,
            w: height,
        };
        FrameGeometry::new(
            &RenderSpecs {
                full_region: region,
                render_region: region,
            },
            (height as usize, width as usize, 4),
        )
        .unwrap()
    }

    /// Depth-mode settings that pass depth through as CoC.
    fn direct_depth_settings() -> Settings {
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.use_direct_math = true;
        settings
    }

    #[test]
    /// Verifies depth mode requires a depth map of the frame size.
    fn depth_mode_requires_matching_depth() {
        let settings = direct_depth_settings();
        let frame = frame(4, 3);
        assert!(matches!(
            coc_from_depth(None, &settings, &frame),
            Err(Error::DepthNotFound)
        ));
        assert!(matches!(
            coc_from_depth(Some(Array2::zeros((3, 5))), &settings, &frame),
            Err(Error::InvalidDimensions)
        ));
        let coc = coc_from_depth(Some(Array2::from_elem((3, 4), 2.5)), &settings, &frame).unwrap();
        assert_eq!(coc, Array2::from_elem((3, 4), 2.5));
    }

    #[test]
    /// Verifies 2D mode ignores any depth and expands the constant CoC to the frame.
    fn two_d_mode_expands_the_constant_coc() {
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Twod);
        let frame = frame(4, 3);
        let without = coc_from_depth(None, &settings, &frame).unwrap();
        let with = coc_from_depth(Some(Array2::zeros((9, 9))), &settings, &frame).unwrap();
        assert_eq!(without.dim(), (3, 4));
        assert_eq!(without, with);
        assert_eq!(without, Array2::from_elem((3, 4), without[[0, 0]]));
    }

    #[test]
    /// Verifies the CoC is clamped to the radius in both directions.
    fn bound_coc_clamps_both_signs() {
        let bounded = bound_coc(
            Array2::from_shape_vec((1, 4), vec![-9.0, -1.0, 2.0, 30.0]).unwrap(),
            4.0,
        );
        assert_eq!(
            bounded.iter().copied().collect::<Vec<_>>(),
            vec![-4.0, -1.0, 2.0, 4.0]
        );
    }

    #[test]
    /// Verifies the pixel count conversion reports overflow instead of truncating.
    fn gpu_pixel_count_rejects_overflow() {
        assert_eq!(gpu_pixel_count(4, 3).unwrap(), 12);
        assert!(matches!(
            gpu_pixel_count(1 << 20, 1 << 20),
            Err(Error::SizeLimitExceeded)
        ));
    }

    #[test]
    /// Verifies a single-channel image fills only red and sets opaque alpha, leaving green and blue at zero.
    fn one_channel_image_fills_red_and_opaque_alpha() {
        let image = Array3::from_elem((1, 2, 1), 0.25);
        let rgba = rgba_from_image(&image.view()).unwrap();
        assert_eq!(rgba.dim(), (1, 2, 4));
        assert_eq!(
            rgba.slice(ndarray::s![0, 0, ..]).to_vec(),
            vec![0.25, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    /// Verifies a three-channel image copies RGB and sets alpha to one.
    fn three_channel_image_copies_rgb_and_sets_opaque_alpha() {
        let image = Array3::from_shape_fn((1, 1, 3), |(_, _, c)| c as f32 + 0.5);
        let rgba = rgba_from_image(&image.view()).unwrap();
        assert_eq!(
            rgba.slice(ndarray::s![0, 0, ..]).to_vec(),
            vec![0.5, 1.5, 2.5, 1.0]
        );
    }

    #[test]
    /// Verifies a four-channel image keeps its own alpha rather than forcing it opaque.
    fn four_channel_image_keeps_its_alpha() {
        let image = Array3::from_shape_fn((1, 1, 4), |(_, _, c)| (c as f32) * 0.1);
        let rgba = rgba_from_image(&image.view()).unwrap();
        assert_eq!(rgba[[0, 0, 3]], image[[0, 0, 3]]);
        assert!((rgba[[0, 0, 3]] - 0.3).abs() < 1e-6);
    }

    #[test]
    /// Verifies images with zero or more than four channels are rejected.
    fn rgba_conversion_rejects_zero_and_five_channels() {
        let empty = Array3::<f32>::zeros((1, 1, 0));
        assert!(matches!(
            rgba_from_image(&empty.view()),
            Err(Error::InvalidChannelCount)
        ));
        let five = Array3::<f32>::zeros((1, 1, 5));
        assert!(matches!(
            rgba_from_image(&five.view()),
            Err(Error::InvalidChannelCount)
        ));
    }
}
