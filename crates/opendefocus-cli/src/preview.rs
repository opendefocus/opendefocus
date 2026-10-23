//! Fixed PNG preview: composites a float result over a checkerboard, tone maps it with
//! Reinhard, and encodes it with the sRGB transfer function.

use ndarray::Array3;

/// Builds an opaque sRGB preview of a height x width x 4 premultiplied float image.
///
/// The result is composited over an 8-pixel checkerboard (dark and light gray) so that
/// transparency is visible, then mapped with `x / (1 + x)` and encoded as sRGB. The
/// returned alpha is always 1.0.
pub fn fixed_preview(source: &Array3<f32>) -> Array3<f32> {
    let (height, width, _) = source.dim();
    let mut preview = Array3::zeros((height, width, 4));
    for y in 0..height {
        for x in 0..width {
            let checker = if (x / 8 + y / 8) % 2 == 0 { 0.3 } else { 0.08 };
            let alpha = source[[y, x, 3]].clamp(0.0, 1.0);
            for channel in 0..3 {
                let linear = (source[[y, x, channel]] + checker * (1.0 - alpha)).max(0.0);
                let mapped = linear / (1.0 + linear);
                preview[[y, x, channel]] = if mapped <= 0.003_130_8 {
                    mapped * 12.92
                } else {
                    1.055 * mapped.powf(1.0 / 2.4) - 0.055
                };
            }
            preview[[y, x, 3]] = 1.0;
        }
    }
    preview
}
