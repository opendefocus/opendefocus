//! PNG encoding for the committed scene previews, and a pixel-level comparison that ignores
//! how the PNG container was compressed.
use anyhow::Result;
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

/// Encodes row-major RGB8 pixels of `width` by `height` as a PNG with best compression and
/// no filtering. Fails when the encoder rejects the data.
pub fn encode_rgb8(width: usize, height: usize, pixels: &[[u8; 3]]) -> Result<Vec<u8>> {
    let bytes: Vec<u8> = pixels.iter().flatten().copied().collect();
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Best, FilterType::NoFilter)
        .write_image(&bytes, width as u32, height as u32, ExtendedColorType::Rgb8)?;
    Ok(out)
}
/// Decode both PNGs to 8-bit RGB pixels. zlib's deflate stream is implementation-specific,
/// so committed previews are compared by content, not by container bytes.
pub fn same_pixels(committed: &[u8], expected: &[u8]) -> Result<bool> {
    let committed = image::load_from_memory(committed)?.into_rgb8();
    let expected = image::load_from_memory(expected)?.into_rgb8();
    Ok(committed.dimensions() == expected.dimensions() && committed.as_raw() == expected.as_raw())
}
