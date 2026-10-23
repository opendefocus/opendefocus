//! Pixel comparison of the base and head renders, with an amplified difference image.
use anyhow::{Context, Result, ensure};
use image::{GrayImage, Luma};
use std::{fs, path::Path};

/// Factor applied to the absolute difference so small changes are visible in the image.
const DIFFERENCE_GAIN: f32 = 16.0;

/// How far two renders of the same scene are apart.
#[derive(Debug, PartialEq)]
pub struct Difference {
    /// Largest absolute difference of any channel.
    pub max_abs: f32,
    /// Root mean square difference over all channels.
    pub rms: f32,
}

/// Compares two float images of equal size.
///
/// Returns the summary and the per-pixel largest channel difference.
///
/// # Errors
/// Fails when the dimensions differ.
fn measure(
    base: &image::Rgba32FImage,
    head: &image::Rgba32FImage,
) -> Result<(Difference, Vec<f32>)> {
    ensure!(
        base.dimensions() == head.dimensions(),
        "Base and head renders differ in size: {:?} against {:?}",
        base.dimensions(),
        head.dimensions()
    );
    let mut per_pixel = Vec::with_capacity(base.as_raw().len() / 4);
    let mut max_abs = 0.0f32;
    let mut squares = 0.0f64;
    for (a, b) in base.pixels().zip(head.pixels()) {
        let mut pixel_max = 0.0f32;
        for (x, y) in a.0.iter().zip(b.0) {
            let delta = (x - y).abs();
            pixel_max = pixel_max.max(delta);
            squares += f64::from(delta) * f64::from(delta);
        }
        max_abs = max_abs.max(pixel_max);
        per_pixel.push(pixel_max);
    }
    let channels = base.as_raw().len().max(1) as f64;
    let rms = (squares / channels).sqrt() as f32;
    Ok((Difference { max_abs, rms }, per_pixel))
}

/// Compares the EXR files at `base` and `head` and writes the amplified difference to `png`.
///
/// # Errors
/// Fails when an image cannot be read or the sizes differ, or the PNG cannot be written.
pub fn compare(base: &Path, head: &Path, png: &Path) -> Result<Difference> {
    let load = |path: &Path| {
        image::open(path)
            .with_context(|| format!("Reading {}", path.display()))
            .map(|image| image.to_rgba32f())
    };
    let (base, head) = (load(base)?, load(head)?);
    let (difference, per_pixel) = measure(&base, &head)?;
    let (width, height) = base.dimensions();
    let mut visual = GrayImage::new(width, height);
    for (pixel, delta) in visual.pixels_mut().zip(per_pixel) {
        *pixel = Luma([((delta * DIFFERENCE_GAIN).min(1.0) * 255.0) as u8]);
    }
    if let Some(parent) = png.parent() {
        fs::create_dir_all(parent)?;
    }
    visual.save(png)?;
    Ok(difference)
}

/// Tests for the difference measure.
#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Verifies identical images give zero and one changed channel gives its size.
    #[test]
    fn test_measure_reports_max_and_rms() {
        let base = image::Rgba32FImage::from_pixel(2, 1, Rgba([0.5; 4]));
        let mut head = base.clone();
        assert_eq!(measure(&base, &head).unwrap().0.max_abs, 0.0);
        head.put_pixel(1, 0, Rgba([0.5, 0.5, 0.5, 0.9]));
        let (difference, per_pixel) = measure(&base, &head).unwrap();
        assert!((difference.max_abs - 0.4).abs() < 1e-6);
        assert!((difference.rms - (0.16f32 / 8.0).sqrt()).abs() < 1e-6);
        assert_eq!(per_pixel.len(), 2);
    }

    /// Verifies that images of different sizes are rejected.
    #[test]
    fn test_measure_rejects_different_sizes() {
        let a = image::Rgba32FImage::new(2, 2);
        let b = image::Rgba32FImage::new(3, 2);
        assert!(measure(&a, &b).is_err());
    }
}
