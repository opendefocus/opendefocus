//! Analytic pixel-centre scenes shared by the committed scenes and the adaptive fixtures.

use super::exr::Rgba;

/// Width of every analytic scene in pixels.
pub const WIDTH: usize = 192;
/// Height of every analytic scene in pixels.
pub const HEIGHT: usize = 128;

/// Analytic scene: premultiplied linear RGBA and depth for every pixel, in row-major order.
pub struct Scene {
    /// Premultiplied linear RGBA of each pixel.
    pub color: Vec<Rgba>,
    /// Depth of each pixel in metres, positive away from the camera.
    pub depth: Vec<f64>,
}

/// Python's `max(a, b)`: keeps `a` unless `b` is strictly greater, so signed zeros match.
pub fn py_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// Python's `min(a, b)`: keeps `a` unless `b` is strictly smaller, so signed zeros match.
pub fn py_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// Builds the named scene at `WIDTH` by `HEIGHT`: `occlusion`, `focus_ramp` or `hdr_points`.
/// Any other name builds `alpha_edges`.
pub fn scene(name: &str) -> Scene {
    let mut color = Vec::with_capacity(WIDTH * HEIGHT);
    let mut depth = Vec::with_capacity(WIDTH * HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (z, rgba) = match name {
                "occlusion" => occlusion(x, y),
                "focus_ramp" => focus_ramp(x, y),
                "hdr_points" => hdr_points(x, y),
                _ => alpha_edges(x, y),
            };
            color.push(rgba);
            depth.push(z);
        }
    }
    Scene { color, depth }
}

/// Returns the depth and RGBA of pixel `(x, y)` in the occlusion scene: a disk and a bar in
/// front of a checkered background, plus a small bright dot.
fn occlusion(x: usize, y: usize) -> (f64, Rgba) {
    let (u, v) = centre(x, y);
    let (xi, yi) = (x as i64, y as i64);
    let mut rgb = [0.08 + 0.5 * u, 0.12 + 0.4 * v, 0.3];
    if ((xi / 4) + (yi / 4)) % 2 == 0 {
        rgb = rgb.map(|channel| channel * 0.3);
    }
    let disk = (xi - 96).pow(2) + (yi - 62).pow(2) < 25_i64.pow(2);
    let bar = (38..61).contains(&xi) || ((102..115).contains(&xi) && (22..110).contains(&yi));
    let mut depth = 8.0;
    if disk {
        depth = 4.0;
        rgb = [0.2, 1.2, 0.15];
    }
    if bar {
        depth = 2.0;
        rgb = [0.8, 0.03, 0.01];
    }
    let dot = (xi - 66).pow(2) + (yi - 38).pow(2) < 3_i64.pow(2);
    if !disk && !bar && dot {
        rgb = [32.0, 18.0, 6.0];
    }
    (depth, [rgb[0], rgb[1], rgb[2], 1.0])
}

/// Returns the depth and RGBA of pixel `(x, y)` in the focus ramp: depth follows the
/// circle of confusion across the width, with striped bands and a vertical strip at a fixed
/// depth.
fn focus_ramp(x: usize, y: usize) -> (f64, Rgba) {
    let (u, v) = centre(x, y);
    let coc = 24.0 * (2.0 * u - 1.0);
    let mut depth = 4.0 / (1.0 - coc / 24.0);
    let mut rgb = [u, v, 0.2 + 0.5 * u];
    if (24..56).contains(&y) {
        let value = if x % 2 == 1 { 0.9 } else { 0.02 };
        rgb = [value; 3];
    }
    if (76..108).contains(&y) {
        let value = if (x / 3 + y / 3) % 2 == 1 { 0.9 } else { 0.02 };
        rgb = [value; 3];
    }
    if (94..98).contains(&x) {
        depth = 4.0;
        rgb = [0.8, 0.4, 0.05];
    }
    (depth, [rgb[0], rgb[1], rgb[2], 1.0])
}

