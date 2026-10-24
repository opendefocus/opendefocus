//! Hardware tests of focus reconstruction: subpixel crossings, sharp edges and opaque discs.
use ndarray::{Array2, Array3};

use crate::runners::adaptive::{AdaptiveLayerPixel, CocPolarity};

use super::references::assert_close;
use super::scenes::uniform_layers;
use super::{feature, high, runner};

/// Plan item 1. The scene mixes pure red and pure blue pixels with alpha 1, so
/// every output must satisfy R + B == 1, G == 0 and alpha == 1 whatever the
/// sample pattern (convolution is a normalised weighted mean). At Accurate the
/// exact integral is y-mirror symmetric because the scene does not vary in y.
/// The unfiltered-column assertion of the original test is repeated.
/// Weak spot: this cannot catch a wrong far/near *blend ratio* away from the
/// focus column, only normalisation and symmetry errors.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_subpixel_focus_crossing_conserves_color_and_mirrors() {
    let runner = runner().await;
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = if x == 8 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| match x {
        0..=7 => -16.0,
        8 => 0.0,
        _ => 16.0,
    });
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 16.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let features = [feature(-16.0, 16.0, 2.0)];
    let low = runner
        .gather_adaptive_frame(&source, &prepared, &features, 16.0)
        .await
        .unwrap();
    let exact = runner
        .gather_adaptive_frame_with_budget(&source, &prepared, &features, 16.0, high())
        .await
        .unwrap();
    for (label, output) in [("low", &low), ("high", &exact)] {
        for y in 0..16 {
            for x in 0..16 {
                let [r, g, b, a] = std::array::from_fn(|c| output[[y, x, c]]);
                assert!(
                    (r + b - 1.0).abs() < 1e-5,
                    "{label} ({x},{y}): R+B={}",
                    r + b
                );
                assert!(g.abs() < 1e-6, "{label} ({x},{y}): G={g}");
                assert!((a - 1.0).abs() < 1e-5, "{label} ({x},{y}): alpha={a}");
                assert!(r >= -1e-6 && b >= -1e-6);
            }
        }
        let neighbors = f32::midpoint(output[[8, 7, 0]], output[[8, 9, 0]]);
        assert!(
            (output[[8, 8, 0]] - neighbors).abs() < 0.005,
            "{label}: unfiltered focus column {} vs neighbours {neighbors}",
            output[[8, 8, 0]]
        );
    }
    for y in 0..16 {
        for x in 0..16 {
            for c in 0..4 {
                assert!(
                    (exact[[y, x, c]] - exact[[15 - y, x, c]]).abs() < 1e-5,
                    "high output is not y-mirror symmetric at ({x},{y})"
                );
            }
        }
    }
}

/// Plan item 2. A focused edge with no `CoC` step and a uniform far plate must
/// leave the image untouched: sharp pixels pass through bit-for-bit and the
/// blurred far side (uniform blue) composites to the same blue. This is
/// stronger than the original GPU==CPU loop because it names the exact answer.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_focused_edge_without_coc_step_is_identity() {
    let runner = runner().await;
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = match x {
            0..8 => [0.0, 1.0, 0.0, 1.0],
            8 => [0.0, 0.25, 0.75, 1.0],
            _ => [0.0, 0.0, 1.0, 1.0],
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| if x <= 8 { 0.0 } else { -8.0 });
    let layers = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let output = runner
        .gather_adaptive_frame(&source, &layers, &[feature(0.0, -8.0, 2.0)], 8.0)
        .await
        .unwrap();
    assert_close(&output, &source, 1e-4, "focused edge");
    // The focused x <= 8 columns are exact pass-through, not merely close.
    for y in 0..16 {
        for x in 0..=8 {
            for c in 0..4 {
                assert_eq!(output[[y, x, c]], source[[y, x, c]], "sharp ({x},{y},{c})");
            }
        }
    }
}

