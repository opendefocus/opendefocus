//! Analytic disk-PSF references: the aperture integral of a disk, the border-normalised
//! reference used for clipped and shifted support, and the edge coverage of a blurred disk.
use std::f64::consts::PI;

/// Supersampled disk aperture integral of a one-pixel impulse, normalized to unit energy.
pub fn disk_reference(shape: (usize, usize), center: (f64, f64), radius: f64) -> Vec<f64> {
    const SAMPLES: usize = 96;
    let (height, width) = shape;
    let mut area = vec![0.0; width * height];
    if radius <= 0.0 {
        area[center.1 as usize * width + center.0 as usize] = 1.0;
        return area;
    }
    let offsets: Vec<f64> = (0..SAMPLES)
        .map(|index| (index as f64 + 0.5) / SAMPLES as f64 - 0.5)
        .collect();
    let (lower_x, upper_x) = bounds(center.0, radius, width);
    let (lower_y, upper_y) = bounds(center.1, radius, height);
    for y in lower_y..upper_y {
        for x in lower_x..upper_x {
            let inside = offsets
                .iter()
                .flat_map(|oy| offsets.iter().map(move |ox| (*ox, *oy)))
                .filter(|(ox, oy)| {
                    let dx = x as f64 - center.0 + ox;
                    let dy = y as f64 - center.1 + oy;
                    dx * dx + dy * dy <= radius * radius
                })
                .count();
            area[y * width + x] = inside as f64 / (SAMPLES * SAMPLES) as f64;
        }
    }
    let normalization = PI * radius * radius;
    area.iter().map(|value| value / normalization).collect()
}

/// Returns the half-open pixel range along one axis that covers a disk of `radius` around
/// `center`, with a margin of two pixels before and three after, clipped to `0..limit`.
fn bounds(center: f64, radius: f64, limit: usize) -> (usize, usize) {
    let lower = (center - radius - 2.0).floor().max(0.0) as usize;
    let upper = ((center + radius + 3.0).ceil().max(0.0) as usize).min(limit);
    (lower.min(upper), upper)
}

/// Radial tent-gather reference with per-destination clipped aperture normalization.
///
/// `center` is the source impulse location. A nonzero shift moves the output PSF and the
/// aperture samples used to normalize each destination, including frame-edge destinations.
pub fn border_normalized_disk_reference(
    shape: (usize, usize),
    center: (f64, f64),
    radius: f64,
    kernel_shift: (f64, f64),
) -> Vec<f64> {
    let (height, width) = shape;
    let (shift_x, shift_y) = kernel_shift;
    let extent = (radius + 0.5 + shift_x.abs().max(shift_y.abs())).ceil() as i64;
    let side = (2 * extent + 1) as usize;
    let mut kernel = vec![0.0; side * side];
    for row in 0..side {
        for column in 0..side {
            let dx = column as f64 - extent as f64;
            let dy = row as f64 - extent as f64;
            let value = radius - (dx + shift_x).hypot(dy + shift_y) + 0.5;
            kernel[row * side + column] = value.clamp(0.0, 1.0);
        }
    }
    let integral = summed_area_table(&kernel, side);
    let mut denominators = vec![0.0; width * height];
    for y in 0..height as i64 {
        let min_dy = (-extent).max(-y);
        let max_dy = extent.min(height as i64 - 1 - y);
        let top = (min_dy + extent) as usize;
        let bottom = (max_dy + extent + 1) as usize;
        for x in 0..width as i64 {
            let min_dx = (-extent).max(-x);
            let max_dx = extent.min(width as i64 - 1 - x);
            let left = (min_dx + extent) as usize;
            let right = (max_dx + extent + 1) as usize;
            let at = |row: usize, column: usize| integral[row * (side + 1) + column];
            denominators[y as usize * width + x as usize] =
                at(bottom, right) - at(top, right) - at(bottom, left) + at(top, left);
        }
    }
    let mut numerator = vec![0.0; width * height];
    let source_x = center.0.floor();
    let source_y = center.1.floor();
    let (fx, fy) = (center.0 - source_x, center.1 - source_y);
    let (x0, y0) = (source_x as i64, source_y as i64);
    for (sy, wy) in [(y0, 1.0 - fy), (y0 + 1, fy)] {
        for (sx, wx) in [(x0, 1.0 - fx), (x0 + 1, fx)] {
            let weight = wx * wy;
            if weight <= 0.0 {
                continue;
            }
            for y in 0..height as i64 {
                for x in 0..width as i64 {
                    let dx = sx - x;
                    let dy = sy - y;
                    if dx.abs() <= extent && dy.abs() <= extent {
                        let index = (dy + extent) as usize * side + (dx + extent) as usize;
                        numerator[y as usize * width + x as usize] += weight * kernel[index];
                    }
                }
            }
        }
    }
    numerator
        .iter()
        .zip(&denominators)
        .map(|(value, denominator)| value / denominator.max(1e-12))
        .collect()
}

