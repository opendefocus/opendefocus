//! Integration tests for normalized float conversions and image-array round trips.

use image_ndarray::prelude::NormalizedFloat;

/// Checks normalized conversions for integer and float values that work without the `image` feature.
#[test]
fn test_normalization_without_image_feature() {
    assert_eq!(255_u8.to_f32_normalized(), Some(1.0));
    assert_eq!(u16::from_f64_normalized(1.0), Some(u16::MAX));
    assert_eq!((-2.5_f32).to_f32_normalized(), Some(-2.5));
}

#[cfg(feature = "image")]
mod image_tests {
    use image::{GrayImage, Luma, RgbaImage};
    use image_ndarray::{Error, prelude::ImageArray};
    use ndarray::{Array2, Array3, s};

    /// Checks that views and compact round trips reuse the same allocation and keep edits.
    #[test]
    fn test_views_and_compact_roundtrip_reuse_allocation() {
        let mut data = Vec::with_capacity(100);
        data.extend(0..24_u8);
        let mut image = RgbaImage::from_raw(3, 2, data).unwrap();
        let pointer = image.as_raw().as_ptr();
        assert_eq!(image.as_ndarray().as_ptr(), pointer);
        image.as_ndarray_mut()[[1, 2, 3]] = 99;
        assert_eq!(image.get_pixel(2, 1)[3], 99);
        let array = image.to_ndarray();
        assert_eq!(array.as_ptr(), pointer);
        let image = RgbaImage::from_ndarray(array).unwrap();
        assert_eq!(image.as_raw().as_ptr(), pointer);
        assert_eq!(image.get_pixel(2, 1)[3], 99);
    }

    /// Checks that sliced, reversed, permuted and strided arrays convert to correctly ordered images.
    #[test]
    fn test_owned_slice_offset_and_nonstandard_layouts() {
        let source = Array3::from_shape_fn((4, 3, 4), |(y, x, c)| (y * 12 + x * 4 + c) as u8);
        for array in [
            source.clone().slice_move(s![1..3, .., ..]),
            source.clone().slice_move(s![..;-1, .., ..]),
            source.clone().permuted_axes([1, 0, 2]),
            source.slice_move(s![.., ..;2, ..]),
        ] {
            let expected: Vec<_> = array.iter().copied().collect();
            let dimensions = (array.shape()[1] as u32, array.shape()[0] as u32);
            let image = RgbaImage::from_ndarray(array).unwrap();
            assert_eq!(image.dimensions(), dimensions);
            assert_eq!(image.into_raw(), expected);
        }
    }

    /// Checks that extra backing storage is ignored and zero-sized images round trip.
    #[test]
    fn test_extra_image_storage_and_empty_images() {
        let image = RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4, 99, 99]).unwrap();
        assert_eq!(
            image.as_ndarray().iter().copied().collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        let array = image.to_ndarray();
        assert_eq!(array.as_slice().unwrap(), &[1, 2, 3, 4]);
        for (width, height) in [(0, 0), (0, 3), (3, 0)] {
            let image =
                RgbaImage::from_ndarray(RgbaImage::new(width, height).to_ndarray()).unwrap();
            assert_eq!(image.dimensions(), (width, height));
        }
    }

    /// Checks grayscale conversion and the errors for wrong channels, ranks and oversized dimensions.
    #[test]
    fn test_dimensions_channels_and_grayscale() {
        let image = GrayImage::from_ndarray(
            Array2::from_shape_vec((2, 3), vec![1, 2, 3, 4, 5, 6]).unwrap(),
        )
        .unwrap();
        assert_eq!(image.get_pixel(2, 1), &Luma([6]));
        assert_eq!(
            RgbaImage::from_ndarray(Array2::<u8>::zeros((2, 3))).unwrap_err(),
            Error::ChannelMismatch
        );
        assert_eq!(
            RgbaImage::from_ndarray(ndarray::Array1::<u8>::zeros(4)).unwrap_err(),
            Error::Dimensions
        );
        assert_eq!(
            RgbaImage::from_ndarray(ndarray::Array4::<u8>::zeros((1, 1, 1, 4))).unwrap_err(),
            Error::Dimensions
        );
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            RgbaImage::from_ndarray(Array3::<u8>::zeros((0, u32::MAX as usize + 1, 4)))
                .unwrap_err(),
            Error::Dimensions
        );
    }
}