/// Plan item 8 and the thin-foreground-feature port. All assertions are on the
/// GPU output (the original asserted the stripe positives on the CPU output).
/// - Near disc over a sharp far plate (Fast): inside the disc red + blue == 1
///   (the near layer composites over the far plate), red falls and blue rises
///   monotonically from the disc centre outward, and the edge pixel keeps both
///   near red and far blue (> 0.1).
/// - 1 px near stripe (`CoC` 1.5, 105 px cap): the stripe survives the blur
///   (red > 0.15, green > 0.1, blue > 0.1 at the stripe), and pixels three or
///   more columns away stay pure blue (1e-3). At Accurate the stripe profile is
///   exactly mirror symmetric (1e-5).
/// The plan's "row sum of red is 1 +/- 3%" is false: the stripe's
/// smoothstep(1,2,coc) occupancy makes it 0.91 at Accurate and 1.23 at Fast.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_near_edge_and_thin_stripe_keep_sharp_far_background() {
    let runner = runner().await;
    let source = Array3::from_shape_fn((16, 16, 4), |(_, _, c)| [0.0, 0.0, 0.0, 1.0][c]);
    let prepared = uniform_layers((16, 16), |x, y| {
        let near = (x as f32 - 4.0).powi(2) + (y as f32 - 8.0).powi(2) <= 64.0;
        AdaptiveLayerPixel {
            near_rgba: if near { [1.0, 0.0, 0.0, 1.0] } else { [0.0; 4] },
            far_rgba: [0.0, 0.0, 1.0, 1.0],
            near_coc: if near { 8.0 } else { 0.0 },
            far_coc: 0.0,
            near_valid: if near { 1.0 } else { 0.0 },
            far_valid: 1.0,
        }
    });
    let output = runner
        .gather_adaptive_frame(&source, &prepared, &[feature(8.0, 8.0, 2.0)], 8.0)
        .await
        .unwrap();
    assert!(output[[8, 11, 0]] > 0.1, "near red: {}", output[[8, 11, 0]]);
    assert!(output[[8, 11, 2]] > 0.1, "far blue: {}", output[[8, 11, 2]]);
    for x in 0..=12 {
        assert!(
            (output[[8, x, 0]] + output[[8, x, 2]] - 1.0).abs() < 1e-4,
            "red + blue != 1 inside the near disc at x={x}"
        );
    }
    for x in 4..12 {
        assert!(
            output[[8, x + 1, 0]] < output[[8, x, 0]],
            "red not falling at x={x}"
        );
        assert!(
            output[[8, x + 1, 2]] > output[[8, x, 2]],
            "blue not rising at x={x}"
        );
    }
    assert!(output.iter().all(|v| (-1e-6..=1.0 + 1e-6).contains(v)));

    let stripe_source = Array3::from_shape_fn((16, 16, 4), |(_, x, c)| {
        if x == 8 {
            [1.0, 0.8, 0.0, 1.0][c]
        } else {
            [0.0, 0.0, 1.0, 1.0][c]
        }
    });
    let stripe = uniform_layers((16, 16), |x, _| {
        let near = x == 8;
        AdaptiveLayerPixel {
            near_rgba: if near { [1.0, 0.8, 0.0, 1.0] } else { [0.0; 4] },
            far_rgba: [0.0, 0.0, 1.0, 1.0],
            near_coc: if near { 1.5 } else { 0.0 },
            far_coc: 0.0,
            near_valid: if near { 1.0 } else { 0.0 },
            far_valid: 1.0,
        }
    });
    let features = [feature(1.5, 1.5, 2.0)];
    let low = runner
        .gather_adaptive_frame(&stripe_source, &stripe, &features, 105.0)
        .await
        .unwrap();
    assert!(
        low[[8, 8, 0]] > 0.15 && low[[8, 8, 1]] > 0.1 && low[[8, 8, 2]] > 0.1,
        "{:?}",
        low.slice(ndarray::s![8, 8, ..])
    );
    let exact = runner
        .gather_adaptive_frame_with_budget(&stripe_source, &stripe, &features, 105.0, high())
        .await
        .unwrap();
    assert!(exact[[8, 8, 0]] > 0.15 && exact[[8, 8, 1]] > 0.1 && exact[[8, 8, 2]] > 0.1);
    for (label, output) in [("low", &low), ("high", &exact)] {
        for y in 0..16 {
            for x in (0..16).filter(|x| (*x as i32 - 8).abs() >= 3) {
                assert!(
                    output[[y, x, 0]].abs() < 1e-3
                        && output[[y, x, 1]].abs() < 1e-3
                        && (output[[y, x, 2]] - 1.0).abs() < 1e-3,
                    "{label}: far background disturbed at ({x},{y})"
                );
            }
        }
    }
    for x in 1..8 {
        for c in 0..3 {
            assert!(
                (exact[[8, 8 - x, c]] - exact[[8, 8 + x, c]]).abs() < 1e-5,
                "high stripe profile is not symmetric at offset {x}, channel {c}"
            );
        }
    }
    assert!(exact[[8, 8, 0]] > exact[[8, 9, 0]] && exact[[8, 9, 0]] > exact[[8, 10, 0]]);
}

