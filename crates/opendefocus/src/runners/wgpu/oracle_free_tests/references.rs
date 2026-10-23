//! Analytic CPU reference helpers (disc coverage, pyramid and assertions) for the oracle-free tests.
use bytemuck::Zeroable;
use ndarray::Array3;

use crate::runners::adaptive::AdaptiveLayerPixel;

use super::scenes::Case;

/// Returns the anti-aliased coverage in [0, 1] of a disc of `radius` at offset (`dx`, `dy`) from its centre.
pub(super) fn disc_coverage(radius: f32, dx: f32, dy: f32) -> f32 {
    (radius.abs().max(0.5) - dx.hypot(dy) + 0.5).clamp(0.0, 1.0)
}

/// Returns the disc coverage normalised by the disc area, with the area floored at 0.25.
fn disc_weight(radius: f32, dx: f32, dy: f32) -> f32 {
    disc_coverage(radius, dx, dy) / (radius * radius).max(0.25)
}

/// Far layer only, every pixel valid: normalised weighted mean of the far colours.
pub(super) fn far_reference(
    (w, h): (usize, usize),
    color: impl Fn(usize, usize) -> [f32; 4],
    radius: impl Fn(usize, usize) -> f32,
) -> Array3<f32> {
    Array3::from_shape_fn((h, w, 4), |(y, x, channel)| {
        let (mut sum, mut total) = (0.0f64, 0.0f64);
        for sy in 0..h {
            for sx in 0..w {
                let weight =
                    disc_weight(radius(sx, sy), sx as f32 - x as f32, sy as f32 - y as f32) as f64;
                sum += weight * color(sx, sy)[channel] as f64;
                total += weight;
            }
        }
        (sum / total) as f32
    })
}

/// Adds `weight` times each channel of `sample` to the running premultiplied sums in `premul`.
fn add_weighted_sample(premul: &mut [f64; 4], weight: f64, sample: [f32; 4]) {
    for (acc, value) in premul.iter_mut().zip(sample) {
        *acc += weight * value as f64;
    }
}

