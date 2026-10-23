//! Conversions between `image::ImageBuffer` and `ndarray` arrays, and the
//! normalized float trait used to move between integer and float channel values.

#[cfg(feature = "image")]
use crate::error::{Error, Result};
#[cfg(feature = "image")]
use image::{ImageBuffer, Pixel};
#[cfg(feature = "image")]
use ndarray::{Array, Array3, ArrayView3, ArrayViewMut, ArrayViewMut3, Dimension};
use num_traits::{AsPrimitive, ToPrimitive};

#[cfg(feature = "image")]
/// Conversion methods for working with ndarrays.
///
/// Views and standard-layout ownership conversions reuse the original allocation.
/// Non-standard layouts are copied into image row order.
pub trait ImageArray<P: image::Pixel, ImageContainer> {
    /// Cast the `ImageBuffer` as an `ArrayView3`.
    ///
    /// * `Y` index is the row
    /// * `X` index is the columns
    /// * `Z` index is the channel
    ///
    /// So when referencing:
    /// `array[[y, x, z]]`
    ///
    /// This does not copy the data, as it is a reference to the actual data in the buffer.
    fn as_ndarray(&self) -> ArrayView3<'_, ImageContainer>;

    /// Cast the `ImageBuffer` as an `ArrayViewMut3`.
    ///
    /// * `Y` index is the row
    /// * `X` index is the columns
    /// * `Z` index is the channel
    ///
    /// So when referencing:
    /// `array[[y, x, z]]`
    ///
    /// This does not copy the data, as it is a reference to the actual data in the buffer.
    fn as_ndarray_mut(&mut self) -> ArrayViewMut3<'_, ImageContainer>;

    /// Interpret the `ImageBuffer` as an Array3.
    ///
    /// * `Y` index is the row
    /// * `X` index is the columns
    /// * `Z` index is the channel
    ///
    /// So when referencing:
    /// `array[[y, x, z]]`
    ///
    /// This does not copy the data, but it does consume the buffer.
    fn to_ndarray(self) -> Array3<ImageContainer>;

    /// Convert the provided array into the `ImageBuffer`
    ///
    /// * `Y` index is the row
    /// * `X` index is the columns
    /// * `Z` index is the channel
    ///
    /// So when referencing:
    /// `array[[y, x, z]]`
    ///
    /// This consumes the array. Standard layouts reuse its allocation; other
    /// layouts are copied into row-major pixel order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Dimensions`] if the array is not 2D or 3D, or its size does not fit
    /// into the image dimensions, [`Error::ChannelMismatch`] if the channel count differs from
    /// the pixel type, and [`Error::ImageConstructFailed`] if the buffer cannot back an image.
    fn from_ndarray<D: Dimension>(
        array: Array<ImageContainer, D>,
    ) -> Result<ImageBuffer<P, Vec<ImageContainer>>>;
}

