//! Zero-depth seam metric: how strongly the luma gradient at a column stands out from its
//! neighbours. A large ratio flags a vertical seam at the zero-depth column.
use anyhow::{Result, bail};

use crate::fixtures::quality::image::{Rgba, median_f64};

/// Gradient statistics around the zero-depth column.
#[derive(Clone, Debug)]
pub struct Seam {
    /// Mean luma gradient at the seam column.
    pub gradient_mean: f64,
    /// Largest luma gradient at the seam column.
    pub gradient_max: f64,
    /// Median of the luma gradients a few columns away from the seam.
    pub neighbor_gradient_median: f64,
    /// Largest seam gradient divided by the neighbour median, which is at least 1e-8.
    pub gradient_ratio: f64,
}

/// Measure whether a zero-depth column creates an artificial vertical luma edge.
pub fn zero_depth_seam_metrics(image: &Rgba, seam_x: i64) -> Result<Seam> {
    if !(1..image.width as i64 - 1).contains(&seam_x) {
        bail!("zero-depth seam metric requires an interior column in an RGB image");
    }
    let luma = image.luma();
    let gradients: Vec<f64> = (0..image.width - 1)
        .map(|column| {
            let total: f64 = (0..image.height)
                .map(|row| f64::from(luma.at(column + 1, row) - luma.at(column, row)).abs())
                .sum();
            total / image.height as f64
        })
        .collect();
    let seam_x = seam_x as usize;
    let seam = slice(&gradients, seam_x.saturating_sub(1), seam_x + 1);
    let mut local = slice(
        &gradients,
        seam_x.saturating_sub(8),
        seam_x.saturating_sub(3),
    )
    .to_vec();
    local.extend_from_slice(slice(&gradients, seam_x + 3, seam_x + 8));
    let local_median = if local.is_empty() {
        0.0
    } else {
        median_f64(&mut local)
    };
    let (seam_mean, seam_max) = if seam.is_empty() {
        (0.0, 0.0)
    } else {
        (
            seam.iter().sum::<f64>() / seam.len() as f64,
            seam.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    Ok(Seam {
        gradient_mean: seam_mean,
        gradient_max: seam_max,
        neighbor_gradient_median: local_median,
        gradient_ratio: seam_max / local_median.max(1e-8),
    })
}

/// Python-style clamped slice: out-of-range bounds are trimmed and inverted bounds are empty.
fn slice(values: &[f64], start: usize, end: usize) -> &[f64] {
    let end = end.min(values.len());
    let start = start.min(end);
    &values[start..end]
}

/// Tests for the zero-depth seam metric.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a vertical luma edge at the seam column gives a gradient ratio of at least 3.
    #[test]
    fn test_seam_detects_vertical_edge_at_column() {
        let image = Rgba::from_fn(32, 16, |x, _| {
            let value = if x == 16 { 0.75 } else { 0.5 };
            [value, value, value, 1.0]
        });
        let seam = zero_depth_seam_metrics(&image, 16).unwrap();
        assert!(seam.gradient_ratio >= 3.0, "{seam:?}");
    }

    /// Verifies that seam columns on the image border are rejected.
    #[test]
    fn test_seam_rejects_border_columns() {
        let image = Rgba::from_fn(8, 4, |_, _| [0.0; 4]);
        assert!(zero_depth_seam_metrics(&image, 0).is_err());
        assert!(zero_depth_seam_metrics(&image, 7).is_err());
    }
}
