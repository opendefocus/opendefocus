//! Procedural adaptive-render inputs: point sources, silhouettes and fractional HDR splats.

use crate::fixtures::scenes::{
    analytic::{HEIGHT, WIDTH, py_max, py_min},
    exr::Rgba,
};

/// Image size as `(width, height)` in pixels.
pub type Dims = (usize, usize);
/// Pixel colours and the matching per-pixel values (depth or CoC), both row-major.
pub type Scene = (Vec<Rgba>, Vec<f64>);

/// Point source with a centre in pixel coordinates, a signed circle-of-confusion radius and
/// a linear RGBA colour.
pub struct Point {
    /// Centre in pixel coordinates; may be fractional.
    pub center_xy: (f64, f64),
    /// Circle-of-confusion radius written to the pixels the point covers; positive for near and
    /// negative for far.
    pub signed_radius: f64,
    /// Linear colour of the point before it is spread over pixels.
    pub rgba: Rgba,
}

/// Returns a 192 by 128 scene of constant premultiplied colour at half alpha, with depth
/// 4 m everywhere.
pub fn constant_scene() -> Scene {
    (
        vec![[0.2, 0.1, 0.05, 0.5]; WIDTH * HEIGHT],
        vec![4.0; WIDTH * HEIGHT],
    )
}

/// Returns a scene of `dims` with one white pixel at the centre and a constant CoC of
/// `sign * radius`. The background is black, and transparent when `transparent` is set.
pub fn point_scene(radius: f64, sign: f64, transparent: bool, dims: Dims) -> Scene {
    let (width, height) = dims;
    let background = [0.0, 0.0, 0.0, if transparent { 0.0 } else { 1.0 }];
    let mut color = vec![background; width * height];
    color[(height / 2) * width + width / 2] = [1.0; 4];
    (color, vec![sign * radius; width * height])
}

/// Spreads each point over its four neighbouring pixels with bilinear weights, so a
/// fractional centre divides its colour and weight between them. Pixels that are not
/// covered keep the background and a CoC of 0. Covered pixels get the point's signed radius;
/// their alpha is 1 over an opaque background and is scaled by the weight over a transparent
/// one.
pub fn fractional_points_scene(points: &[Point], dims: Dims, background: Rgba) -> Scene {
    let (width, height) = dims;
    let mut color = vec![background; width * height];
    let mut coc = vec![0.0; width * height];
    for point in points {
        let (center_x, center_y) = point.center_xy;
        let x0 = center_x.floor();
        let y0 = center_y.floor();
        let (fx, fy) = (center_x - x0, center_y - y0);
        let (x0, y0) = (x0 as usize, y0 as usize);
        for (y, wy) in [(y0, 1.0 - fy), (y0 + 1, fy)] {
            for (x, wx) in [(x0, 1.0 - fx), (x0 + 1, fx)] {
                let weight = wx * wy;
                if weight <= 0.0 {
                    continue;
                }
                let pixel = y * width + x;
                let [red, green, blue, alpha] = point.rgba;
                let alpha = if background[3] > 0.0 {
                    1.0
                } else {
                    alpha * weight
                };
                color[pixel] = [red * weight, green * weight, blue * weight, alpha];
                coc[pixel] = point.signed_radius;
            }
        }
    }
    (color, coc)
}

/// Sets the CoC of a square patch around `center_xy` to `signed_radius`. The centre is
/// rounded to whole pixels, the patch extends `half_extent` pixels in each direction, and it
/// is clipped to the frame.
pub fn paint_coc_patch(
    coc: &mut [f64],
    dims: Dims,
    center_xy: (f64, f64),
    half_extent: i64,
    signed_radius: f64,
) {
    let (width, height) = dims;
    let cx = center_xy.0.round_ties_even() as i64;
    let cy = center_xy.1.round_ties_even() as i64;
    let left = (cx - half_extent).max(0) as usize;
    let right = ((cx + half_extent + 1).min(width as i64)) as usize;
    let top = (cy - half_extent).max(0) as usize;
    let bottom = ((cy + half_extent + 1).min(height as i64)) as usize;
    for y in top..bottom {
        for value in &mut coc[y * width + left..y * width + right] {
            *value = signed_radius;
        }
    }
}

/// Returns a 192 by 128 scene with a soft-edged disk of radius 28 pixels, centred `x_offset`
/// pixels from the middle of the frame. The CoC is `radius` inside the disk and 0 outside.
pub fn silhouette_scene(radius: f64, x_offset: f64) -> Scene {
    let cx = WIDTH as f64 / 2.0 + x_offset;
    let cy = HEIGHT as f64 / 2.0;
    let mut color = Vec::with_capacity(WIDTH * HEIGHT);
    let mut coc = Vec::with_capacity(WIDTH * HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let distance = 28.0 - ((x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy));
            let alpha = py_min(1.0, py_max(0.0, distance + 0.5));
            color.push([alpha, 0.0, 0.25 * (1.0 - alpha), 1.0]);
            coc.push(if distance >= 0.0 { radius } else { 0.0 });
        }
    }
    (color, coc)
}

#[cfg(test)]
mod tests {
    use super::{Point, fractional_points_scene, paint_coc_patch};

    #[test]
    fn integer_centre_leaves_zero_weight_neighbours_untouched() {
        let point = Point {
            center_xy: (2.0, 1.0),
            signed_radius: -4.0,
            rgba: [8.0, 4.0, 2.0, 1.0],
        };
        let (color, coc) = fractional_points_scene(&[point], (4, 3), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(color[4 + 2], [8.0, 4.0, 2.0, 1.0]);
        assert_eq!(coc[4 + 2], -4.0);
        assert_eq!(coc[4 + 3], 0.0);
        assert_eq!(color[2 * 4 + 2], [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn fractional_centre_splits_weight_and_transparent_alpha() {
        let point = Point {
            center_xy: (0.5, 0.5),
            signed_radius: 2.0,
            rgba: [4.0, 0.0, 0.0, 0.5],
        };
        let (color, _) = fractional_points_scene(&[point], (2, 2), [0.0; 4]);
        assert_eq!(color[0], [1.0, 0.0, 0.0, 0.125]);
        assert_eq!(color[3], [1.0, 0.0, 0.0, 0.125]);
    }

    #[test]
    fn coc_patch_is_clipped_to_the_frame() {
        let mut coc = vec![0.0; 9];
        paint_coc_patch(&mut coc, (3, 3), (0.0, 0.0), 1, 5.0);
        assert_eq!(coc, vec![5.0, 5.0, 0.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0]);
    }
}
