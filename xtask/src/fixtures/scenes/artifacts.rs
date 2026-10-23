//! The committed scene assets: EXR channels, PNG previews, aperture mask, contact sheet and metadata.

use anyhow::Result;

use super::{
    analytic::{HEIGHT, WIDTH, py_max, py_min, scene},
    exr::{self, Rgba},
    json::Json,
    png,
};

/// Scene names in output order.
pub const SCENES: [&str; 4] = ["occlusion", "focus_ramp", "hdr_points", "alpha_edges"];
/// Command recorded in each metadata file to name its generator.
pub const GENERATOR: &str = "cargo xtask fixtures scenes";

/// One generated file: its path relative to `test/images/scenes` and its exact bytes.
pub struct Asset {
    /// Path relative to `test/images/scenes`, with forward slashes.
    pub path: String,
    /// Contents of the file.
    pub bytes: Vec<u8>,
}

/// Generates every committed scene asset in memory: per scene the colour, depth and CoC EXR
/// files, both PNG previews and the metadata, then the aperture mask and the contact sheet.
/// Fails when a PNG cannot be encoded.
pub fn artifacts() -> Result<Vec<Asset>> {
    let mut assets = Vec::new();
    let mut gallery = Vec::new();
    for name in SCENES {
        let scene = scene(name);
        let coc: Vec<f64> = scene.depth.iter().map(|z| 24.0 * (1.0 - 4.0 / z)).collect();
        let preview = preview_pixels(&scene.color);
        let coc_preview = coc_pixels(&coc);
        let world_depth: Vec<Rgba> = scene.depth.iter().map(|&z| [z, z, z, 1.0]).collect();
        let coc_pixels_exr: Vec<Rgba> = coc.iter().map(|&c| [c, c, c, 1.0]).collect();
        assets.push(asset(
            format!("{name}/color.exr"),
            exr::write(WIDTH, HEIGHT, &scene.color),
        ));
        assets.push(asset(
            format!("{name}/depth.exr"),
            exr::write(WIDTH, HEIGHT, &world_depth),
        ));
        assets.push(asset(
            format!("{name}/coc.exr"),
            exr::write(WIDTH, HEIGHT, &coc_pixels_exr),
        ));
        assets.push(asset(
            format!("{name}/preview.png"),
            png::encode_rgb8(WIDTH, HEIGHT, &preview)?,
        ));
        assets.push(asset(
            format!("{name}/coc_preview.png"),
            png::encode_rgb8(WIDTH, HEIGHT, &coc_preview)?,
        ));
        assets.push(asset(
            format!("{name}/metadata.json"),
            metadata(name, &scene.depth, &coc),
        ));
        gallery.push((preview, coc_preview));
    }
    assets.extend(aperture_assets()?);
    assets.push(asset(
        "contact_sheet.png".to_owned(),
        png::encode_rgb8(2 * WIDTH, 4 * HEIGHT, &contact_sheet(&gallery))?,
    ));
    Ok(assets)
}

/// Wraps a path and its contents as an `Asset`.
fn asset(path: String, bytes: Vec<u8>) -> Asset {
    Asset { path, bytes }
}

/// Converts the premultiplied colour buffer to 8-bit preview pixels. The colour is composited
/// over a checkerboard of two background levels, then passed through the fixed display tone
/// map.
fn preview_pixels(color: &[Rgba]) -> Vec<[u8; 3]> {
    color
        .iter()
        .enumerate()
        .map(|(index, pixel)| {
            let (x, y) = (index % WIDTH, index / WIDTH);
            let background = if (x / 8 + y / 8) % 2 == 1 { 0.08 } else { 0.3 };
            [0, 1, 2].map(|channel| {
                super::analytic::display(pixel[channel] + background * (1.0 - pixel[3]))
            })
        })
        .collect()
}

/// Maps the circle of confusion to 8-bit preview colours: red for positive (background) CoC,
/// blue for negative (foreground) CoC, and green for values near focus.
fn coc_pixels(coc: &[f64]) -> Vec<[u8; 3]> {
    coc.iter()
        .map(|&c| {
            [
                (255.0 * py_max(0.0, c) / 24.0).round_ties_even() as u8,
                (255.0 * (1.0 - py_min(1.0, c.abs() / 24.0))).round_ties_even() as u8,
                (255.0 * py_max(0.0, -c) / 24.0).round_ties_even() as u8,
            ]
        })
        .collect()
}

