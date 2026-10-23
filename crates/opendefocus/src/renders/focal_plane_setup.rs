//! Focal plane overlay for depth maps. It is used as a reference by the focal plane preview tests.
use ndarray::{ArrayView2, ArrayViewMut3};

/// Returns one overlay channel for a depth sample. With `output_real_values`, channel 0 is positive depth, channel 1 marks zero depth, channel 2 is the magnitude of negative depth, and other channels are 1.
fn render_pixel(depth: f32, channel: usize, output_real_values: bool) -> f32 {
    if output_real_values {
        match channel {
            0 => {
                if depth > 0.0 {
                    depth.abs()
                } else {
                    0.0
                }
            }
            1 => {
                if depth == 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            2 => {
                if depth < 0.0 {
                    depth.abs()
                } else {
                    0.0
                }
            }
            _ => 1.0,
        }
    } else {
        match channel {
            0 => {
                if depth > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            1 => {
                if depth == 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            2 => {
                if depth < 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.5,
        }
    }
}

/// Writes the focal plane overlay for `depth` into `image`, one channel at a time as described by `render_pixel`.
pub fn render_focal_plane_overlay(
    mut image: ArrayViewMut3<f32>,
    depth: &ArrayView2<f32>,
    output_real_values: bool,
) {
    image
        .indexed_iter_mut()
        .for_each(|((y, x, channel), value)| {
            let alpha = render_pixel(depth[[y, x]], 3, output_real_values);
            *value = (render_pixel(depth[[y, x]], channel, output_real_values) * alpha)
                + (*value * (1.0 - alpha));
        });
}

#[cfg(test)]
/// Tests for the focal plane overlay.
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;
    use rstest::rstest;

    #[rstest]
    #[case(true, 10.0, 10.0, 0.0, 0.0, 1.0)] // Test positive depth with output_real_values
    #[case(true, -10.0, 0.0, 0.0, 10.0, 1.0)] // Test negative depth with output_real_values
    #[case(true, 0.0, 0.0, 1.0, 0.0, 1.0)] // Test zero depth with output_real_values
    #[case(false, 10.0, 1.0, 0.0, 0.0, 0.5)] // Test positive depth with output_real_values=false
    #[case(false, -10.0, 0.0, 0.0, 1.0, 0.5)] // Test negative depth with output_real_values=false
    #[case(false, 0.0, 0.0, 1.0, 0.0, 0.5)] // Test zero depth with output_real_values=false

    /// Test `render_pixel` method with different depth values and `output_real_values` settings
    fn test_render_pixel(
        #[case] output_real_values: bool,
        #[case] depth: f32,
        #[case] expected_r: f32,
        #[case] expected_g: f32,
        #[case] expected_b: f32,
        #[case] expected_a: f32,
    ) {
        assert_approx_eq!(render_pixel(depth, 0, output_real_values), expected_r, 1e-6);
        assert_approx_eq!(render_pixel(depth, 1, output_real_values), expected_g, 1e-6);
        assert_approx_eq!(render_pixel(depth, 2, output_real_values), expected_b, 1e-6);
        assert_approx_eq!(render_pixel(depth, 3, output_real_values), expected_a, 1e-6);
    }
}
