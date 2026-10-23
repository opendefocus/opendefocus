//! Separable CPU image resampling for one to N interleaved channels, and mip-chain construction for the adaptive pyramid.
//! Every channel is resampled independently and the output is clamped to each channel's source range.
mod filter;
mod pass;
mod taps;

pub use filter::ResizeFilter;

use crate::error::Error;
use crate::error::Result;
use glam::{USizeVec2, Vec2};
use ndarray::{Array3, ArrayView3};
use pass::{clamp_channels, resample_horizontal, resample_vertical};
use std::borrow::Cow;
use taps::TapTable;

/// Resizes `src` by `scale` with the given filter. The output size comes from `get_resolution`.
///
/// # Errors
/// Returns an error when the source or the computed output has a zero dimension.
pub fn resize_by_scale(
    src: &ArrayView3<f32>,
    scale: f32,
    filter: ResizeFilter,
) -> Result<Array3<f32>> {
    let target_resolution = get_resolution(src, scale);
    let resized_image = resize_to_resolution(src, target_resolution, filter)?;
    Ok(resized_image)
}

/// Returns the output size `(width, height)` for scaling `src` by `scale`, rounded and at least 1x1.
fn get_resolution(src: &ArrayView3<f32>, scale: f32) -> USizeVec2 {
    (USizeVec2::new(src.dim().1, src.dim().0).as_vec2() * scale)
        .max(Vec2::new(1.0, 1.0))
        .round()
        .as_usizevec2()
}

/// Resizes `src` to exactly `resolution` (width, height) with the given filter.
/// Each channel is resampled independently; alpha is not treated specially.
///
/// # Errors
/// Returns an error when the source or the requested resolution has a zero dimension.
pub fn resize_to_resolution(
    src: &ArrayView3<f32>,
    resolution: USizeVec2,
    filter: ResizeFilter,
) -> Result<Array3<f32>> {
    let (src_height, src_width, channels) = src.dim();
    if src_height == 0 || src_width == 0 || channels == 0 {
        return Err(Error::ResizeEmptySource);
    }
    if resolution.x == 0 || resolution.y == 0 {
        return Err(Error::ResizeEmptyTarget);
    }

    let source = contiguous_data(src);
    let width_taps = TapTable::new(src_width, resolution.x, filter);
    let (rows, source_ranges) = resample_horizontal(&source, src_width, channels, &width_taps);
    let height_taps = TapTable::new(src_height, resolution.y, filter);
    let mut data = resample_vertical(&rows, resolution.x * channels, &height_taps);
    clamp_channels(&mut data, &source_ranges);
    Ok(Array3::from_shape_vec(
        (resolution.y, resolution.x, channels),
        data,
    )?)
}

/// Returns the row-major data of `src`, borrowed when the view is standard layout and copied otherwise.
fn contiguous_data<'a>(src: &'a ArrayView3<f32>) -> Cow<'a, [f32]> {
    match src.as_slice() {
        Some(slice) => Cow::Borrowed(slice),
        None => Cow::Owned(src.iter().copied().collect()),
    }
}

/// Builds a mip chain starting at `src`, halving each side with Catmull-Rom filtering until one side reaches 1.
/// Each level keeps the per-channel mean of the level above, so repeated downscaling does not lose energy.
/// The first entry is a copy of `src`.
///
/// # Errors
/// Returns an error when `src` has a zero dimension or a resize step fails.
pub fn create_mipmaps(src: &Array3<f32>) -> Result<Vec<Array3<f32>>> {
    if src.is_empty() {
        return Err(Error::ResizeEmptySource);
    }
    let mut mipmaps = vec![src.to_owned()];
    let mut current_resolution = USizeVec2::new(src.dim().1, src.dim().0);
    while current_resolution.x > 1 && current_resolution.y > 1 {
        current_resolution /= 2;
        let previous = &mipmaps[mipmaps.len() - 1];
        let mut mipmap =
            resize_to_resolution(&previous.view(), current_resolution, ResizeFilter::Catrom)?;
        match_channel_means(&previous.view(), &mut mipmap);
        mipmaps.push(mipmap);
    }
    Ok(mipmaps)
}

