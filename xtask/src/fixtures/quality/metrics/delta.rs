//! Difference statistics between two float RGBA frames: over the whole frame, and inside the
//! boundary band around a moving silhouette.
use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::fixtures::quality::image::{Rgba, quantile};

/// Mean and extreme absolute differences between two frames.
#[derive(Clone, Debug)]
pub struct Delta {
    /// Mean absolute difference over all RGBA values.
    pub mae_rgba: f64,
    /// 99th percentile of the absolute RGBA differences.
    pub p99_abs_rgba: f64,
    /// Largest absolute RGBA difference.
    pub max_abs_rgba: f64,
    /// Mean absolute difference over the RGB values only.
    pub mae_rgb: f64,
    /// Mean absolute difference of the alpha values.
    pub mae_alpha: f64,
}

impl Delta {
    /// Serialises the statistics to a JSON object.
    pub fn to_value(&self) -> Value {
        json!({
            "mae_rgba": self.mae_rgba,
            "p99_abs_rgba": self.p99_abs_rgba,
            "max_abs_rgba": self.max_abs_rgba,
            "mae_rgb": self.mae_rgb,
            "mae_alpha": self.mae_alpha,
        })
    }
}

/// Absolute RGBA difference statistics between two frames of identical shape.
pub fn image_delta(first: &Rgba, second: &Rgba) -> Result<Delta> {
    if first.shape() != second.shape() {
        bail!(
            "Output shape changed: {:?} vs {:?}",
            first.shape(),
            second.shape()
        );
    }
    let deltas: Vec<f64> = first
        .data
        .iter()
        .zip(&second.data)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
        .collect();
    let count = deltas.len() as f64;
    let rgb: Vec<f64> = deltas
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| pixel[..3].iter().copied())
        .collect();
    let alpha: Vec<f64> = deltas
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect();
    let mut sorted = deltas.clone();
    Ok(Delta {
        mae_rgba: deltas.iter().sum::<f64>() / count,
        p99_abs_rgba: quantile(&mut sorted, 0.99),
        max_abs_rgba: deltas.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        mae_rgb: rgb.iter().sum::<f64>() / rgb.len() as f64,
        mae_alpha: alpha.iter().sum::<f64>() / alpha.len() as f64,
    })
}

/// Differences between two frames of a moving sequence, over the whole frame and inside the
/// boundary band around the silhouette edge.
#[derive(Clone, Debug)]
pub struct MotionDelta {
    /// Mean absolute RGBA difference over the whole frame.
    pub mae_rgba: f64,
    /// Mean absolute RGBA difference inside the boundary band.
    pub boundary_band_mae_rgba: f64,
    /// 99th percentile of the absolute channel differences inside the band.
    pub boundary_band_p99_abs_channel: f64,
    /// Largest absolute channel difference inside the band.
    pub boundary_band_max_abs_channel: f64,
    /// Summed RGB difference over the frame, divided by the summed RGB signal.
    pub normalized_difference_energy: f64,
    /// Summed RGB difference inside the band, divided by the summed RGB signal inside the band.
    pub boundary_band_normalized_difference_energy: f64,
    /// Number of pixels inside the boundary band.
    pub boundary_band_pixels: usize,
}

impl MotionDelta {
    /// Serialises the motion statistics to a JSON object.
    pub fn to_value(&self) -> Value {
        json!({
            "mae_rgba": self.mae_rgba,
            "boundary_band_mae_rgba": self.boundary_band_mae_rgba,
            "boundary_band_p99_abs_channel": self.boundary_band_p99_abs_channel,
            "boundary_band_max_abs_channel": self.boundary_band_max_abs_channel,
            "normalized_difference_energy": self.normalized_difference_energy,
            "boundary_band_normalized_difference_energy": self.boundary_band_normalized_difference_energy,
            "boundary_band_pixels": self.boundary_band_pixels,
        })
    }
}

/// Compare aligned frames globally and within the moving silhouette boundary band.
pub fn motion_delta(
    first: &Rgba,
    second: &Rgba,
    center: (f64, f64),
    band_radius: f64,
) -> Result<MotionDelta> {
    if first.shape() != second.shape() {
        bail!(
            "Output shape changed: {:?} vs {:?}",
            first.shape(),
            second.shape()
        );
    }
    let deltas: Vec<f64> = first
        .data
        .iter()
        .zip(&second.data)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
        .collect();
    let mut band_pixels = Vec::new();
    let mut signal_mass = 0.0;
    let mut band_mass = 0.0;
    for y in 0..second.height {
        for x in 0..second.width {
            let index = (y * second.width + x) * 4;
            let distance =
                ((x as f64 + 0.5 - center.0).hypot(y as f64 + 0.5 - center.1) - 28.0).abs();
            let rgb_mass: f64 = (0..3)
                .map(|c| f64::from(second.data[index + c]).abs())
                .sum();
            signal_mass += rgb_mass;
            if distance <= band_radius {
                band_mass += rgb_mass;
                band_pixels.push(index);
            }
        }
    }
    let rgb_delta = |index: usize| deltas[index..index + 3].iter().sum::<f64>();
    let band_values: Vec<f64> = band_pixels
        .iter()
        .flat_map(|index| deltas[*index..*index + 4].iter().copied())
        .collect();
    let band_rgb_delta: f64 = band_pixels.iter().map(|index| rgb_delta(*index)).sum();
    let all_rgb_delta: f64 = (0..deltas.len() / 4)
        .map(|pixel| rgb_delta(pixel * 4))
        .sum();
    let mut sorted_band = band_values.clone();
    let count = deltas.len() as f64;
    Ok(MotionDelta {
        mae_rgba: deltas.iter().sum::<f64>() / count,
        boundary_band_mae_rgba: band_values.iter().sum::<f64>() / band_values.len() as f64,
        boundary_band_p99_abs_channel: quantile(&mut sorted_band, 0.99),
        boundary_band_max_abs_channel: band_values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max),
        normalized_difference_energy: all_rgb_delta / signal_mass.max(1e-12),
        boundary_band_normalized_difference_energy: band_rgb_delta / band_mass.max(1e-12),
        boundary_band_pixels: band_pixels.len(),
    })
}

/// Tests for the frame difference statistics.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that identical frames have no difference.
    #[test]
    fn test_identical_frames_have_no_delta() {
        let image = Rgba::from_fn(4, 3, |x, y| [x as f32, y as f32, 0.5, 1.0]);
        let delta = image_delta(&image, &image).unwrap();
        assert_eq!(delta.mae_rgba, 0.0);
        assert_eq!(delta.max_abs_rgba, 0.0);
    }

    /// Verifies the RGB and alpha means and the maximum of a known difference.
    #[test]
    fn test_image_delta_reports_channel_means() {
        let first = Rgba::from_fn(2, 1, |_, _| [0.0, 0.0, 0.0, 1.0]);
        let second = Rgba::from_fn(2, 1, |_, _| [0.5, 0.0, 0.0, 0.0]);
        let delta = image_delta(&first, &second).unwrap();
        assert!((delta.mae_rgb - 0.5 / 3.0).abs() < 1e-12);
        assert_eq!(delta.mae_alpha, 1.0);
        assert_eq!(delta.max_abs_rgba, 1.0);
    }

    /// Verifies that frames of different sizes are rejected by the motion comparison.
    #[test]
    fn test_motion_delta_rejects_shape_changes() {
        let first = Rgba::from_fn(2, 2, |_, _| [0.0; 4]);
        let second = Rgba::from_fn(3, 2, |_, _| [0.0; 4]);
        assert!(motion_delta(&first, &second, (1.0, 1.0), 8.0).is_err());
    }
}
