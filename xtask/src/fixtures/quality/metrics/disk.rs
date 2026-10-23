//! Error statistics of a measured single-channel signal against an analytic disk reference:
//! centroid, energy, shape and leakage beyond the disk.
use serde_json::{Map, Value, json};

use super::reference::disk_reference;
use crate::fixtures::quality::image::Plane;

/// Error statistics of a measured signal against an analytic disk-PSF reference.
#[derive(Clone, Debug)]
pub struct DiskProfile {
    /// Measured energy centroid `[x, y]` in pixels.
    pub centroid_px: [f64; 2],
    /// Centroid of the reference `[x, y]` in pixels.
    pub expected_centroid_px: [f64; 2],
    /// Distance between the measured and the expected centroid, in pixels.
    pub centroid_error_px: f64,
    /// Sum of the measured signal.
    pub signal_energy: f64,
    /// Difference between the measured and the expected energy, relative to the expected energy.
    pub energy_relative_error: f64,
    /// Sum of the reference.
    pub expected_energy: f64,
    /// Root-mean-square error against the reference over the reference support.
    pub disk_reference_rmse: f64,
    /// Absolute error over the reference support, divided by the reference energy.
    pub normalized_disk_l1: f64,
    /// Root-mean-square error divided by the RMS value of the reference.
    pub normalized_disk_rmse: f64,
    /// Largest absolute error over the reference support.
    pub disk_reference_max_error: f64,
    /// Fraction of the signal outside the disk radius plus one pixel.
    pub leakage_beyond_radius_plus_1: f64,
    /// Smallest and largest measured value, `[min, max]`.
    pub signal_range: [f64; 2],
}

impl DiskProfile {
    /// Returns every field as a JSON map keyed by its metric name.
    pub fn to_map(&self) -> Map<String, Value> {
        let mut map = Map::new();
        let entries = [
            ("centroid_px", json!(self.centroid_px)),
            ("expected_centroid_px", json!(self.expected_centroid_px)),
            ("centroid_error_px", json!(self.centroid_error_px)),
            ("signal_energy", json!(self.signal_energy)),
            ("energy_relative_error", json!(self.energy_relative_error)),
            ("expected_energy", json!(self.expected_energy)),
            ("disk_reference_rmse", json!(self.disk_reference_rmse)),
            ("normalized_disk_l1", json!(self.normalized_disk_l1)),
            ("normalized_disk_rmse", json!(self.normalized_disk_rmse)),
            (
                "disk_reference_max_error",
                json!(self.disk_reference_max_error),
            ),
            (
                "leakage_beyond_radius_plus_1",
                json!(self.leakage_beyond_radius_plus_1),
            ),
            ("signal_range", json!(self.signal_range)),
        ];
        for (key, value) in entries {
            map.insert(key.to_owned(), value);
        }
        map
    }

    /// Value of one of the five gated profile norms, keyed by its Python metric name.
    pub fn gated_value(&self, name: &str) -> f64 {
        match name {
            "centroid_error_px" => self.centroid_error_px,
            "energy_relative_error" => self.energy_relative_error,
            "normalized_disk_l1" => self.normalized_disk_l1,
            "normalized_disk_rmse" => self.normalized_disk_rmse,
            _ => self.leakage_beyond_radius_plus_1,
        }
    }
}

/// Compare a single-channel signal with a disk reference; the centre defaults to the frame centre.
pub fn disk_signal_metrics(
    signal: &Plane,
    radius: f64,
    center: Option<(f64, f64)>,
    reference: Option<&[f64]>,
) -> DiskProfile {
    let (width, height) = (signal.width, signal.height);
    let (cx, cy) = center.unwrap_or(((width / 2) as f64, (height / 2) as f64));
    let target: Vec<f64> = match reference {
        Some(values) => values.to_vec(),
        None => disk_reference((height, width), (cx, cy), radius),
    };
    let values: Vec<f64> = signal.data.iter().map(|value| f64::from(*value)).collect();
    let target_mass: f64 = target.iter().sum();
    let mass: f64 = values.iter().sum();
    let support: Vec<usize> = (0..target.len())
        .filter(|index| target[*index] > 0.0)
        .collect();
    let errors: Vec<f64> = support
        .iter()
        .map(|index| values[*index] - target[*index])
        .collect();
    let target_squares: Vec<f64> = support.iter().map(|index| target[*index].powi(2)).collect();
    let error_mean_square = mean(errors.iter().map(|error| error * error));
    let target_rms = mean(target_squares.iter().copied()).sqrt();

    let mut outside_mass = 0.0;
    let (mut signal_x, mut signal_y, mut target_x, mut target_y) = (0.0, 0.0, 0.0, 0.0);
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let (fx, fy) = (x as f64, y as f64);
            if (fx - cx).hypot(fy - cy) > radius + 1.0 {
                outside_mass += values[index];
            }
            signal_x += values[index] * fx;
            signal_y += values[index] * fy;
            target_x += target[index] * fx;
            target_y += target[index] * fy;
        }
    }
    let centroid = [signal_x / mass, signal_y / mass];
    let expected_centroid = [
        target_x / target_mass.max(1e-12),
        target_y / target_mass.max(1e-12),
    ];
    let absolute_error_sum: f64 = errors.iter().map(|error| error.abs()).sum();
    let max_error = errors
        .iter()
        .map(|error| error.abs())
        .fold(f64::NEG_INFINITY, f64::max);
    let signal_range = [
        values.iter().copied().fold(f64::INFINITY, f64::min),
        values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    ];
    DiskProfile {
        centroid_px: centroid,
        expected_centroid_px: expected_centroid,
        centroid_error_px: (centroid[0] - expected_centroid[0])
            .hypot(centroid[1] - expected_centroid[1]),
        signal_energy: mass,
        energy_relative_error: (mass - target_mass).abs() / target_mass.max(1e-12),
        expected_energy: target_mass,
        disk_reference_rmse: error_mean_square.sqrt(),
        normalized_disk_l1: absolute_error_sum / target_mass.max(1e-12),
        normalized_disk_rmse: error_mean_square.sqrt() / target_rms.max(1e-12),
        disk_reference_max_error: max_error,
        leakage_beyond_radius_plus_1: outside_mass / mass.max(1e-12),
        signal_range,
    }
}

/// Returns the arithmetic mean of `values`, or NaN when there are none.
fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (total, count) = values.fold((0.0, 0usize), |(total, count), value| {
        (total + value, count + 1)
    });
    total / count as f64
}

/// Tests for the disk profile metrics.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a signal equal to its reference has no disk error and the expected energy.
    #[test]
    fn test_matching_reference_has_no_disk_error() {
        let plane = Plane {
            width: 5,
            height: 5,
            data: (0..25).map(|value| (value % 3) as f32 * 0.25).collect(),
        };
        let reference: Vec<f64> = plane.data.iter().map(|value| f64::from(*value)).collect();
        let profile = disk_signal_metrics(&plane, 2.0, Some((2.0, 2.0)), Some(&reference));
        assert_eq!(profile.disk_reference_rmse, 0.0);
        assert_eq!(profile.normalized_disk_l1, 0.0);
        assert!((profile.signal_energy - reference.iter().sum::<f64>()).abs() < 1e-12);
    }
}
