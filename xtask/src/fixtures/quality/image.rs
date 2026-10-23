//! Float RGBA and single-channel images, with the pixel operations the QA gates need: channel
//! extraction, luma, column crops and medians.
use std::ops::Range;

/// Interleaved linear RGBA float image, row-major from the top-left pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Rgba {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Interleaved `[r, g, b, a]` values, `4 * width * height` long.
    pub data: Vec<f32>,
}

/// Single-channel float plane, row-major from the top-left pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Plane {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// One value per pixel, `width * height` long.
    pub data: Vec<f32>,
}

/// Rec. 709 luma computed in f32 with the same operation order as the Python reference.
pub fn luma(red: f32, green: f32, blue: f32) -> f32 {
    red * 0.2126 + green * 0.7152 + blue * 0.0722
}

impl Rgba {
    /// Creates an image from `data`. Panics when `data` is not `4 * width * height` values long.
    pub fn new(width: usize, height: usize, data: Vec<f32>) -> Self {
        assert_eq!(data.len(), width * height * 4, "RGBA buffer size");
        Self {
            width,
            height,
            data,
        }
    }

    /// Creates a `width` by `height` image, asking `pixel(x, y)` for the RGBA value of each
    /// pixel.
    pub fn from_fn(width: usize, height: usize, pixel: impl Fn(usize, usize) -> [f32; 4]) -> Self {
        let mut data = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&pixel(x, y));
            }
        }
        Self::new(width, height, data)
    }

    /// Returns `channel` (0 to 3) of pixel `(x, y)`.
    pub fn at(&self, x: usize, y: usize, channel: usize) -> f32 {
        self.data[(y * self.width + x) * 4 + channel]
    }

    /// Returns the values of `channel` (0 to 3) as a single-channel plane of the same size.
    pub fn channel(&self, channel: usize) -> Plane {
        Plane {
            width: self.width,
            height: self.height,
            data: self.data.iter().skip(channel).step_by(4).copied().collect(),
        }
    }

    /// Returns the Rec. 709 luma of every pixel as a plane.
    pub fn luma(&self) -> Plane {
        Plane {
            width: self.width,
            height: self.height,
            data: self
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| luma(pixel[0], pixel[1], pixel[2]))
                .collect(),
        }
    }

    /// Copy a column range, as the Python `image[:, a:b]` slices do.
    pub fn crop_columns(&self, columns: Range<usize>) -> Self {
        let width = columns.len();
        let mut data = Vec::with_capacity(width * self.height * 4);
        for y in 0..self.height {
            for x in columns.clone() {
                let start = (y * self.width + x) * 4;
                data.extend_from_slice(&self.data[start..start + 4]);
            }
        }
        Self::new(width, self.height, data)
    }

    /// Returns the size as `(height, width)`, the order NumPy uses for shapes.
    pub fn shape(&self) -> (usize, usize) {
        (self.height, self.width)
    }
}

impl Plane {
    /// Returns the value of pixel `(x, y)`.
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    /// Median of the top-left 8x8 corner, used as the background estimate.
    pub fn corner_median(&self) -> f32 {
        let mut values = Vec::with_capacity(64);
        for y in 0..8.min(self.height) {
            for x in 0..8.min(self.width) {
                values.push(self.at(x, y));
            }
        }
        median_f32(&mut values)
    }
}

/// Returns the smallest and the largest value of `values`, or `(inf, -inf)` when the slice
/// is empty.
pub fn min_max(values: &[f32]) -> (f32, f32) {
    values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), value| {
            (low.min(*value), high.max(*value))
        })
}

/// Median with the numpy convention: the mean of the two middle values for even counts.
pub fn median_f32(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_unstable_by(f32::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 1 {
        values[middle]
    } else {
        f32::midpoint(values[middle - 1], values[middle])
    }
}

/// Median of `values` with the NumPy convention: the mean of the two middle values for an
/// even count. Sorts the slice in place and returns NaN when it is empty.
pub fn median_f64(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_unstable_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 1 {
        values[middle]
    } else {
        f64::midpoint(values[middle - 1], values[middle])
    }
}

/// Linear-interpolation quantile with numpy's default method and `_lerp` rounding.
pub fn quantile(values: &mut [f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_unstable_by(f64::total_cmp);
    let virtual_index = (values.len() - 1) as f64 * fraction;
    let lower = virtual_index.floor() as usize;
    let upper = (lower + 1).min(values.len() - 1);
    let weight = virtual_index - lower as f64;
    let (a, b) = (values[lower], values[upper]);
    let difference = b - a;
    if weight >= 0.5 {
        b - difference * (1.0 - weight)
    } else {
        a + difference * weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantile_matches_numpy_linear_interpolation() {
        let mut values = [4.0, 1.0, 3.0, 2.0];
        assert_eq!(quantile(&mut values, 0.5), 2.5);
        let mut values = [0.0, 10.0];
        assert!((quantile(&mut values, 0.99) - 9.9).abs() < 1e-12);
        assert!(quantile(&mut [], 0.5).is_nan());
    }

    #[test]
    fn test_median_averages_middle_pair_for_even_counts() {
        assert_eq!(median_f64(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median_f64(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median_f32(&mut [1.0, 2.0]), 1.5);
    }

    #[test]
    fn test_crop_columns_keeps_row_major_layout() {
        let image = Rgba::from_fn(3, 2, |x, y| [x as f32, y as f32, 0.0, 1.0]);
        let cropped = image.crop_columns(1..3);
        assert_eq!(cropped.shape(), (2, 2));
        assert_eq!(cropped.at(0, 1, 0), 1.0);
        assert_eq!(cropped.at(1, 0, 0), 2.0);
    }
}