/// Zero-padded cumulative sum over rows and columns, shaped `(side + 1) x (side + 1)`.
fn summed_area_table(kernel: &[f64], side: usize) -> Vec<f64> {
    let stride = side + 1;
    let mut table = vec![0.0; stride * stride];
    for row in 0..side {
        for column in 0..side {
            table[(row + 1) * stride + column + 1] = kernel[row * side + column]
                + table[row * stride + column + 1]
                + table[(row + 1) * stride + column]
                - table[row * stride + column];
        }
    }
    table
}

/// Exact area fraction of a blur disk of radius `blur` covered by a foreground disk.
pub fn circle_overlap_fraction(distance: f64, object_radius: f64, blur_radius: f64) -> f64 {
    let (r, big_r) = (blur_radius, object_radius);
    if distance <= big_r - r {
        return 1.0;
    }
    if distance >= big_r + r || distance.is_nan() {
        return 0.0;
    }
    let d = distance;
    let term = ((-d + big_r + r) * (d + big_r - r) * (d - big_r + r) * (d + big_r + r)).max(0.0);
    let area = big_r.powi(2)
        * ((d * d + big_r * big_r - r * r) / (2.0 * d * big_r))
            .clamp(-1.0, 1.0)
            .acos()
        + r.powi(2)
            * ((d * d + r * r - big_r * big_r) / (2.0 * d * r))
                .clamp(-1.0, 1.0)
                .acos()
        - 0.5 * term.sqrt();
    area / (PI * r.powi(2))
}

/// Tests for the analytic disk references.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a disk reference away from the edges sums to one.
    #[test]
    fn test_disk_reference_has_unit_energy_away_from_edges() {
        let reference = disk_reference((33, 33), (16.0, 16.0), 4.0);
        let total: f64 = reference.iter().sum();
        assert!((total - 1.0).abs() < 0.02, "{total}");
    }

    /// Verifies that a corner-clipped reference keeps the expected energy.
    #[test]
    fn test_border_reference_matches_clipped_corner_energy() {
        let clipped = border_normalized_disk_reference((33, 33), (2.0, 2.0), 8.0, (0.0, 0.0));
        let total: f64 = clipped.iter().sum();
        assert!((total - 0.778_900_376_3).abs() < 1e-8, "{total}");
    }

    /// Verifies that the blur-circle coverage is full inside, empty outside and partial at the edge.
    #[test]
    fn test_circle_overlap_is_full_inside_and_empty_outside() {
        assert_eq!(circle_overlap_fraction(0.0, 10.0, 2.0), 1.0);
        assert_eq!(circle_overlap_fraction(20.0, 10.0, 2.0), 0.0);
        let edge = circle_overlap_fraction(10.0, 10.0, 2.0);
        assert!(edge > 0.0 && edge < 1.0);
    }
}