/// Port of the opaque-near stage tests. An opaque blue near disc (`CoC` >= 2 so
/// it is fully occupied) in front of a red source and red far plate must hide
/// both: pixels well inside the disc are pure blue and pixels well outside are
/// pure red (the near layer must not leak into the sharp surroundings).
/// Covers a constant `CoC` and a `CoC` that varies per column. Alpha stays 1.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
#[allow(
    clippy::many_single_char_names,
    reason = "grid coordinates are conventionally single-letter in this pixel test"
)]
async fn hardware_opaque_near_disc_hides_source_and_far_plate() {
    let runner = runner().await;
    let (w, h) = (40usize, 40usize);
    let source = Array3::from_shape_fn((h, w, 4), |(_, _, c)| [1.0, 0.0, 0.0, 1.0][c]);
    for varying in [false, true] {
        let prepared = uniform_layers((w, h), |x, y| {
            let near = (x as i32 - 20).pow(2) + (y as i32 - 20).pow(2) <= 64;
            AdaptiveLayerPixel {
                near_rgba: if near { [0.0, 0.0, 1.0, 1.0] } else { [0.0; 4] },
                far_rgba: [1.0, 0.0, 0.0, 1.0],
                near_coc: match (near, varying) {
                    (false, _) => 0.0,
                    (true, false) => 3.0,
                    (true, true) => 2.0 + (x % 8) as f32 * 0.5,
                },
                far_coc: -0.1,
                near_valid: if near { 1.0 } else { 0.0 },
                far_valid: 1.0,
            }
        });
        let reach = if varying { 5.5 } else { 3.0 };
        let output = runner
            .gather_adaptive_frame_with_budget(
                &source,
                &prepared,
                &[feature(0.0, 5.5, 2.0); 9],
                8.0,
                high(),
            )
            .await
            .unwrap();
        for y in 0..h {
            for x in 0..w {
                let distance = (x as f32 - 20.0).hypot(y as f32 - 20.0);
                let [r, g, b, a] = std::array::from_fn(|c| output[[y, x, c]]);
                assert!((a - 1.0).abs() < 1e-5);
                if distance <= 7.0 - reach {
                    assert!(
                        r.abs() < 1e-4 && g.abs() < 1e-4 && (b - 1.0).abs() < 1e-4,
                        "varying={varying}: source leaks through the near disc at ({x},{y}): {r} {g} {b}"
                    );
                }
                if distance >= 8.0 + reach + 1.0 {
                    assert!(
                        (r - 1.0).abs() < 1e-3 && g.abs() < 1e-3 && b.abs() < 1e-3,
                        "varying={varying}: near disc leaks outside at ({x},{y}): {r} {g} {b}"
                    );
                }
            }
        }
    }
}

/// An opaque near surface whose `CoC` stays below the front regime (2 px) is
/// the visible surface: it occludes the far plate, so a uniform red near layer
/// over a green far plate must stay red at every small `CoC`, in the exact and
/// the sampled gather. The earlier centre-blur gate blended in the far plate
/// from 0.25 px and drew a line where a receding floor crosses the focal plane.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_small_coc_near_surface_hides_the_far_plate() {
    let runner = runner().await;
    let source = Array3::from_shape_fn((9, 9, 4), |(_, _, c)| [1.0, 0.0, 0.0, 1.0][c]);
    for coc in [0.2f32, 0.3, 0.4, 0.5, 0.75, 1.0] {
        let prepared = uniform_layers((9, 9), |_, _| AdaptiveLayerPixel {
            near_rgba: [1.0, 0.0, 0.0, 1.0],
            far_rgba: [0.0, 1.0, 0.0, 1.0],
            near_coc: coc,
            far_coc: 0.0,
            near_valid: 1.0,
            far_valid: 1.0,
        });
        let features = [feature(0.0, coc, 2.0)];
        let exact = runner
            .gather_adaptive_frame_with_budget(&source, &prepared, &features, 4.0, high())
            .await
            .unwrap();
        let sampled = runner
            .gather_adaptive_frame(&source, &prepared, &features, 4.0)
            .await
            .unwrap();
        assert_close(&exact, &source, 1e-4, &format!("exact at coc {coc}"));
        assert_close(&sampled, &source, 1e-3, &format!("sampled at coc {coc}"));
    }
}

