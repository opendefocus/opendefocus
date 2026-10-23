//! Evaluate committed analytic scenes without relying on an external CLI.
use anyhow::{Result, ensure};
use image::{Rgb, RgbImage, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
use ndarray::{Array2, Array3, s};
use opendefocus::{
    OpenDefocusRenderer,
    datamodel::{
        IVector4, Settings, UVector2,
        defocus::DefocusMode,
        render::{FilterMode, Quality, RenderSpecs},
    },
};
use std::{fmt::Write as _, path::PathBuf};

/// Analytic scenes rendered by this example, in report order.
const SCENES: &[&str] = &["occlusion", "focus_ramp", "hdr_points", "alpha_edges"];

/// Converts a linear RGBA array to an 8-bit sRGB preview over a checkerboard, so transparent areas show the pattern.
fn preview(array: &Array3<f32>) -> RgbImage {
    RgbImage::from_fn(array.dim().1 as u32, array.dim().0 as u32, |x, y| {
        let bg = if (x / 8 + y / 8) % 2 == 0 { 0.3 } else { 0.08 };
        Rgb(std::array::from_fn(|c| {
            let linear = (array[[y as usize, x as usize, c]]
                + bg * (1.0 - array[[y as usize, x as usize, 3]]))
            .max(0.0);
            let mapped = linear / (1.0 + linear);
            let srgb = if mapped <= 0.0031308 {
                12.92 * mapped
            } else {
                1.055 * mapped.powf(1.0 / 2.4) - 0.055
            };
            (srgb * 255.0).round() as u8
        }))
    })
}

/// Renders the scene in 32 and 64 pixel stripes, joins them, and requires the join to match the full render.
///
/// # Errors
/// Returns an error when a stripe cannot be rendered, the join is not finite, or it differs from `full` by more than 1e-5.
async fn check_stripe_joins(
    renderer: &OpenDefocusRenderer,
    config: &mut Settings,
    (input, depth, filter): (&Array3<f32>, &Array2<f32>, Option<&Array3<f32>>),
    full: &Array3<f32>,
    label: &str,
    report: &mut String,
) -> Result<()> {
    for stripe_height in [32, 64] {
        let mut joined = input.clone();
        config.render.resolution = UVector2 { x: 192, y: 128 };
        for y in (0..128).step_by(stripe_height) {
            // Full immutable input with a restricted output region provides all halos.
            let mut stripe = input.clone();
            renderer
                .render_stripe(
                    RenderSpecs {
                        full_region: IVector4 {
                            x: 0,
                            y: 0,
                            z: 192,
                            w: 128,
                        },
                        render_region: IVector4 {
                            x: 0,
                            y: y as i32,
                            z: 192,
                            w: (y + stripe_height).min(128) as i32,
                        },
                    },
                    config.clone(),
                    stripe.view_mut(),
                    Some(depth.clone()),
                    filter.cloned(),
                )
                .await?;
            joined
                .slice_mut(s!(y..(y + stripe_height).min(128), .., ..))
                .assign(&stripe.slice(s!(y..(y + stripe_height).min(128), .., ..)));
        }
        let max_diff = full
            .iter()
            .zip(joined.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        writeln!(
            report,
            "  {stripe_height}px stripes: max absolute difference={max_diff:.9}"
        )?;
        ensure!(
            joined.iter().all(|v| v.is_finite()),
            "Nonfinite stripe output"
        );
        ensure!(max_diff <= 1e-5, "Stripe mismatch: {label}: {max_diff}");
    }
    Ok(())
}

#[tokio::main]
/// Renders every committed diagnostic scene and writes its previews and report to the output directory.
///
/// # Errors
/// Returns an error when a scene cannot be rendered or an output cannot be written.
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map_or_else(|| PathBuf::from("diagnostic-results"), PathBuf::from);
    std::fs::create_dir_all(&output)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/images/scenes");
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = true;
    settings.defocus.circle_of_confusion.max_size = 24.0;
    settings.defocus.camera_max_size = 24.0;
    settings.defocus.size_multiplier = 1.0;
    settings.defocus.proxy_scale = Some(1.0);
    settings.render.set_quality(Quality::Accurate);
    settings.render.set_farm_quality(Quality::Accurate);
    let renderer = OpenDefocusRenderer::new(&mut settings).await?;
    let mut report = format!(
        "Visual diagnostics, not correctness oracles\nBackend: {:?}\nQuality: Accurate; gamma=1; max radius=24px; direct signed CoC\n",
        settings.render.device_name
    );
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unknown".to_string(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_string(),
        );
    writeln!(
        report,
        "Checkout HEAD: {revision} (may include local changes)"
    )?;
    let mut panels = Vec::new();
    for name in SCENES {
        let input = image::open(root.join(name).join("color.exr"))?
            .to_rgba32f()
            .to_ndarray();
        let coc_image = image::open(root.join(name).join("coc.exr"))?.to_rgba32f();
        let depth = Array2::from_shape_fn((128, 192), |(y, x)| {
            coc_image.get_pixel(x as u32, y as u32)[0]
        });
        for custom in [false, true] {
            if custom && *name != "hdr_points" {
                continue;
            }
            let mut config = settings.clone();
            let filter = if custom {
                config.render.filter.set_mode(FilterMode::Image);
                config.render.filter.resolution = 33;
                Some(
                    image::open(root.join(name).join("aperture.exr"))?
                        .to_rgba32f()
                        .to_ndarray(),
                )
            } else {
                None
            };
            let label = if custom {
                format!("{name}_asymmetric")
            } else {
                name.to_string()
            };
            let mut full = input.clone();
            renderer
                .render(
                    config.clone(),
                    full.view_mut(),
                    Some(depth.clone()),
                    filter.clone(),
                )
                .await?;
            ensure!(
                full.iter().all(|v| v.is_finite()),
                "Nonfinite output: {label}"
            );
            let alpha_min = full
                .slice(s!(.., .., 3))
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let alpha_max = full
                .slice(s!(.., .., 3))
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let rgb_max = full
                .slice(s!(.., .., 0..3))
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            writeln!(
                report,
                "{label}: RGB max={rgb_max:.6}, alpha=[{alpha_min:.6},{alpha_max:.6}]"
            )?;
            let input_sum: f64 = input
                .slice(s!(.., .., 0..3))
                .iter()
                .map(|v| *v as f64)
                .sum();
            let output_sum: f64 = full.slice(s!(.., .., 0..3)).iter().map(|v| *v as f64).sum();
            writeln!(
                report,
                "  RGB sum input={input_sum:.6}, output={output_sum:.6} (diagnostic, no conservation assertion)"
            )?;
            check_stripe_joins(
                &renderer,
                &mut config,
                (&input, &depth, filter.as_ref()),
                &full,
                &label,
                &mut report,
            )
            .await?;
            Rgba32FImage::from_ndarray(full.clone())?.save(output.join(format!("{label}.exr")))?;
            let rendered = preview(&full);
            rendered.save(output.join(format!("{label}.png")))?;
            panels.push((preview(&input), rendered));
        }
    }
    let mut sheet = RgbImage::new(384, panels.len() as u32 * 128);
    for (row, (source, rendered)) in panels.iter().enumerate() {
        image::imageops::replace(&mut sheet, source, 0, row as i64 * 128);
        image::imageops::replace(&mut sheet, rendered, 192, row as i64 * 128);
    }
    sheet.save(output.join("render_contact_sheet.png"))?;
    std::fs::write(output.join("report.txt"), &report)?;
    print!("{report}");
    Ok(())
}
