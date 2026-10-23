//! Measures a moving point after fractional horizontal transport of the previous frame, which
//! checks that the renderer moves the point's energy with the point.
use serde_json::{Value, json};

use crate::fixtures::quality::image::{Plane, quantile};

/// Difference between a point's support in the current frame and the previous frame shifted by
/// the point's motion.
#[derive(Clone, Debug)]
pub struct PointDelta {
    /// Number of pixels in the support.
    pub support_pixels: usize,
    /// Mean absolute difference over the support.
    pub mean_abs_delta: f64,
    /// 99th percentile of the absolute differences over the support.
    pub p99_abs_delta: f64,
    /// Largest absolute difference over the support.
    pub max_abs_delta: f64,
    /// Total difference divided by the current energy in the support.
    pub normalized_difference_energy: f64,
}

impl PointDelta {
    /// Serialises the delta to a JSON object.
    pub fn to_value(&self) -> Value {
        json!({
            "support_pixels": self.support_pixels,
            "mean_abs_delta": self.mean_abs_delta,
            "p99_abs_delta": self.p99_abs_delta,
            "max_abs_delta": self.max_abs_delta,
            "normalized_difference_energy": self.normalized_difference_energy,
        })
    }
}

/// Compare a moving point after fractional horizontal transport of the previous frame.
pub fn transported_point_delta(
    previous: &Plane,
    current: &Plane,
    dx: f64,
    center: (f64, f64),
    radius: f64,
) -> PointDelta {
    let (width, height) = (current.width, current.height);
    let sources: Vec<(usize, usize, f64)> = (0..width)
        .map(|x| {
            let source = x as f64 - dx;
            let lower = source.floor();
            let fraction = source - lower;
            let lower = (lower as i64).clamp(0, width as i64 - 1) as usize;
            let upper = (lower + 1).min(width - 1);
            (lower, upper, fraction)
        })
        .collect();
    let mut differences = Vec::new();
    let mut current_energy = 0.0;
    for y in 0..height {
        for (x, (lower, upper, fraction)) in sources.iter().enumerate() {
            let distance = (x as f64 - center.0).hypot(y as f64 - center.1);
            if distance > radius + 2.0 {
                continue;
            }
            let now = f64::from(current.at(x, y));
            let before = f64::from(previous.at(*lower, y)) * (1.0 - fraction)
                + f64::from(previous.at(*upper, y)) * fraction;
            differences.push((now - before).abs());
            current_energy += now.abs();
        }
    }
    let support_pixels = differences.len();
    let count = support_pixels as f64;
    let total: f64 = differences.iter().sum();
    let max = differences
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let mut sorted = differences;
    PointDelta {
        support_pixels,
        mean_abs_delta: total / count,
        p99_abs_delta: quantile(&mut sorted, 0.99),
        max_abs_delta: max,
        normalized_difference_energy: total / current_energy.max(1e-12),
    }
}

/// Tests for the transported point delta.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that an unshifted identical plane leaves no residue.
    #[test]
    fn test_zero_shift_of_identical_plane_has_no_residue() {
        let plane = Plane {
            width: 5,
            height: 3,
            data: (0..15).map(|value| value as f32 * 0.1).collect(),
        };
        let delta = transported_point_delta(&plane, &plane, 0.0, (2.0, 1.0), 2.0);
        assert!(delta.support_pixels > 0);
        assert_eq!(delta.max_abs_delta, 0.0);
        assert_eq!(delta.normalized_difference_energy, 0.0);
    }

    /// Verifies that a whole-pixel shift of a point leaves no residue inside its support.
    #[test]
    fn test_whole_pixel_shift_moves_residue_to_the_edge() {
        let previous = Plane {
            width: 4,
            height: 1,
            data: vec![0.0, 1.0, 0.0, 0.0],
        };
        let current = Plane {
            width: 4,
            height: 1,
            data: vec![0.0, 0.0, 1.0, 0.0],
        };
        let delta = transported_point_delta(&previous, &current, 1.0, (2.0, 0.0), 1.0);
        assert!(delta.max_abs_delta < 1e-6, "{delta:?}");
    }
}
