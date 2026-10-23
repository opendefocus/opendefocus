//! Preview images for the adaptive layers and the diagnostic overlay of classifier tiles.
use super::PreparedLayers;
use crate::runners::wgpu::TileFeatures;
use ndarray::{Array3, s};

impl PreparedLayers {
    /// Return a linear float EXR-friendly diagnostic with near on the left and
    /// extended far color on the right; invalid samples remain transparent.
    #[must_use]
    pub fn side_by_side(&self) -> Array3<f32> {
        let mut image = Array3::zeros((self.height, self.width * 2, 4));
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixels[y * self.width + x];
                for channel in 0..4 {
                    image[[y, x, channel]] = pixel.near_rgba[channel];
                    image[[y, x + self.width, channel]] = pixel.far_rgba[channel];
                }
            }
        }
        image
    }

    /// Return a fixed checker-composited, Reinhard/sRGB PNG preview of the
    /// near layer and extended far layer. Float EXR remains the numeric source.
    #[must_use]
    pub fn preview(&self) -> Array3<f32> {
        let mut image = Array3::zeros((self.height, self.width * 2, 4));
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixels[y * self.width + x];
                let checker = if (x / 8 + y / 8) % 2 == 0 { 0.12 } else { 0.28 };
                for (column, rgba) in [pixel.near_rgba, pixel.far_rgba].into_iter().enumerate() {
                    let alpha = rgba[3].clamp(0.0, 1.0);
                    for channel in 0..3 {
                        let linear = (rgba[channel] + checker * (1.0 - alpha)).max(0.0);
                        let mapped = linear / (1.0 + linear);
                        image[[y, x + column * self.width, channel]] = srgb_encode(mapped);
                    }
                    image[[y, x + column * self.width, 3]] = 1.0;
                }
            }
        }
        image
    }
}

/// Applies the sRGB transfer curve to a linear value in `0..=1`.
fn srgb_encode(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Build a side-by-side diagnostic: the untouched source and a tile class/reason map.
#[must_use]
pub fn classifier_overlay(
    source: &Array3<f32>,
    features: &[TileFeatures],
    tiles: [usize; 2],
    origin: [i32; 2],
) -> Array3<f32> {
    let (height, width, _) = source.dim();
    let mut output = Array3::zeros((height, width * 2, 4));
    output.slice_mut(s![.., ..width, ..]).assign(source);
    let tile_origin = [origin[0].div_euclid(16), origin[1].div_euclid(16)];
    for y in 0..height {
        for x in 0..width {
            let tile_x = (origin[0] + x as i32).div_euclid(16) - tile_origin[0];
            let tile_y = (origin[1] + y as i32).div_euclid(16) - tile_origin[1];
            let index = tile_y as usize * tiles[0] + tile_x as usize;
            let feature = &features[index].values;
            let (r, g, b): (f32, f32, f32) = match feature[9] as u8 {
                1 => (0.12, 0.55, 0.9),
                2 => (1.0, 0.28, 0.08),
                _ => (0.12, 0.75, 0.18),
            };
            let reasons = feature[8] as u32;
            let tint = (reasons.count_ones() as f32 * 0.08).min(0.24);
            let column = x + width;
            output[[y, column, 0]] = (r + tint).min(1.0);
            output[[y, column, 1]] = (g + tint).min(1.0);
            output[[y, column, 2]] = (b + tint).min(1.0);
            output[[y, column, 3]] = 1.0;
        }
    }
    output
}

#[cfg(test)]
/// Tests for the side-by-side layout and the checker-composited sRGB preview.
mod tests {
    use super::*;
    use crate::runners::adaptive::AdaptiveLayerPixel;
    use bytemuck::Zeroable;

    /// Builds a single-row layer set from explicit near and far pixels.
    fn row_layers(pixels: &[([f32; 4], [f32; 4])]) -> PreparedLayers {
        PreparedLayers {
            width: pixels.len(),
            height: 1,
            near_edge_distance: vec![-1.0; pixels.len()],
            pixels: pixels
                .iter()
                .map(|&(near_rgba, far_rgba)| AdaptiveLayerPixel {
                    near_rgba,
                    far_rgba,
                    ..AdaptiveLayerPixel::zeroed()
                })
                .collect(),
        }
    }

    #[test]
    /// Verifies the side-by-side image places near pixels in the left half and far pixels at the right offset.
    fn side_by_side_places_near_then_far_columns() {
        let layers = row_layers(&[
            ([0.1, 0.2, 0.3, 0.4], [0.5, 0.6, 0.7, 0.8]),
            ([1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ]);
        let image = layers.side_by_side();
        assert_eq!(image.dim(), (1, 4, 4));
        assert_eq!(
            image.slice(ndarray::s![0, 0, ..]).to_vec(),
            vec![0.1, 0.2, 0.3, 0.4]
        );
        assert_eq!(
            image.slice(ndarray::s![0, 2, ..]).to_vec(),
            vec![0.5, 0.6, 0.7, 0.8]
        );
        assert_eq!(image.slice(ndarray::s![0, 1, ..]).to_vec(), vec![1.0; 4]);
        assert_eq!(image.slice(ndarray::s![0, 3, ..]).to_vec(), vec![0.0; 4]);
    }

    #[test]
    /// Verifies the preview is opaque, composites transparent samples over the checker, and maps linear light to sRGB.
    fn preview_composites_over_checker_and_maps_to_srgb() {
        let layers = row_layers(&[([0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0])]);
        let image = layers.preview();
        assert_eq!(image.dim(), (1, 2, 4));
        for channel in 0..3 {
            assert!((image[[0, 0, channel]] - 0.3609781).abs() < 1e-5);
            assert!((image[[0, 1, channel]] - 0.735357).abs() < 1e-5);
        }
        assert_eq!(image[[0, 0, 3]], 1.0);
        assert_eq!(image[[0, 1, 3]], 1.0);
    }

    #[test]
    /// Verifies the checker alternates every eight pixels and alpha is clamped before compositing.
    fn preview_checker_alternates_every_eight_pixels_and_clamps_alpha() {
        let mut pixels = vec![([0.0, 0.0, 0.0, 0.0], [0.0; 4]); 9];
        pixels[0].0 = [-1.0, 0.0, 0.0, 2.0];
        let layers = row_layers(&pixels);
        let image = layers.preview();
        assert!(image[[0, 0, 0]].abs() < 1e-7);
        assert!((image[[0, 1, 0]] - 0.3609781).abs() < 1e-5);
        assert!((image[[0, 8, 0]] - 0.5050552).abs() < 1e-5);
    }

    #[test]
    /// Verifies the linear segment of the sRGB curve applies below the 0.0031308 threshold.
    fn preview_uses_linear_segment_for_very_dark_values() {
        let layers = row_layers(&[([0.0001, 0.0001, 0.0001, 1.0], [0.0; 4])]);
        let image = layers.preview();
        assert!((image[[0, 0, 0]] - 0.0012919).abs() < 1e-6);
    }
}