/// Scales each channel of `mipmap` so its mean matches the mean of `source`. Leaves a channel unchanged when either mean is not positive.
fn match_channel_means(source: &ArrayView3<f32>, mipmap: &mut Array3<f32>) {
    let channels = source.dim().2;
    let source_means = channel_means(&contiguous_data(source), channels);
    let mipmap_means = channel_means(&contiguous_data(&mipmap.view()), channels);
    let factors: Vec<f32> = source_means
        .iter()
        .zip(&mipmap_means)
        .map(|(&s, &m)| {
            if s > 0.0 && m > 0.0 {
                (s / m) as f32
            } else {
                1.0
            }
        })
        .collect();
    for ((_, _, channel), value) in mipmap.indexed_iter_mut() {
        *value *= factors[channel];
    }
}

/// Returns the mean of each interleaved channel of `values`.
fn channel_means(values: &[f32], channels: usize) -> Vec<f64> {
    let mut sums = vec![0.0_f64; channels];
    for pixel in values.chunks_exact(channels) {
        for (sum, &value) in sums.iter_mut().zip(pixel) {
            *sum += f64::from(value);
        }
    }
    let pixels = (values.len() / channels) as f64;
    sums.iter().map(|sum| sum / pixels).collect()
}

/// Mip chain of an image together with its base resolution.
pub struct MipmapBuffer {
    /// Base level resolution as (width, height).
    resolution: USizeVec2,
    /// Mip levels from full resolution down to the smallest level, as height by width by channel arrays.
    mipmaps: Vec<Array3<f32>>,
}

impl MipmapBuffer {
    /// Creates a new `MipmapBuffer` from a vector of mipmap images.
    ///
    /// # Errors
    /// Returns `MipmapEmpty` when `mipmaps` is empty.
    pub fn from_vec(mipmaps: Vec<Array3<f32>>) -> Result<Self> {
        if mipmaps.is_empty() {
            return Err(Error::MipmapEmpty);
        }

        Ok(Self {
            resolution: USizeVec2::new(mipmaps[0].dim().1, mipmaps[0].dim().0),
            mipmaps,
        })
    }

    /// Returns borrowed views of every mip level, in order from full resolution.
    pub fn get_images_view(&'_ self) -> Vec<ArrayView3<'_, f32>> {
        self.mipmaps.iter().map(|f| f.view()).collect()
    }

    /// Returns the base level resolution as (width, height).
    pub fn get_resolution(&self) -> USizeVec2 {
        self.resolution
    }
}

#[cfg(test)]
/// Tests for resizing and mip chains.
mod tests {
    use std::path::PathBuf;

    use super::*;
    use image::{DynamicImage, Rgba32FImage};
    use image_ndarray::prelude::ImageArray;
    use ndarray::s;
    use rstest::rstest;

    #[rstest]
    #[case(USizeVec2::new(100, 80), 0.5, USizeVec2::new(50, 40))]
    #[case(USizeVec2::new(200, 160), 2.0, USizeVec2::new(400, 320))]
    #[case(USizeVec2::new(300, 400), 0.75, USizeVec2::new(225, 300))]
    /// Verifies output sizes for scale factors below, at and above 1.
    fn test_get_resolution(
        #[case] src_resolution: USizeVec2,
        #[case] scale: f32,
        #[case] expected_target: USizeVec2,
    ) {
        let src_image =
            Rgba32FImage::new(src_resolution.x as u32, src_resolution.y as u32).to_ndarray();
        assert_eq!(get_resolution(&src_image.view(), scale), expected_target);
    }

