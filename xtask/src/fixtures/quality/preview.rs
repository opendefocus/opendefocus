//! Writes the sRGB preview image of a float RGBA frame, composited over a checkerboard, for
//! visual review of the QA outputs.
use std::path::Path;

use anyhow::Result;
use image::RgbImage;

use super::image::Rgba;

/// Write a checker-composited Reinhard tone-map with sRGB encoding for visual review.
pub fn write_preview(path: &Path, rgba: &Rgba) -> Result<()> {
    let mut bytes = Vec::with_capacity(rgba.width * rgba.height * 3);
    for y in 0..rgba.height {
        for x in 0..rgba.width {
            let checker = if ((x / 8 + y / 8) & 1) == 0 {
                0.12
            } else {
                0.24
            };
            for channel in 0..3 {
                let one_minus_alpha = 1.0_f32 - rgba.at(x, y, 3);
                let value = f64::from(rgba.at(x, y, channel).max(0.0))
                    + checker * f64::from(one_minus_alpha);
                let positive = value.max(0.0);
                let mapped = positive / (1.0 + positive);
                let encoded = if mapped <= 0.003_130_8 {
                    mapped * 12.92
                } else {
                    1.055 * mapped.powf(1.0 / 2.4) - 0.055
                };
                bytes.push((encoded * 255.0 + 0.5).clamp(0.0, 255.0) as u8);
            }
        }
    }
    let image = RgbImage::from_raw(rgba.width as u32, rgba.height as u32, bytes)
        .ok_or_else(|| anyhow::anyhow!("preview buffer size mismatch"))?;
    image.save(path)?;
    Ok(())
}
