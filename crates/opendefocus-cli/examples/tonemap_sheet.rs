//! Tone-maps premultiplied float images for viewing: composite over a linear checker,
//! `x / (1 + x)`, sRGB OETF, 3x nearest upscale; inputs are tiled in rows of three.
//!
//! `tonemap_sheet OUT.png IN.exr [IN.exr ...]`
use anyhow::{Result, ensure};

/// Integer upscale factor applied to each input image.
const SCALE: u32 = 3;

/// Encodes a linear value in `[0, 1]` with the sRGB transfer function as an 8-bit level.
fn srgb(value: f32) -> u8 {
    let v = value.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5) as u8
}

/// Writes a PNG contact sheet of the given EXR inputs, three per row.
///
/// # Errors
///
/// Returns an error if no inputs are given, an input cannot be read, or the sheet cannot be saved.
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(args.len() >= 2, "usage: tonemap_sheet OUT.png IN.exr...");
    let images: Vec<_> = args[1..]
        .iter()
        .map(|path| image::open(path).map(image::DynamicImage::into_rgba32f))
        .collect::<Result<_, _>>()?;
    let (tile_w, tile_h) = (images[0].width() * SCALE, images[0].height() * SCALE);
    let columns = images.len().min(3) as u32;
    let rows = images.len().div_ceil(3) as u32;
    let mut sheet = image::RgbImage::new(columns * tile_w, rows * tile_h);
    for (index, image) in images.iter().enumerate() {
        let (ox, oy) = ((index as u32 % 3) * tile_w, (index as u32 / 3) * tile_h);
        for y in 0..tile_h.min(image.height() * SCALE) {
            for x in 0..tile_w.min(image.width() * SCALE) {
                let pixel = image.get_pixel(x / SCALE, y / SCALE).0;
                let checker = if ((x / (8 * SCALE)) + (y / (8 * SCALE))).is_multiple_of(2) {
                    0.1
                } else {
                    0.4
                };
                let rgb = [0, 1, 2].map(|c| {
                    let v = pixel[c] + (1.0 - pixel[3]) * checker;
                    srgb(v / (1.0 + v))
                });
                sheet.put_pixel(ox + x, oy + y, image::Rgb(rgb));
            }
        }
    }
    sheet.save(&args[0])?;
    Ok(())
}