    #[rstest]
    #[case(USizeVec2::new(256, 256),
    vec![
     USizeVec2::new(256, 256),
     USizeVec2::new(128, 128),
     USizeVec2::new(64, 64),
     USizeVec2::new(32, 32),
     USizeVec2::new(16, 16),
     USizeVec2::new(8, 8),
     USizeVec2::new(4, 4),
     USizeVec2::new(2, 2),
     USizeVec2::new(1, 1)
     ])]
    /// Verifies mip chain sizes for a 256 by 256 image down to 1x1.
    fn test_expected_resolutions(
        #[case] input_resolution: USizeVec2,
        #[case] expected_resolutions: Vec<USizeVec2>,
    ) {
        let original_buffer = Array3::zeros((input_resolution.y, input_resolution.x, 4));
        let result: Vec<Array3<f32>> = create_mipmaps(&original_buffer).unwrap();
        for i in 0..expected_resolutions.len() {
            let (width, height, _) = result[i].dim();
            let result_resolution = USizeVec2::new(width, height);
            assert_eq!(result_resolution, expected_resolutions[i]);
        }
    }

    /// Loads an image from `path` and converts it to RGBA float.
    fn load_test_image(path: PathBuf) -> Rgba32FImage {
        let image = image::open(path);
        image.unwrap().to_rgba32f()
    }

    #[rstest]
    #[case(PathBuf::from("../../test/images/resize/"))]
    /// Verifies every mip level matches its reference image with a similarity score of at least 0.9.
    fn test_all_sizes(#[case] test_path: PathBuf) {
        let image = load_test_image(test_path.join("0.png")).to_ndarray();
        let result: Vec<Array3<f32>> = create_mipmaps(&image).unwrap();
        for (i, image) in result.into_iter().enumerate() {
            let expected_image = image::open(test_path.join(format!("{i}.png")))
                .unwrap()
                .to_rgb8();
            let comparison_score = image_compare::rgb_hybrid_compare(
                &DynamicImage::from(Rgba32FImage::from_ndarray(image).unwrap()).to_rgb8(),
                &expected_image,
            )
            .unwrap()
            .score;
            assert!(
                comparison_score >= 0.9,
                "image {i} got score {comparison_score}"
            );
        }
    }

    #[test]
    /// Verifies a constant RGB plane keeps each channel value after upscaling and a non-integer downscale.
    fn rgb_constant_planes_are_preserved() {
        let src = Array3::from_shape_fn((2, 2, 3), |(_, _, c)| [0.1, 0.5, 0.9][c]);
        let resized =
            resize_to_resolution(&src.view(), USizeVec2::new(5, 3), ResizeFilter::Catrom).unwrap();
        assert_eq!(resized.dim(), (3, 5, 3));
        for (channel, expected) in [0.1_f32, 0.5, 0.9].into_iter().enumerate() {
            for value in resized.slice(s![.., .., channel]) {
                assert!(
                    (value - expected).abs() < 1e-5,
                    "channel {channel}: {value}"
                );
            }
        }
    }

    #[test]
    /// Verifies one and two channel arrays are resized plane by plane without mixing channels.
    fn single_plane_channel_counts_resize_each_channel_separately() {
        let src = Array3::from_shape_fn((2, 2, 2), |(_, _, c)| [0.25_f32, 0.75][c]);
        let resized =
            resize_to_resolution(&src.view(), USizeVec2::new(3, 3), ResizeFilter::Triangle)
                .unwrap();
        assert_eq!(resized.dim(), (3, 3, 2));
        for (channel, expected) in [0.25_f32, 0.75].into_iter().enumerate() {
            for value in resized.slice(s![.., .., channel]) {
                assert!(
                    (value - expected).abs() < 1e-5,
                    "channel {channel}: {value}"
                );
            }
        }
        let gray = Array3::from_shape_fn((1, 2, 1), |(_, x, _)| x as f32);
        let upscaled =
            resize_to_resolution(&gray.view(), USizeVec2::new(4, 1), ResizeFilter::Triangle)
                .unwrap();
        let expected = [0.0_f32, 0.25, 0.75, 1.0];
        for (value, expected) in upscaled.iter().zip(expected) {
            assert!((value - expected).abs() < 1e-5, "{value} != {expected}");
        }
    }

    #[test]
    /// Verifies a zero-sized source or destination is rejected for every channel layout.
    fn zero_sized_resize_is_rejected_for_every_channel_layout() {
        for channels in [1, 2, 3, 4] {
            let src = Array3::<f32>::zeros((2, 2, channels));
            let result =
                resize_to_resolution(&src.view(), USizeVec2::new(0, 2), ResizeFilter::Triangle);
            assert!(
                matches!(result, Err(Error::ResizeEmptyTarget)),
                "channels {channels}"
            );
            let empty = Array3::<f32>::zeros((0, 2, channels));
            let result =
                resize_to_resolution(&empty.view(), USizeVec2::new(2, 2), ResizeFilter::Triangle);
            assert!(
                matches!(result, Err(Error::ResizeEmptySource)),
                "channels {channels}"
            );
        }
    }

    #[test]
    /// Verifies a zero-sized source is rejected when building a mip chain.
    fn zero_sized_mip_source_is_rejected() {
        let empty = Array3::<f32>::zeros((0, 4, 4));
        assert!(matches!(
            create_mipmaps(&empty),
            Err(Error::ResizeEmptySource)
        ));
    }

    #[test]
    /// Verifies a 2 by 2 image downscaled by two with Triangle becomes its mean.
    fn triangle_halving_gives_the_mean_of_a_two_by_two_block() {
        let src = Array3::from_shape_vec((2, 2, 1), vec![0.0_f32, 1.0, 2.0, 5.0]).unwrap();
        let resized =
            resize_to_resolution(&src.view(), USizeVec2::new(1, 1), ResizeFilter::Triangle)
                .unwrap();
        assert!((resized[[0, 0, 0]] - 2.0).abs() < 1e-6);
    }

    #[test]
    /// Verifies a Catmull-Rom ramp stays linear in the interior after a 2 to 1 downscale.
    fn ramp_is_preserved_in_the_interior() {
        let src = Array3::from_shape_fn((1, 16, 1), |(_, x, _)| x as f32 / 15.0);
        let resized =
            resize_to_resolution(&src.view(), USizeVec2::new(8, 1), ResizeFilter::Catrom).unwrap();
        for x in 2..6 {
            let centre = (x as f32 + 0.5) * 2.0 - 0.5;
            let value = resized[[0, x, 0]];
            assert!(
                (value - centre / 15.0).abs() < 1e-5,
                "x {x}: {value} vs {}",
                centre / 15.0
            );
        }
    }

    #[test]
    /// Verifies a step edge upscaled with the negative-lobed Catmull-Rom kernel does not overshoot the source range.
    fn step_edge_does_not_overshoot() {
        let src = Array3::from_shape_fn((1, 8, 1), |(_, x, _)| if x < 4 { 0.0 } else { 1.0 });
        let resized =
            resize_to_resolution(&src.view(), USizeVec2::new(32, 1), ResizeFilter::Catrom).unwrap();
        assert!(resized.iter().all(|value| (0.0..=1.0).contains(value)));
    }

    #[test]
    /// Verifies RGB stays 1 when alpha is 0, so filter images padded with zero alpha keep their colour energy.
    fn zero_alpha_does_not_zero_colour() {
        let src = Array3::from_shape_fn((4, 4, 4), |(_, _, c)| if c == 3 { 0.0 } else { 1.0 });
        let resized =
            resize_to_resolution(&src.view(), USizeVec2::new(2, 2), ResizeFilter::Catrom).unwrap();
        for channel in 0..3 {
            assert!(
                resized
                    .slice(s![.., .., channel])
                    .iter()
                    .all(|v| (v - 1.0).abs() < 1e-6)
            );
        }
        assert!(resized.slice(s![.., .., 3]).iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    /// Verifies a scale that rounds below one pixel still produces a 1x1 output.
    fn tiny_scale_yields_one_by_one() {
        let src = Array3::<f32>::from_elem((4, 4, 4), 0.25);
        let resized = resize_by_scale(&src.view(), 0.01, ResizeFilter::Mitchell).unwrap();
        assert_eq!(resized.dim(), (1, 1, 4));
        assert!((resized[[0, 0, 0]] - 0.25).abs() < 1e-6);
    }

    #[test]
    /// Verifies every mip level keeps the per-channel mean of the full-resolution image.
    fn mip_levels_preserve_the_channel_mean() {
        let src = Array3::from_shape_fn((16, 16, 2), |(y, x, c)| {
            ((x * 31 + y * 17 + c * 7) % 23) as f32 / 23.0
        });
        let base_mean = src.iter().map(|v| f64::from(*v)).sum::<f64>() / src.len() as f64;
        for mip in create_mipmaps(&src).unwrap() {
            let mean = mip.iter().map(|v| f64::from(*v)).sum::<f64>() / mip.len() as f64;
            assert!((mean - base_mean).abs() < 1e-4, "{mean} vs {base_mean}");
        }
    }

    #[test]
    /// Verifies a non-square mip chain halves each side until one side reaches 1, keeping the other.
    fn rectangular_mip_chain_stops_when_one_side_reaches_one() {
        let src = Array3::<f32>::zeros((2, 8, 4));
        let mips = create_mipmaps(&src).unwrap();
        let sizes: Vec<_> = mips.iter().map(|m| (m.dim().1, m.dim().0)).collect();
        assert_eq!(sizes, vec![(8, 2), (4, 1)]);
    }

    #[test]
    /// Verifies an empty mip list is rejected and a buffer reports its base level as width by height.
    fn mipmap_buffer_rejects_empty_and_reports_base_resolution() {
        assert!(matches!(
            MipmapBuffer::from_vec(Vec::new()),
            Err(Error::MipmapEmpty)
        ));
        let buffer = MipmapBuffer::from_vec(vec![Array3::<f32>::zeros((2, 8, 4))]).unwrap();
        assert_eq!(buffer.get_resolution(), USizeVec2::new(8, 2));
        assert_eq!(buffer.get_images_view().len(), 1);
    }
}