#[cfg(feature = "image")]
impl<P, C> ImageArray<P, C> for ImageBuffer<P, Vec<C>>
where
    P: Pixel<Subpixel = C>,
    C: Clone + Copy,
{
    fn as_ndarray(&self) -> ArrayView3<'_, C> {
        let (width, height) = self.dimensions();
        // SAFETY: `as_raw` holds at least `width * height * CHANNEL_COUNT` elements (an
        // `ImageBuffer` invariant), which is exactly the shape passed here. The returned view
        // borrows `self`, so the buffer outlives it and is not mutated while it is read.
        unsafe {
            ArrayView3::from_shape_ptr(
                (height as usize, width as usize, P::CHANNEL_COUNT as usize),
                self.as_raw().as_ptr(),
            )
        }
    }

    #[allow(
        clippy::expect_used,
        reason = "the trait method is infallible and the shape is derived from the buffer's own dimensions after truncation"
    )]
    fn to_ndarray(self) -> Array3<C> {
        let (width, height) = self.dimensions();
        let shape = (height as usize, width as usize, P::CHANNEL_COUNT as usize);
        let mut data = self.into_raw();
        // ImageBuffer permits extra backing elements outside its pixel dimensions.
        data.truncate(shape.0 * shape.1 * shape.2);
        Array3::from_shape_vec(shape, data).expect("ImageBuffer dimensions match its pixel buffer")
    }
    fn from_ndarray<D: Dimension>(array: Array<C, D>) -> Result<ImageBuffer<P, Vec<C>>> {
        let shape = array.shape();
        if shape.len() < 2 {
            return Err(Error::Dimensions);
        }

        let (width, height) = (shape[1], shape[0]);
        let channels = match shape.len() {
            2 => 1,
            3 => shape[2],
            _ => return Err(Error::Dimensions),
        };

        if channels != P::CHANNEL_COUNT.into() {
            return Err(Error::ChannelMismatch);
        }

        let width = u32::try_from(width).map_err(|_| Error::Dimensions)?;
        let height = u32::try_from(height).map_err(|_| Error::Dimensions)?;
        let size = array.len();
        let array = if array.is_standard_layout() {
            array
        } else {
            array.as_standard_layout().into_owned()
        };
        let (mut data, offset) = array.into_raw_vec_and_offset();
        // Owned slices can retain an offset and a larger backing allocation.
        if let Some(offset) = offset.filter(|offset| *offset != 0) {
            data.drain(..offset);
        }
        data.truncate(size);
        Self::from_raw(width, height, data).ok_or(Error::ImageConstructFailed)
    }

    fn as_ndarray_mut(&mut self) -> ArrayViewMut3<'_, C> {
        let (width, height) = self.dimensions();

        // SAFETY: the buffer holds at least `width * height * CHANNEL_COUNT` elements (an
        // `ImageBuffer` invariant), matching the shape passed here. Taking `&mut self` gives
        // exclusive access for the lifetime of the returned view.
        unsafe {
            ArrayViewMut::from_shape_ptr(
                (height as usize, width as usize, P::CHANNEL_COUNT as usize),
                self.as_mut_ptr(),
            )
        }
    }
}

/// Trait for converting the provided value to a normalized float.
///
/// This is used for image processing where a lot of operations rely on floating values.
pub trait NormalizedFloat<T>
where
    T: AsPrimitive<f32> + AsPrimitive<f64>,
{
    /// Convert the value to a 32 bit float.
    ///
    /// The value will be in a normalized range according to color depths.
    ///
    /// For example in u8, a value of 255 would be represented as 1.0.
    ///
    /// Returns None if it overflows and could not be represented.
    fn to_f32_normalized(&self) -> Option<f32>;
    /// Convert the value to a 64 bit float
    ///
    /// The value will be in a normalized range according to color depths.
    ///
    /// For example in u8, a value of 255 would be represented as 1.0.
    ///
    /// Returns None if it overflows and could not be represented.
    fn to_f64_normalized(&self) -> Option<f64>;

    /// Converts the f32 value to the provided type
    ///
    /// Returns None if it overflows and could not be represented.
    fn from_f32_normalized(value: f32) -> Option<T>;

    /// Converts the f64 value to the provided type
    ///
    /// Returns None if it overflows and could not be represented.
    fn from_f64_normalized(value: f64) -> Option<T>;
}

impl NormalizedFloat<f32> for f32 {
    fn to_f32_normalized(&self) -> Option<f32> {
        Some(*self)
    }

    fn to_f64_normalized(&self) -> Option<f64> {
        self.to_f64()
    }
    fn from_f32_normalized(value: f32) -> Option<f32> {
        Some(value)
    }

    fn from_f64_normalized(value: f64) -> Option<f32> {
        value.to_f32()
    }
}

impl NormalizedFloat<f64> for f64 {
    fn to_f32_normalized(&self) -> Option<f32> {
        self.to_f32()
    }

    fn to_f64_normalized(&self) -> Option<f64> {
        Some(*self)
    }
    fn from_f32_normalized(value: f32) -> Option<f64> {
        value.to_f64()
    }

    fn from_f64_normalized(value: f64) -> Option<f64> {
        Some(value)
    }
}

/// Implements [`NormalizedFloat`] for an integer type, scaling by its `MAX` value.
///
/// Converted values are multiplied or divided by `<$type>::MAX`, so `MAX` maps to 1.0.
#[macro_export]
macro_rules! impl_as_float {
    ($type:ty) => {
        impl NormalizedFloat<$type> for $type {
            fn to_f32_normalized(&self) -> Option<f32> {
                self.to_f32()
                    .map(|converted| converted / <$type>::MAX as f32)
            }

            fn to_f64_normalized(&self) -> Option<f64> {
                self.to_f64()
                    .map(|converted| converted / <$type>::MAX as f64)
            }

            fn from_f32_normalized(value: f32) -> Option<$type> {
                Some((value * <$type>::MAX as f32).as_())
            }

            fn from_f64_normalized(value: f64) -> Option<$type> {
                Some((value * <$type>::MAX as f64).as_())
            }
        }
    };
}

