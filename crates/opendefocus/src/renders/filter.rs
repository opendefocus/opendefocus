//! Turns the configured filter (a custom image or a generated bokeh) into the RGBA array the renderer consumes.
//! Channels are padded so the array always has the processing channel count.
use crate::datamodel::{Settings, render::FilterMode};
use crate::error::{Error, Result};
use crate::renders::resize::ResizeFilter;
use glam::UVec2;
use ndarray::{Array3, ArrayView3, s};

/// Side length in pixels of the placeholder filter used by Simple mode.
const MINIMUM_FILTER_SIZE: u32 = 8;
/// Channel count the processing pipeline expects: red, green, blue and alpha.
const PROCESSING_CHANNEL_COUNT: usize = 4;

/// Prepare a filter image for Adaptive processing.
///
/// # Errors
/// Returns `InvalidChannelCount` when the filter has more channels than the pipeline supports, or a resize error when resampling fails.
pub(crate) fn prepare_filter_image(
    filter: Option<Array3<f32>>,
    settings: &Settings,
    max_size: u32,
    channels: usize,
) -> Result<Array3<f32>> {
    let image = if let Some(image) = filter {
        let (height, width, _) = image.dim();
        let scale_factor = height.max(width) as f32;
        let filter_image = crate::renders::resize::resize_by_scale(
            &image.view(),
            max_size as f32 / scale_factor,
            ResizeFilter::Mitchell,
        )?;

        resize_array_dimensions(&filter_image.view(), PROCESSING_CHANNEL_COUNT)?
    } else if settings.render.filter.mode() == FilterMode::Simple {
        Array3::zeros((
            MINIMUM_FILTER_SIZE as usize,
            MINIMUM_FILTER_SIZE as usize,
            channels,
        ))
    } else {
        let resolution = settings
            .render
            .filter
            .calculate_filter_box(settings.bokeh.aspect_ratio);
        let resolution =
            UVec2::new(resolution[2] - resolution[0], resolution[3] - resolution[1]).as_usizevec2();
        let mut image = Array3::zeros((resolution.y, resolution.x, channels));
        bokeh_creator::Renderer::render_to_array_f32(settings.bokeh, &mut image.view_mut());
        resize_array_dimensions(&image.view(), PROCESSING_CHANNEL_COUNT)?
    };

    Ok(image)
}

/// Pads an array with zero channels up to `dimensions` channels. Returns a copy when the counts already match.
/// # Errors
/// Returns `InvalidChannelCount` when the array has more channels than `dimensions`.
fn resize_array_dimensions(array: &ArrayView3<f32>, dimensions: usize) -> Result<Array3<f32>> {
    let difference = dimensions as i32 - array.dim().2 as i32;
    if difference < 0 {
        return Err(Error::InvalidChannelCount);
    } else if difference == 0 {
        return Ok(array.to_owned());
    }
    let mut target = Array3::zeros((array.dim().0, array.dim().1, dimensions));
    target.slice_mut(s![.., .., ..array.dim().2]).assign(array);

    Ok(target)
}

#[cfg(test)]
/// Tests for filter preparation.
mod tests {
    use super::*;

    #[test]
    /// Verifies a generated bokeh filter matches a direct render of its filter box and is finite.
    fn generated_filter_uses_local_settings_and_ndarray_types() {
        let mut settings = Settings::default();
        settings.render.filter.mode = FilterMode::BokehCreator.into();
        settings.render.filter.resolution = 32;
        settings.bokeh.aspect_ratio = 0.5;
        settings.bokeh.angle = 195.3;
        let bounds = settings
            .render
            .filter
            .calculate_filter_box(settings.bokeh.aspect_ratio);
        let width = (bounds[2] - bounds[0]) as usize;
        let height = (bounds[3] - bounds[1]) as usize;
        let mut expected = Array3::<f32>::zeros((height, width, 4));
        bokeh_creator::Renderer::render_to_array_f32(settings.bokeh, &mut expected.view_mut());
        let actual = prepare_filter_image(None, &settings, 32, 4).unwrap();
        assert_eq!(actual, expected);
        assert!(actual.iter().all(|value| value.is_finite()));
    }

    #[test]
    /// Verifies a custom filter keeps HDR values and asymmetry, and ignores unrelated bokeh settings.
    fn custom_filter_preserves_asymmetry_and_hdr() {
        let settings = Settings::default();
        let mut custom = Array3::<f32>::zeros((8, 8, 4));
        for y in 0..8 {
            for x in 0..8 {
                for channel in 0..4 {
                    custom[(y, x, channel)] = (y * 8 + x) as f32 + channel as f32 * 0.25;
                }
            }
        }
        let actual = prepare_filter_image(Some(custom.clone()), &settings, 8, 4).unwrap();
        assert_eq!(actual.dim(), (8, 8, 4));
        let mut other_settings = settings;
        other_settings.render.filter.mode = FilterMode::BokehCreator.into();
        other_settings.bokeh.radius = 0.25;
        other_settings.bokeh.angle = 90.0;
        let other = prepare_filter_image(Some(custom), &other_settings, 8, 4).unwrap();
        assert_eq!(actual, other);
        assert!(actual[(7, 7, 0)] > 1.0);
        assert!(actual[(7, 7, 3)] > 1.0);
        assert_ne!(actual[(0, 7, 0)], actual[(7, 0, 0)]);
    }

    #[test]
    /// Verifies a custom filter with three channels is zero-padded to four, keeping its colour values.
    fn three_channel_custom_filter_is_padded_with_zero_alpha() {
        let settings = Settings::default();
        let custom = Array3::from_shape_fn((4, 4, 3), |(_, _, c)| [0.2_f32, 0.4, 0.6][c]);
        let actual = prepare_filter_image(Some(custom), &settings, 4, 4).unwrap();
        assert_eq!(actual.dim(), (4, 4, 4));
        for y in 0..4 {
            for x in 0..4 {
                assert!((actual[(y, x, 0)] - 0.2).abs() < 1e-5);
                assert!((actual[(y, x, 1)] - 0.4).abs() < 1e-5);
                assert!((actual[(y, x, 2)] - 0.6).abs() < 1e-5);
                assert_eq!(actual[(y, x, 3)], 0.0);
            }
        }
    }

    #[test]
    /// Verifies a custom filter with more channels than the pipeline supports is rejected.
    fn custom_filter_with_five_channels_is_rejected() {
        let settings = Settings::default();
        let custom = Array3::<f32>::zeros((4, 4, 5));
        assert!(matches!(
            prepare_filter_image(Some(custom), &settings, 4, 4),
            Err(Error::InvalidChannelCount)
        ));
    }

    #[test]
    /// Verifies a zero-height custom filter fails in the resampler instead of producing an empty filter.
    fn zero_height_custom_filter_is_a_resize_error() {
        let settings = Settings::default();
        let custom = Array3::<f32>::zeros((0, 4, 4));
        assert!(matches!(
            prepare_filter_image(Some(custom), &settings, 8, 4),
            Err(Error::ResizeEmptySource)
        ));
    }

    #[test]
    /// Verifies Simple mode without a custom image yields an 8 by 8 black filter with the requested channel count.
    fn simple_mode_yields_black_placeholder_filter() {
        let mut settings = Settings::default();
        settings.render.filter.mode = FilterMode::Simple.into();
        let actual = prepare_filter_image(None, &settings, 64, 4).unwrap();
        assert_eq!(actual.dim(), (8, 8, 4));
        assert!(actual.iter().all(|value| *value == 0.0));
    }
}