/// Near layer with `CoC` `radius` over the pixels where `near` is true. The
/// result is the front layer composited over the sharp source (outside the
/// near footprint) or over nothing (inside it), then re-normalised to opaque
/// because the source is opaque. `front.valid / potential.valid` is the near
/// coverage; the potential term is the same disc over every in-frame pixel.
pub(super) fn near_reference(
    (w, h): (usize, usize),
    source: &Array3<f32>,
    color: impl Fn(usize, usize) -> [f32; 4],
    near: impl Fn(usize, usize) -> bool,
    radius: f32,
) -> Array3<f32> {
    let mut out = Array3::zeros((h, w, 4));
    for y in 0..h {
        for x in 0..w {
            let (mut premul, mut occupied, mut potential) = ([0.0f64; 4], 0.0f64, 0.0f64);
            for sy in 0..h {
                for sx in 0..w {
                    let weight =
                        disc_weight(radius, sx as f32 - x as f32, sy as f32 - y as f32) as f64;
                    potential += weight;
                    if near(sx, sy) {
                        occupied += weight;
                        let sample = color(sx, sy);
                        add_weighted_sample(&mut premul, weight, sample);
                    }
                }
            }
            let blend = {
                let t = ((occupied / potential - 0.99) / 0.01).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let denominator = potential * (1.0 - blend) + occupied * blend;
            let mut front: [f64; 4] = std::array::from_fn(|c| premul[c] / denominator.max(1e-6));
            if front[3] > 1.0 {
                front = front.map(|v| v / front[3]);
            }
            front[3] = front[3].clamp(0.0, 1.0);
            // Inside the near footprint the far plate is empty and the centre
            // blur gate is 1, so nothing lies behind the front layer.
            let background: [f64; 4] = if near(x, y) {
                [0.0; 4]
            } else {
                std::array::from_fn(|c| source[[y, x, c]] as f64)
            };
            let mut result: [f64; 4] =
                std::array::from_fn(|c| front[c] + background[c] * (1.0 - front[3]));
            if source[[y, x, 3]] >= 1.0 - 1e-6 && result[3] > 1e-6 {
                result = result.map(|v| v / result[3]);
            }
            for c in 0..4 {
                out[[y, x, c]] = result[c] as f32;
            }
        }
    }
    out
}

/// Near layer at `CoC` 1: entirely handled by the 5x5 local gather, whose
/// opaque result is the normalised mean of the near colours under radius 1.
pub(super) fn local_reference(
    (w, h): (usize, usize),
    color: impl Fn(usize, usize) -> [f32; 4],
) -> Array3<f32> {
    far_reference((w, h), color, |_, _| 1.0)
}

/// Checks the output at each probe pixel of `case` against its expected value, failing with `group` in the message.
pub(super) fn assert_probes(
    group: &str,
    case: &Case,
    output: &Array3<f32>,
    pixels: &[(usize, usize)],
    frozen: &[(&str, &[[f32; 4]])],
    tolerance: f32,
) {
    let (_, expected) = frozen
        .iter()
        .find(|(name, _)| *name == case.name)
        .unwrap_or_else(|| panic!("{group}: no frozen probes for {}", case.name));
    assert_eq!(pixels.len(), expected.len());
    for (&(x, y), expected) in pixels.iter().zip(expected.iter()) {
        for (channel, expected) in expected.iter().enumerate() {
            let actual = output[[y, x, channel]];
            assert!(
                (actual - expected).abs() <= tolerance,
                "{group} {} probe ({x}, {y}) channel {channel}: {actual} != frozen {expected}",
                case.name
            );
        }
    }
}

/// Returns the largest absolute per-channel difference between two equally sized frames.
pub(super) fn max_abs_diff(a: &Array3<f32>, b: &Array3<f32>) -> f32 {
    assert_eq!(a.dim(), b.dim());
    a.iter()
        .zip(b.iter())
        .fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
}

/// First and second moments of one channel of an image: `(mass, cx, cy, sxx, syy)`.
pub(super) fn moments(image: &Array3<f32>, channel: usize) -> (f64, f64, f64, f64, f64) {
    let (h, w, _) = image.dim();
    let (mut m, mut mx, mut my) = (0.0f64, 0.0f64, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            let v = image[[y, x, channel]] as f64;
            m += v;
            mx += v * x as f64;
            my += v * y as f64;
        }
    }
    let (cx, cy) = (mx / m, my / m);
    let (mut sxx, mut syy) = (0.0f64, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            let v = image[[y, x, channel]] as f64;
            sxx += v * (x as f64 - cx).powi(2);
            syy += v * (y as f64 - cy).powi(2);
        }
    }
    (m, cx, cy, sxx / m, syy / m)
}

/// One pyramid reduction step in f64 (see `adaptive_pyramid.wgsl`): colours are
/// the mean over the cell, `CoC`s the valid-weighted mean, valid the mean; the
/// last row and column absorb an odd source edge.
pub(super) fn reduce_reference(
    source: &[AdaptiveLayerPixel],
    (sw, sh): (usize, usize),
    (tw, th): (usize, usize),
) -> Vec<AdaptiveLayerPixel> {
    let mut out = Vec::with_capacity(tw * th);
    for ty in 0..th {
        for tx in 0..tw {
            let x1 = if tx + 1 == tw {
                sw
            } else {
                (tx * 2 + 2).min(sw)
            };
            let y1 = if ty + 1 == th {
                sh
            } else {
                (ty * 2 + 2).min(sh)
            };
            let mut color = [0.0f64; 8];
            let (mut near_coc, mut far_coc, mut near_valid, mut far_valid, mut count) =
                (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for y in ty * 2..y1 {
                for x in tx * 2..x1 {
                    let p = source[y * sw + x];
                    let nv = p.near_valid.clamp(0.0, 1.0) as f64;
                    let fv = p.far_valid.clamp(0.0, 1.0) as f64;
                    for c in 0..4 {
                        color[c] += p.near_rgba[c] as f64;
                        color[4 + c] += p.far_rgba[c] as f64;
                    }
                    near_coc += p.near_coc as f64 * nv;
                    far_coc += p.far_coc as f64 * fv;
                    near_valid += nv;
                    far_valid += fv;
                    count += 1.0;
                }
            }
            let color = color.map(|v| (v / count) as f32);
            out.push(AdaptiveLayerPixel {
                near_rgba: [color[0], color[1], color[2], color[3]],
                far_rgba: [color[4], color[5], color[6], color[7]],
                near_coc: if near_valid > 0.0 {
                    (near_coc / near_valid) as f32
                } else {
                    0.0
                },
                far_coc: if far_valid > 0.0 {
                    (far_coc / far_valid) as f32
                } else {
                    0.0
                },
                near_valid: (near_valid / count) as f32,
                far_valid: (far_valid / count) as f32,
            });
        }
    }
    out
}

/// Asserts that two frames match within `tolerance` in every channel.
pub(super) fn assert_close(
    actual: &Array3<f32>,
    expected: &Array3<f32>,
    tolerance: f32,
    what: &str,
) {
    assert_eq!(actual.dim(), expected.dim(), "{what}: dimensions");
    let (_, w, channels) = actual.dim();
    for (index, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            a.is_finite() && (a - e).abs() <= tolerance * (1.0 + e.abs()),
            "{what}: pixel ({}, {}) channel {}: {a} != expected {e}",
            (index / channels) % w,
            (index / channels) / w,
            index % channels,
        );
    }
}