impl_as_float!(i32);
impl_as_float!(u32);
impl_as_float!(i16);
impl_as_float!(u16);
impl_as_float!(i8);
impl_as_float!(u8);

#[cfg(feature = "image")]
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb, Rgb32FImage, Rgba32FImage};
    use ndarray::{Array1, Array2, Array4};
    use rstest::*;

    /// Checks that an RGBA float image viewed as an array indexes the same values as its pixels.
    #[test]
    fn test_as_ndarray_rgba() {
        let (width, height, channels) = (256, 128, 4);
        let data = create_test_data(width, height, channels);
        let test_image = Rgba32FImage::from_vec(256, 128, data).unwrap();

        let array = test_image.as_ndarray();

        for ((y, x, channel), value) in array.indexed_iter() {
            assert_eq!(test_image.get_pixel(x as u32, y as u32)[channel], *value);
        }
    }

    /// Checks that a single-channel float image viewed as an array matches its pixels.
    #[test]
    fn test_as_ndarray_luma() {
        let (width, height, channels) = (256, 128, 1);
        let data = create_test_data(width, height, channels);
        let test_image: ImageBuffer<Luma<f32>, Vec<f32>> =
            ImageBuffer::from_vec(256, 128, data).unwrap();

        let array = test_image.as_ndarray();

        for ((y, x, channel), value) in array.indexed_iter() {
            assert_eq!(test_image.get_pixel(x as u32, y as u32)[channel], *value);
        }
    }

    /// Checks that writes through the mutable array view update the image pixels.
    #[test]
    fn test_as_ndarray_mut() {
        let (width, height, channels) = (256, 128, 4);
        let data = create_test_data(width, height, channels);
        let mut test_image = Rgba32FImage::from_vec(256, 128, data).unwrap();
        let compare = test_image.clone();

        let mut array = test_image.as_ndarray_mut();
        array += 1.0;

        for (x, y, pixel) in test_image.enumerate_pixels() {
            let compare_pixel = compare.get_pixel(x, y);
            for (channel, value) in pixel.channels().iter().enumerate() {
                assert_eq!(*value, compare_pixel[channel] + 1.0);
            }
        }
    }

    /// Checks that consuming an image into an array keeps every pixel value in place.
    #[test]
    fn test_to_ndarray() {
        let (width, height, channels) = (256, 128, 4);
        let data = create_test_data(width, height, channels);
        let test_image = Rgba32FImage::from_vec(256, 128, data).unwrap();

        let mut array = test_image.clone().to_ndarray();

        array += 1.0;
        for ((y, x, channel), value) in array.indexed_iter() {
            assert_eq!(
                test_image.get_pixel(x as u32, y as u32)[channel] + 1.0,
                *value
            );
        }
    }

    /// Checks that a 3D array converts into an RGBA image with matching pixel values.
    #[test]
    fn test_from_ndarray() {
        let (width, height, channels) = (256, 128, 4);
        let data = create_test_data(width, height, channels);
        let test_image = Array3::from_shape_vec((height, width, channels), data).unwrap();
        let compare_data = test_image.clone();

        let result = Rgba32FImage::from_ndarray(test_image).unwrap();

        for (x, y, pixel) in result.enumerate_pixels() {
            for (channel, value) in pixel.channels().iter().enumerate() {
                assert_eq!(*value, compare_data[[y as usize, x as usize, channel]]);
            }
        }
    }

    /// Checks that a 2D array converts into a single-channel image.
    #[test]
    fn test_from_ndarray_2d() {
        let (width, height, channels) = (256, 128, 1);
        let data = create_test_data(width, height, channels);
        let test_image = Array2::from_shape_vec((height, width), data).unwrap();
        let compare_data = test_image.clone();

        let result = ImageBuffer::<Luma<f32>, Vec<f32>>::from_ndarray(test_image).unwrap();

        for (x, y, pixel) in result.enumerate_pixels() {
            for value in pixel.channels() {
                assert_eq!(*value, compare_data[[y as usize, x as usize]]);
            }
        }
    }

    /// Returns `width * height * channels` ascending values starting at 1.
    fn create_test_data(width: usize, height: usize, channels: usize) -> Vec<f32> {
        let total_elements = width * height * channels;
        (0..total_elements).map(|x| (x + 1) as f32).collect()
    }

    /// Checks that converting an array whose channel count differs from the pixel type fails with `ChannelMismatch`.
    #[test]
    fn test_from_ndarray_with_invalid_channels() {
        let channels = 4;
        let (width, height) = (256.0, 128.0);
        let total_elements = (width * height * 4.0) as usize;
        let data: Vec<f32> = (0..total_elements).map(|x| (x + 1) as f32).collect();
        let test_image =
            Array3::from_shape_vec((height as usize, width as usize, channels), data).unwrap();

        let result = Rgb32FImage::from_ndarray(test_image.into_dyn())
            .err()
            .unwrap();

        assert_eq!(result, Error::ChannelMismatch);
    }

    /// Checks that an RGB `u8` image viewed as an array maps `[y, x, channel]` to the stored bytes.
    #[test]
    fn test_as_ndarray_u8_hand_values() {
        let image = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(3, 2, (0..18).collect()).unwrap();

        let array = image.as_ndarray();

        assert_eq!(array.shape(), &[2, 3, 3]);
        assert_eq!(array[[0, 0, 0]], 0);
        assert_eq!(array[[0, 1, 0]], 3);
        assert_eq!(array[[1, 2, 2]], 17);
    }

    /// Checks that a write through `as_ndarray_mut` lands at the expected byte of a grayscale image.
    #[test]
    fn test_as_ndarray_mut_luma_hand_values() {
        let mut image =
            ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(2, 2, vec![10, 20, 30, 40]).unwrap();

        image.as_ndarray_mut()[[1, 0, 0]] = 99;

        assert_eq!(image.as_raw().as_slice(), &[10, 20, 99, 40]);
    }

    /// Checks that a `(height, width, 1)` array becomes a grayscale image with row-major pixels.
    #[test]
    fn test_from_ndarray_3d_single_channel_hand_values() {
        let array = Array3::from_shape_vec((2, 3, 1), vec![1_u8, 2, 3, 4, 5, 6]).unwrap();

        let image = ImageBuffer::<Luma<u8>, Vec<u8>>::from_ndarray(array).unwrap();

        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.get_pixel(2, 0)[0], 3);
        assert_eq!(image.get_pixel(0, 1)[0], 4);
    }

    /// Checks that 1D and 4D arrays are rejected with `Dimensions`, since images are 2D or 3D.
    #[test]
    fn test_from_ndarray_rejects_1d_and_4d_arrays() {
        let one_d = Array1::from_vec(vec![1_u8, 2, 3, 4]);
        assert_eq!(
            ImageBuffer::<Luma<u8>, Vec<u8>>::from_ndarray(one_d).err(),
            Some(Error::Dimensions)
        );

        let four_d = Array4::from_shape_vec((1, 2, 2, 1), vec![1_u8, 2, 3, 4]).unwrap();
        assert_eq!(
            ImageBuffer::<Luma<u8>, Vec<u8>>::from_ndarray(four_d).err(),
            Some(Error::Dimensions)
        );
    }

    /// Checks that a 2D array cannot back a three-channel image, because it has one implicit channel.
    #[test]
    fn test_from_ndarray_2d_into_rgb_channel_mismatch() {
        let array = Array2::from_shape_vec((2, 3), vec![1_u8; 6]).unwrap();

        assert_eq!(
            ImageBuffer::<Rgb<u8>, Vec<u8>>::from_ndarray(array).err(),
            Some(Error::ChannelMismatch)
        );
    }

    /// Checks the exact scale factors: `MAX` maps to 1.0, and the truncating cast back to an integer.
    #[test]
    fn test_normalized_ints_hand_values() {
        assert_eq!(200_u8.to_f32_normalized(), Some(200.0 / 255.0));
        assert_eq!((-128_i8).to_f32_normalized(), Some(-128.0 / 127.0));
        assert_eq!(u16::MAX.to_f64_normalized(), Some(1.0));
        assert_eq!(u8::from_f32_normalized(0.5), Some(127));
        assert_eq!(i8::from_f32_normalized(-1.0), Some(-127));
    }

    /// Checks that out-of-range normalized values saturate at the integer limits instead of wrapping.
    #[test]
    fn test_from_normalized_saturates_at_limits() {
        assert_eq!(u8::from_f32_normalized(2.0), Some(u8::MAX));
        assert_eq!(u8::from_f64_normalized(-0.5), Some(0));
        assert_eq!(i8::from_f64_normalized(-2.0), Some(i8::MIN));
    }

    #[rstest]
    #[case(1.0)]
    #[case(255.0)]
    #[case(0.5)]
    #[case(-1.0)]
    #[case(-255.0)]
    /// Checks that `f32` and `f64` normalization are identity conversions and that they round-trip.
    fn test_f32(#[case] float: f32) {
        assert_eq!(float.to_f32_normalized().unwrap(), float);
        assert_eq!(f32::from_f32_normalized(float).unwrap(), float);

        let float_64: f64 = float.as_();
        assert_eq!(float_64.to_f64_normalized().unwrap(), float_64);
        assert_eq!(f64::from_f64_normalized(float_64).unwrap(), float_64);

        let converted_to_float64 = float.to_f64_normalized().unwrap();
        assert_eq!(converted_to_float64, float as f64);

        let converted_back_to_float32 = float_64.to_f32_normalized().unwrap();
        assert_eq!(converted_back_to_float32, float);
    }

    /// Generates a test that normalizes and denormalizes unsigned integers of the given type.
    #[macro_export]
    macro_rules! test_unsigned_ints {
        ($name:ident, $type:ty) => {
            #[rstest]
            #[case(0)]
            #[case(1)]
            #[case($type::MAX)]
            #[case($type::MIN)]
            fn $name(#[case] int: $type) {
                let normalized_f32 = int.to_f32_normalized().unwrap();
                let expected_normalized_f32 = int as f32 / <$type>::MAX as f32;
                assert_eq!(normalized_f32, expected_normalized_f32);

                let int_from_float32 =
                    <$type>::from_f32_normalized(expected_normalized_f32).unwrap();
                let expected_int_from_float32 =
                    (expected_normalized_f32 * <$type>::MAX as f32) as $type;
                assert_eq!(int_from_float32, expected_int_from_float32);

                let normalized_f64 = int.to_f64_normalized().unwrap();
                let expected_normalized_f64 = int as f64 / <$type>::MAX as f64;
                assert_eq!(normalized_f64, expected_normalized_f64);

                let int_from_float64 =
                    <$type>::from_f64_normalized(expected_normalized_f64).unwrap();
                let expected_int_from_float64 =
                    (expected_normalized_f64 * <$type>::MAX as f64) as $type;
                assert_eq!(int_from_float64, expected_int_from_float64);
            }
        };
    }

    /// Generates a test that normalizes and denormalizes signed integers of the given type.
    #[macro_export]
    macro_rules! test_signed_ints {
        ($name:ident, $type:ty) => {
            #[rstest]
            #[case(0)]
            #[case(1)]
            #[case($type::MAX)]
            #[case($type::MIN)]
            #[case(-1)]
            #[case(-$type::MAX)]
            fn $name(#[case] int: $type) {
                let normalized_f32 = int.to_f32_normalized().unwrap();
                let expected_normalized_f32 = int as f32 / <$type>::MAX as f32;
                assert_eq!(normalized_f32, expected_normalized_f32);

                let int_from_float32 =
                    <$type>::from_f32_normalized(expected_normalized_f32).unwrap();
                let expected_int_from_float32 =
                    (expected_normalized_f32 * <$type>::MAX as f32) as $type;
                assert_eq!(int_from_float32, expected_int_from_float32);

                let normalized_f64 = int.to_f64_normalized().unwrap();
                let expected_normalized_f64 = int as f64 / <$type>::MAX as f64;
                assert_eq!(normalized_f64, expected_normalized_f64);

                let int_from_float64 =
                    <$type>::from_f64_normalized(expected_normalized_f64).unwrap();
                let expected_int_from_float64 =
                    (expected_normalized_f64 * <$type>::MAX as f64) as $type;
                assert_eq!(int_from_float64, expected_int_from_float64);
            }
        };
    }
    test_signed_ints!(test_i32, i32);
    test_signed_ints!(test_i16, i16);
    test_signed_ints!(test_i8, i8);
    test_unsigned_ints!(test_u32, u32);
    test_unsigned_ints!(test_u16, u16);
    test_unsigned_ints!(test_u8, u8);
}