/// Row of the receding-surface scenes that lies on the focal plane.
const FOCUS_ROW: usize = 24;
/// `CoC` change per row of the receding surface, as on the bundled floor.
const RAMP_SLOPE: f32 = 0.45;

/// Renders a receding surface whose `CoC` is `RAMP_SLOPE` per row, crossing the
/// focal plane just above `FOCUS_ROW`, in the exact and the sampled gather.
async fn render_focus_ramp(color: impl Fn(usize) -> f32) -> (Array3<f32>, [Array3<f32>; 2]) {
    let runner = runner().await;
    let (width, height) = (24usize, 2 * FOCUS_ROW);
    let source = Array3::from_shape_fn(
        (height, width, 4),
        |(y, _, c)| {
            if c == 3 { 1.0 } else { color(y) }
        },
    );
    let coc = Array2::from_shape_fn((height, width), |(y, _)| {
        RAMP_SLOPE * (y as f32 - FOCUS_ROW as f32) + 0.15
    });
    let max_radius = coc
        .iter()
        .fold(0.0_f32, |max, value| max.max(value.abs()))
        .ceil();
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, max_radius, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let features =
        vec![feature(-max_radius, max_radius, 2.0); width.div_ceil(16) * height.div_ceil(16)];
    let exact = runner
        .gather_adaptive_frame_with_budget(&source, &prepared, &features, max_radius, high())
        .await
        .unwrap();
    let sampled = runner
        .gather_adaptive_frame(&source, &prepared, &features, max_radius)
        .await
        .unwrap();
    (source, [exact, sampled])
}

/// A receding surface with a colour edge on its focal plane (issue #49). The
/// near rows next to focus have discs smaller than their distance to the far
/// rows, and the far rows' discs never reach the near side, so the edge stays
/// sharp and no light far colour may leak into the dark near rows. Before the
/// fix the first near row took the far continuation and showed as a line.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_focus_ramp_edge_has_no_line() {
    let (light, dark) = (0.6, 0.15);
    let (_, outputs) = render_focus_ramp(|y| if y < FOCUS_ROW { light } else { dark }).await;
    for (label, output) in ["exact", "sampled"].iter().zip(&outputs) {
        for x in 4..20 {
            assert!(
                (output[[FOCUS_ROW - 1, x, 0]] - light).abs() < 5e-3,
                "{label}: far focus row ({x}) = {}",
                output[[FOCUS_ROW - 1, x, 0]]
            );
            for y in FOCUS_ROW..FOCUS_ROW + 4 {
                assert!(
                    (output[[y, x, 0]] - dark).abs() < 5e-3,
                    "{label}: far colour leaks into near row {y} ({x}): {}",
                    output[[y, x, 0]]
                );
            }
        }
    }
}

/// A one-pixel line on the focal plane of a receding surface stays sharp and
/// does not spread: its own disc is smaller than a pixel and the focus band is
/// several rows wide, so focus reconstruction must leave it alone instead of
/// interpolating it away or drawing streaks across neighbouring rows. The
/// antialiased disc of the next row overlaps it slightly (about 6% of 0.3), so
/// the line keeps 90% while reconstruction would pull it to the surface 0.3.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_focus_ramp_keeps_one_pixel_focal_line() {
    let (surface, line) = (0.3, 1.0);
    let (_, outputs) = render_focus_ramp(|y| if y == FOCUS_ROW { line } else { surface }).await;
    // The sampled gather reads filtered layers, so it may spread slightly.
    for ((label, spread), output) in [("exact", 1e-2), ("sampled", 3e-2)].iter().zip(&outputs) {
        for x in 4..20 {
            assert!(
                output[[FOCUS_ROW, x, 0]] > 0.9 * line,
                "{label}: focal line softened at {x}: {}",
                output[[FOCUS_ROW, x, 0]]
            );
            for y in (FOCUS_ROW - 8..FOCUS_ROW + 8).filter(|&y| y != FOCUS_ROW) {
                assert!(
                    (output[[y, x, 0]] - surface).abs() < *spread,
                    "{label}: focal line spreads to row {y} ({x}): {}",
                    output[[y, x, 0]]
                );
            }
        }
    }
}
