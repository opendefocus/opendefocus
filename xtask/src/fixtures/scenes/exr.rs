//! Minimal uncompressed scanline `OpenEXR` writer for FLOAT RGBA images.

/// One pixel as `[r, g, b, a]` values. The writer stores each value as a 32-bit float.
pub type Rgba = [f64; 4];

/// Encodes `pixels` as an uncompressed OpenEXR scanline file with FLOAT `R`, `G`, `B` and
/// `A` channels. `pixels` is row-major and must hold `width * height` entries; the data
/// and display windows cover the whole image. Panics when `pixels` is shorter than that.
pub fn write(width: usize, height: usize, pixels: &[Rgba]) -> Vec<u8> {
    let box_bytes = box2i(width, height);
    let mut header = Vec::new();
    header.extend(20_000_630_u32.to_le_bytes());
    header.extend(2_u32.to_le_bytes());
    attribute(&mut header, "channels", "chlist", &channel_list());
    attribute(&mut header, "compression", "compression", &[0]);
    attribute(&mut header, "dataWindow", "box2i", &box_bytes);
    attribute(&mut header, "displayWindow", "box2i", &box_bytes);
    attribute(&mut header, "lineOrder", "lineOrder", &[0]);
    attribute(
        &mut header,
        "pixelAspectRatio",
        "float",
        &1.0_f32.to_le_bytes(),
    );
    let mut center = Vec::new();
    center.extend(0.0_f32.to_le_bytes());
    center.extend(0.0_f32.to_le_bytes());
    attribute(&mut header, "screenWindowCenter", "v2f", &center);
    attribute(
        &mut header,
        "screenWindowWidth",
        "float",
        &1.0_f32.to_le_bytes(),
    );
    header.push(0);

    let stride = 8 + width * 16;
    let data_start = header.len() + 8 * height;
    let mut out = header;
    for row in 0..height {
        out.extend(((data_start + stride * row) as u64).to_le_bytes());
    }
    for row in 0..height {
        let line = &pixels[row * width..(row + 1) * width];
        let mut data = Vec::with_capacity(width * 16);
        for channel in [3, 2, 1, 0] {
            for pixel in line {
                data.extend((pixel[channel] as f32).to_le_bytes());
            }
        }
        out.extend((row as i32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
    }
    out
}

/// Encodes the EXR box `[0, 0, width - 1, height - 1]` as four little-endian 32-bit
/// integers.
fn box2i(width: usize, height: usize) -> Vec<u8> {
    [0, 0, width as i32 - 1, height as i32 - 1]
        .into_iter()
        .flat_map(i32::to_le_bytes)
        .collect()
}

/// Encodes the `chlist` attribute: channels `A`, `B`, `G` and `R`, each FLOAT and sampled at
/// every pixel.
fn channel_list() -> Vec<u8> {
    let mut list = Vec::new();
    for name in ["A", "B", "G", "R"] {
        list.extend(name.as_bytes());
        list.push(0);
        list.extend(2_i32.to_le_bytes());
        list.extend([0, 0, 0, 0]);
        list.extend(1_i32.to_le_bytes());
        list.extend(1_i32.to_le_bytes());
    }
    list.push(0);
    list
}

/// Appends one header attribute: its NUL-terminated name and type, then its byte length and
/// value.
fn attribute(header: &mut Vec<u8>, name: &str, kind: &str, value: &[u8]) {
    header.extend(name.as_bytes());
    header.push(0);
    header.extend(kind.as_bytes());
    header.push(0);
    header.extend((value.len() as u32).to_le_bytes());
    header.extend(value);
}

/// Tests for the EXR writer.
#[cfg(test)]
mod tests {
    use super::{Rgba, write};

    /// Verifies that written float RGBA values decode to the same values through the image
    /// crate.
    #[test]
    fn written_float_rgba_decodes_to_the_same_values() {
        let pixels: Vec<Rgba> = vec![
            [0.5, -2.0, 16.0, 1.0],
            [0.0, 0.25, 4.0, 0.5],
            [1.0, 1.0, 1.0, 0.0],
            [3.0, 0.125, 0.0, 1.0],
        ];
        let bytes = write(2, 2, &pixels);
        let decoded = image::load_from_memory(&bytes)
            .expect("decodes as OpenEXR")
            .into_rgba32f();
        assert_eq!(decoded.dimensions(), (2, 2));
        for (index, pixel) in pixels.iter().enumerate() {
            let decoded_pixel = decoded.get_pixel((index % 2) as u32, (index / 2) as u32);
            for channel in 0..4 {
                assert_eq!(decoded_pixel.0[channel], pixel[channel] as f32);
            }
        }
    }
}
