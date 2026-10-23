//! Compare two EXR renders after `v / (1 + v)` tonemapping: PSNR, block SSIM on luma, and absolute error.
//!
//! Usage: `compare_exr REFERENCE.exr CANDIDATE.exr [DIFF.png]`
use anyhow::{Context, Result, bail, ensure};
use image::{Rgb, RgbImage};
use std::path::Path;

/// Peak of the tonemapped range, used by PSNR.
const PEAK: f64 = 1.0;
/// SSIM stabilisers for a peak of 1.0.
const C1: f64 = 0.0001;
/// Second SSIM stabiliser for a peak of 1.0.
const C2: f64 = 0.0009;
/// Side length of the non-overlapping SSIM blocks.
const SSIM_BLOCK: usize = 8;

/// Loads an EXR as row-major RGB samples after per-channel Reinhard tonemapping.
fn load(path: &Path) -> Result<(u32, u32, Vec<[f32; 3]>)> {
    let img = image::open(path)
        .with_context(|| format!("reading {}", path.display()))?
        .to_rgba32f();
    let (w, h) = img.dimensions();
    let tonemap = |v: f32| {
        let v = v.max(0.0);
        v / (1.0 + v)
    };
    let pixels = img
        .pixels()
        .map(|p| [0, 1, 2].map(|c| tonemap(p.0[c])))
        .collect();
    Ok((w, h, pixels))
}

/// Mean SSIM over non-overlapping 8x8 blocks of two luma planes; edge blocks are clipped, not dropped.
fn mean_ssim(ref_luma: &[f64], cand_luma: &[f64], width: usize, height: usize) -> f64 {
    let mut total = 0.0;
    let mut blocks = 0usize;
    for top in (0..height).step_by(SSIM_BLOCK) {
        for left in (0..width).step_by(SSIM_BLOCK) {
            let idx: Vec<usize> = (top..(top + SSIM_BLOCK).min(height))
                .flat_map(|y| (left..(left + SSIM_BLOCK).min(width)).map(move |x| y * width + x))
                .collect();
            let n = idx.len() as f64;
            let mean_r = idx.iter().map(|&i| ref_luma[i]).sum::<f64>() / n;
            let mean_c = idx.iter().map(|&i| cand_luma[i]).sum::<f64>() / n;
            let (mut var_r, mut var_c, mut cov) = (0.0, 0.0, 0.0);
            for &i in &idx {
                let (dr, dc) = (ref_luma[i] - mean_r, cand_luma[i] - mean_c);
                var_r += dr * dr;
                var_c += dc * dc;
                cov += dr * dc;
            }
            let (va, vb, cov) = (var_r / n, var_c / n, cov / n);
            let (ma, mb) = (mean_r, mean_c);
            total += ((2.0 * ma * mb + C1) * (2.0 * cov + C2))
                / ((ma * ma + mb * mb + C1) * (va + vb + C2));
            blocks += 1;
        }
    }
    total / blocks as f64
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [reference, candidate, rest @ ..] = args.as_slice() else {
        bail!("usage: compare_exr REFERENCE.exr CANDIDATE.exr [DIFF.png]");
    };
    let (width, height, ref_px) = load(Path::new(reference))?;
    let (cand_w, cand_h, cand_px) = load(Path::new(candidate))?;
    ensure!(
        (width, height) == (cand_w, cand_h),
        "dimension mismatch: {width}x{height} vs {cand_w}x{cand_h}"
    );
    ensure!(!ref_px.is_empty(), "empty image");

    let (mut sq, mut sum_abs, mut max_abs) = (0.0f64, 0.0f64, 0.0f32);
    for (p, q) in ref_px.iter().zip(&cand_px) {
        for (x, y) in p.iter().zip(q) {
            let d = (x - y).abs();
            sq += f64::from(d * d);
            sum_abs += f64::from(d);
            max_abs = max_abs.max(d);
        }
    }
    let samples = (ref_px.len() * 3) as f64;
    let mse = sq / samples;
    let psnr = if mse <= 0.0 {
        99.0
    } else {
        10.0 * (PEAK * PEAK / mse).log10()
    };

    let luma = |v: &[[f32; 3]]| -> Vec<f64> {
        v.iter()
            .map(|p| f64::from(0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]))
            .collect()
    };
    let ssim = mean_ssim(
        &luma(&ref_px),
        &luma(&cand_px),
        width as usize,
        height as usize,
    );
    println!(
        "psnr_db={psnr:.2} ssim={ssim:.4} max_abs={max_abs:.4} mean_abs={:.6}",
        sum_abs / samples
    );

    if let Some(path) = rest.first() {
        let diff = RgbImage::from_fn(width, height, |x, y| {
            let i = (y * width + x) as usize;
            Rgb(std::array::from_fn(|c| {
                let d = (8.0 * (ref_px[i][c] - cand_px[i][c]).abs()).min(1.0);
                (d * 255.0).round() as u8
            }))
        });
        diff.save(path).with_context(|| format!("writing {path}"))?;
    }
    Ok(())
}