/// Returns the 33 by 33 aperture mask as an EXR and a PNG in the `hdr_points` folder. The
/// mask is a disk with a cut-off on the left and a notch at the lower right.
fn aperture_assets() -> Result<Vec<Asset>> {
    let mut mask: Vec<Rgba> = Vec::with_capacity(33 * 33);
    for y in 0..33 {
        for x in 0..33 {
            let u = (f64::from(x) - 16.0) / 16.0;
            let v = (f64::from(y) - 16.0) / 16.0;
            let inside = u * u + v * v < 0.9 && u > -0.45 && !(u > 0.15 && v < -0.1);
            let value = if inside { 1.0 } else { 0.0 };
            mask.push([value, value, value, 1.0]);
        }
    }
    let rgb: Vec<[u8; 3]> = mask
        .iter()
        .map(|p| [0, 1, 2].map(|c| (255.0 * p[c]).round_ties_even() as u8))
        .collect();
    Ok(vec![
        asset(
            "hdr_points/aperture.exr".to_owned(),
            exr::write(33, 33, &mask),
        ),
        asset(
            "hdr_points/aperture.png".to_owned(),
            png::encode_rgb8(33, 33, &rgb)?,
        ),
    ])
}

/// Lays out the colour and CoC previews of every scene in one image: for each scene, the
/// colour preview beside its CoC preview, row by row.
fn contact_sheet(gallery: &[(Vec<[u8; 3]>, Vec<[u8; 3]>)]) -> Vec<[u8; 3]> {
    let mut sheet = Vec::with_capacity(2 * WIDTH * 4 * HEIGHT);
    for (preview, coc_preview) in gallery {
        for row in 0..HEIGHT {
            let span = row * WIDTH..(row + 1) * WIDTH;
            sheet.extend_from_slice(&preview[span.clone()]);
            sheet.extend_from_slice(&coc_preview[span]);
        }
    }
    sheet
}

/// Returns the pretty-printed JSON metadata for scene `name`, with a trailing newline. It
/// records the resolution, provenance, camera, render, colour and depth conventions, and the
/// depth and CoC ranges.
fn metadata(name: &str, depth: &[f64], coc: &[f64]) -> Vec<u8> {
    let depth_range = range(depth);
    let coc_range = range(coc);
    let document = Json::Object(vec![
        ("schema", Json::Int(1)),
        ("scene", Json::str(name)),
        ("resolution", Json::ints(&[WIDTH as i64, HEIGHT as i64])),
        ("seed", Json::Int(0)),
        ("generator", Json::str(GENERATOR)),
        ("license", Json::str("CC0-1.0")),
        (
            "provenance",
            Json::str(
                "Original analytic surfaces; no downloaded or renderer-produced source assets.",
            ),
        ),
        (
            "camera",
            Json::Object(vec![
                ("projection", Json::str("orthographic analytic pixel chart")),
                ("pixel_aspect", Json::Int(1)),
                ("focus_distance_m", Json::Int(4)),
            ]),
        ),
        (
            "render",
            Json::Object(vec![
                (
                    "sampling",
                    Json::str("pixel centers; analytic alpha edge coverage"),
                ),
                ("engine", Json::str("Rust standard library")),
                ("denoise", Json::Bool(false)),
            ]),
        ),
        (
            "color",
            Json::Object(vec![
                ("space", Json::str("linear sRGB / Rec.709 primaries")),
                ("storage", Json::str("OpenEXR FLOAT RGBA")),
                ("alpha", Json::str("premultiplied")),
                ("exposure", Json::Int(0)),
            ]),
        ),
        (
            "depth",
            Json::Object(vec![
                ("units", Json::str("meters")),
                ("direction", Json::str("positive away from camera")),
                ("storage_channel", Json::str("R (replicated in G/B)")),
                ("range", Json::floats(&depth_range)),
            ]),
        ),
        (
            "coc",
            Json::Object(vec![
                ("units", Json::str("pixel radius")),
                ("formula", Json::str("24 * (1 - 4 / depth_m)")),
                ("negative", Json::str("foreground")),
                ("positive", Json::str("background")),
                ("range", Json::floats(&coc_range)),
                (
                    "renderer",
                    Json::str(
                        "Depth mode, use_direct_math=true, size_multiplier=1, proxy_scale=1, max_size=24; load coc.exr R",
                    ),
                ),
            ]),
        ),
        (
            "preview",
            Json::str(
                "Fixed RGB/(1+RGB), then sRGB OETF; alpha composited over linear checker. CoC preview: R=far, B=near, G=focus.",
            ),
        ),
    ]);
    let mut text = document.to_pretty_string();
    text.push('\n');
    text.into_bytes()
}

/// Returns the minimum and maximum of `values`, or `[0.0, 0.0]` when the slice is empty.
fn range(values: &[f64]) -> [f64; 2] {
    let low = values.iter().copied().reduce(py_min).unwrap_or(0.0);
    let high = values.iter().copied().reduce(py_max).unwrap_or(0.0);
    [low, high]
}