/// Bright point sources as `(x, y, depth, intensity, tint)`. The `hdr_points` scene draws
/// each one as a 3 by 3 square.
const HDR_POINTS: [(i64, i64, f64, f64, [f64; 3]); 6] = [
    (30, 30, 2.0, 16.0, [1.0, 0.2, 0.05]),
    (96, 30, 4.0, 32.0, [0.2, 1.0, 0.1]),
    (158, 30, 8.0, 64.0, [0.1, 0.3, 1.0]),
    (30, 94, 8.0, 128.0, [1.0, 0.6, 0.2]),
    (96, 94, 8.0, 32.0, [1.0, 1.0, 1.0]),
    (188, 124, 8.0, 64.0, [0.2, 1.0, 1.0]),
];

/// Returns the depth and RGBA of pixel `(x, y)` in the HDR points scene: a dark background
/// with the squares from `HDR_POINTS`.
fn hdr_points(x: usize, y: usize) -> (f64, Rgba) {
    let (xi, yi) = (x as i64, y as i64);
    let mut depth = 8.0;
    let mut rgb = [0.002; 3];
    for (cx, cy, distance, intensity, tint) in HDR_POINTS {
        if (xi - cx).abs() <= 1 && (yi - cy).abs() <= 1 {
            depth = distance;
            rgb = tint.map(|channel| intensity * channel);
        }
    }
    (depth, [rgb[0], rgb[1], rgb[2], 1.0])
}

/// Returns the depth and premultiplied RGBA of pixel `(x, y)` in the alpha edges scene. The
/// left half has a soft-edged disk and the right half a rectangle, with a border and a
/// horizontal line of partial alpha.
fn alpha_edges(x: usize, y: usize) -> (f64, Rgba) {
    let (xi, yi) = (x as i64, y as i64);
    let left = x < 96;
    let depth = if left { 2.0 } else { 8.0 };
    let distance = if left {
        py_min(
            25.0 - ((xi - 40) as f64).hypot((yi - 64) as f64),
            (80 - xi) as f64,
        )
    } else {
        let edges = [
            (xi - 102) as f64,
            (170 - xi) as f64,
            (yi - 24) as f64,
            (103 - yi) as f64,
        ];
        edges.into_iter().fold(f64::INFINITY, py_min)
    };
    let scale = if left { 1.0 } else { 0.35 };
    let mut alpha = py_min(1.0, py_max(0.0, distance + 0.5)) * scale;
    if xi < 3 || yi < 3 || ((62..66).contains(&yi) && xi < 80) {
        alpha = 0.65;
    }
    let rgb = [alpha * 2.0, alpha * 0.3, alpha * 0.05];
    (depth, [rgb[0], rgb[1], rgb[2], alpha])
}

/// Returns the normalised centre `((x + 0.5) / WIDTH, (y + 0.5) / HEIGHT)` of pixel `(x, y)`.
fn centre(x: usize, y: usize) -> (f64, f64) {
    (
        (x as f64 + 0.5) / WIDTH as f64,
        (y as f64 + 0.5) / HEIGHT as f64,
    )
}

/// Fixed Reinhard tone map, then sRGB OETF. Never used as renderer input.
pub fn display(value: f64) -> u8 {
    let mapped = py_max(0.0, value);
    let mapped = mapped / (1.0 + mapped);
    let encoded = if mapped <= 0.003_130_8 {
        12.92 * mapped
    } else {
        1.055 * mapped.powf(1.0 / 2.4) - 0.055
    };
    (255.0 * encoded).round_ties_even() as u8
}

/// Tests for the analytic scene helpers.
#[cfg(test)]
mod tests {
    use super::{display, py_max, py_min};

    /// Verifies that `py_max` and `py_min` keep the first argument on ties, as Python does.
    #[test]
    fn min_and_max_follow_python_argument_order() {
        assert!(py_max(0.0, -0.0).is_sign_positive());
        assert!(py_min(0.0, -0.0).is_sign_positive());
        assert_eq!(py_min(1.0, 0.5), 0.5);
        assert_eq!(py_max(0.0, -3.0), 0.0);
    }

    /// Verifies that the display tone map rounds half to even and clamps negative input to
    /// black.
    #[test]
    fn display_rounds_half_to_even_and_clamps_negative_input() {
        assert_eq!(display(0.0), 0);
        assert_eq!(display(-5.0), 0);
        assert_eq!(display(1.0e6), 255);
    }
}
