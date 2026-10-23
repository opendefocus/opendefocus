//! Upscales an analytic scene's `color.exr` and `coc.exr` by an integer factor for benchmark inputs.
//!
//! Color and CoC are bilinearly interpolated; CoC values are multiplied by `FACTOR / 4` so the blur
//! radius grows more slowly than the image.
//!
//! Usage: `scale_scene SCENE_DIR OUT_DIR FACTOR`
use anyhow::{Context, Result, bail, ensure};
use image::{Rgba32FImage, open};
use std::path::Path;

/// Reads `name` from `scene_dir` as RGBA f32.
fn load_rgba(scene_dir: &Path, name: &str) -> Result<Rgba32FImage> {
    let path = scene_dir.join(name);
    Ok(open(&path)
        .with_context(|| format!("reading {}", path.display()))?
        .to_rgba32f())
}

/// Bilinearly resamples `image` to `width` by `height` with pixel-centre mapping. The image crate's own resize clamps floats to 0..1, which would destroy HDR colour and signed CoC values.
fn resample(image: &Rgba32FImage, width: u32, height: u32) -> Rgba32FImage {
    let (source_width, source_height) = image.dimensions();
    Rgba32FImage::from_fn(width, height, |x, y| {
        let position = |index: u32, out: u32, source: u32| {
            let at = ((index as f32 + 0.5) * source as f32 / out as f32 - 0.5).max(0.0);
            let first = (at.floor() as u32).min(source - 1);
            (first, (first + 1).min(source - 1), at - first as f32)
        };
        let (x0, x1, fx) = position(x, width, source_width);
        let (y0, y1, fy) = position(y, height, source_height);
        let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
        image::Rgba(std::array::from_fn(|c| {
            let top = mix(image.get_pixel(x0, y0)[c], image.get_pixel(x1, y0)[c], fx);
            let bottom = mix(image.get_pixel(x0, y1)[c], image.get_pixel(x1, y1)[c], fx);
            mix(top, bottom, fy)
        }))
    })
}

/// Parses `FACTOR` as a positive integer.
fn parse_factor(text: &str) -> Result<u32> {
    let factor: u32 = text
        .parse()
        .with_context(|| format!("FACTOR must be a positive integer, got {text:?}"))?;
    ensure!(factor >= 1, "FACTOR must be at least 1");
    Ok(factor)
}

/// Writes the scaled color and CoC images for one scene into `out_dir`.
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [scene_dir, out_dir, factor] = args.as_slice() else {
        bail!("usage: scale_scene SCENE_DIR OUT_DIR FACTOR");
    };
    let factor = parse_factor(factor)?;
    let scene_dir = Path::new(scene_dir);
    let out_dir = Path::new(out_dir);
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let color = load_rgba(scene_dir, "color.exr")?;
    let mut coc = load_rgba(scene_dir, "coc.exr")?;
    ensure!(
        coc.dimensions() == color.dimensions(),
        "coc.exr and color.exr dimensions differ"
    );
    let coc_scale = factor as f32 / 4.0;
    for pixel in coc.pixels_mut() {
        for channel in &mut pixel.0 {
            *channel *= coc_scale;
        }
    }

    let (w, h) = color.dimensions();
    let (out_w, out_h) = (w * factor, h * factor);
    let color_out = resample(&color, out_w, out_h);
    let coc_out = resample(&coc, out_w, out_h);
    color_out
        .save(out_dir.join("color.exr"))
        .context("writing color.exr")?;
    coc_out
        .save(out_dir.join("coc.exr"))
        .context("writing coc.exr")?;
    Ok(())
}