#[test]
/// Verifies the disc reference helpers agree with each other.
fn disc_reference_helpers_are_self_consistent() {
    // Coverage ramps over one pixel around the radius and never drops below
    // half a pixel of support.
    assert_eq!(disc_coverage(5.0, 0.0, 0.0), 1.0);
    assert_eq!(disc_coverage(5.0, 5.5, 0.0), 0.0);
    assert!((disc_coverage(5.0, 5.0, 0.0) - 0.5).abs() < 1e-7);
    assert!((disc_coverage(0.1, 0.5, 0.0) - 0.5).abs() < 1e-7);
    // A constant frame stays constant under the far reference.
    let flat = far_reference((9, 7), |_, _| [2.0, 3.0, 4.0, 1.0], |_, _| 3.5);
    assert!(
        flat.iter()
            .enumerate()
            .all(|(i, v)| (v - [2.0, 3.0, 4.0, 1.0][i % 4]).abs() < 1e-6)
    );
}

#[test]
/// Verifies the pyramid reference folds an odd edge into the final cell.
fn pyramid_reference_follows_the_odd_edge_rule() {
    // Same numbers as the removed CPU stage test: a 3x3 reduces to one cell
    // containing every source pixel (odd edges fold into the last cell).
    let mut pixels = vec![
        AdaptiveLayerPixel {
            far_rgba: [0.0, 0.0, 0.4, 1.0],
            far_coc: 2.0,
            far_valid: 1.0,
            ..AdaptiveLayerPixel::zeroed()
        };
        9
    ];
    pixels[0] = AdaptiveLayerPixel {
        near_rgba: [4.0, 0.0, 0.0, 1.0],
        near_coc: -4.0,
        near_valid: 1.0,
        ..AdaptiveLayerPixel::zeroed()
    };
    let reduced = reduce_reference(&pixels, (3, 3), (1, 1));
    let mixed = reduced[0];
    assert!((mixed.near_rgba[0] - 4.0 / 9.0).abs() < 1e-6);
    assert_eq!(mixed.near_coc, -4.0);
    assert!((mixed.near_valid - 1.0 / 9.0).abs() < 1e-6);
    assert!((mixed.far_rgba[2] - 0.4 * 8.0 / 9.0).abs() < 1e-6);
    assert_eq!(mixed.far_coc, 2.0);
    assert!((mixed.far_valid - 8.0 / 9.0).abs() < 1e-6);
    // 5 -> 2 columns: the second cell spans three source columns.
    let row = (0..5)
        .map(|x| AdaptiveLayerPixel {
            far_rgba: [x as f32; 4],
            far_valid: 1.0,
            ..AdaptiveLayerPixel::zeroed()
        })
        .collect::<Vec<_>>();
    let reduced = reduce_reference(&row, (5, 1), (2, 1));
    assert_eq!(reduced[0].far_rgba[0], 0.5);
    assert_eq!(reduced[1].far_rgba[0], 3.0);
}
