//! Hardware tests of the adaptive gather and pyramid against brute-force references.
use bytemuck::Zeroable;
use ndarray::{Array2, Array3};

use crate::AdaptiveSampleBudget;
use crate::runners::adaptive::{AdaptiveLayerPixel, CocPolarity};

use super::TileFeatures;
use super::references::{assert_close, far_reference, max_abs_diff, reduce_reference};
use super::scenes::{far_pixel, uniform_layers};
use super::{feature, high, runner};

/// Plan item 3, constant part. Normalisation must reproduce a constant HDR
/// frame at every pixel for tile classes 0/1/2, for a sub-pixel (1 px) blur
/// with a 105 px cap, and for odd frame sizes (the original only checked one
/// pixel each). The tolerance is relative 1e-5; the measured error is 0.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_constant_hdr_survives_every_class_small_blur_and_odd_frames() {
    let runner = runner().await;
    let color = [128.0, 0.5, 0.25, 1.0];
    let constant = |w: usize, h: usize| Array3::from_shape_fn((h, w, 4), |(_, _, c)| color[c]);
    let source = constant(16, 16);
    for class in [0.0, 1.0, 2.0] {
        let prepared = uniform_layers((16, 16), |_, _| far_pixel(color, 4.0));
        let output = runner
            .gather_adaptive_frame(&source, &prepared, &[feature(0.0, 0.0, class)], 8.0)
            .await
            .unwrap();
        assert_close(&output, &source, 1e-5, &format!("class {class}"));
    }
    let small = uniform_layers((16, 16), |_, _| far_pixel(color, -1.0));
    let output = runner
        .gather_adaptive_frame(&source, &small, &[feature(0.0, 0.0, 2.0)], 105.0)
        .await
        .unwrap();
    assert_close(&output, &source, 1e-5, "1 px blur, 105 px cap");
    for (w, h) in [(13usize, 11usize), (7, 5), (1, 9), (9, 1)] {
        let source = constant(w, h);
        let prepared = uniform_layers((w, h), |_, _| far_pixel(color, 4.0));
        let tiles = w.div_ceil(16) * h.div_ceil(16);
        for budget in [
            high(),
            AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Fast),
        ] {
            let output = runner
                .gather_adaptive_frame_with_budget(
                    &source,
                    &prepared,
                    &vec![feature(0.0, 0.0, 2.0); tiles],
                    8.0,
                    budget,
                )
                .await
                .unwrap();
            assert_close(&output, &source, 1e-5, &format!("odd frame {w}x{h}"));
        }
    }
}

/// Plan item 3, gradient parts. A far plate with a constant 8 px `CoC` must equal
/// the brute-force normalised disc mean everywhere, including the frame edges
/// where the in-frame renormalisation (and therefore the radius) is visible.
/// Accurate is compared at 5e-5 (measured 2e-6). The sampled Fast path is compared
/// against the same reference with a sampling-noise bound: 0.4 on the 16x16
/// gradient (values 0..15, measured 0.30) and 0.25 on the 40x40 field
/// (measured 0.16). Those bounds are loose; they catch a wrong radius or
/// centre offset (several units at the frame edges) but not sub-0.1 errors.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_gather_matches_brute_force_disc_mean_on_gradients() {
    let runner = runner().await;
    let gradient = |x: usize, y: usize| [x as f32, y as f32, (x + y) as f32 * 0.25, 1.0];
    let wavy = |x: usize, y: usize| [x as f32 * 0.5, y as f32 * 0.25, (x * y) as f32 * 0.01, 1.0];
    for (size, color, coc, low_bound) in [
        (
            (16usize, 16usize),
            &gradient as &dyn Fn(usize, usize) -> [f32; 4],
            8.0f32,
            0.4f32,
        ),
        ((40, 40), &wavy, -8.0, 0.25),
    ] {
        let source = Array3::from_shape_fn((size.1, size.0, 4), |(y, x, c)| color(x, y)[c]);
        let prepared = uniform_layers(size, |x, y| far_pixel(color(x, y), coc));
        let expected = far_reference(size, color, |_, _| coc.abs());
        let features = vec![feature(-8.0, 8.0, 2.0); size.0.div_ceil(16) * size.1.div_ceil(16)];
        let exact = runner
            .gather_adaptive_frame_with_budget(&source, &prepared, &features, 8.0, high())
            .await
            .unwrap();
        assert_close(&exact, &expected, 5e-5, "high gradient");
        let low = runner
            .gather_adaptive_frame(&source, &prepared, &features, 8.0)
            .await
            .unwrap();
        assert!(
            max_abs_diff(&low, &expected) < low_bound,
            "low gradient deviates from the disc mean by {}",
            max_abs_diff(&low, &expected)
        );
    }
}

/// Plan item 3, pyramid part. The GPU mip chain must equal the box-mean
/// reference in [`reduce_reference`] (which copies the odd-edge rule of
/// `adaptive_pyramid.wgsl` and the removed stage test: the last row/column
/// absorbs an odd source edge, colours are cell means, `CoC` is the
/// valid-weighted mean, valid is the mean). Level 0 must be the input. The
/// mixed validity/CoC scene keeps the near and far layers separate.
/// Tolerance 1e-5 relative (measured 2e-6).
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_pyramid_matches_box_mean_reference() {
    let runner = runner().await;
    for (w, h) in [
        (16, 16),
        (2, 1),
        (6, 4),
        (8, 8),
        (14, 14),
        (11, 13),
        (3, 3),
        (1, 5),
    ] {
        let prepared = uniform_layers((w, h), |x, y| AdaptiveLayerPixel {
            near_rgba: [
                x as f32,
                0.5 * y as f32,
                1.0,
                if (x + y) % 3 == 0 { 0.0 } else { 1.0 },
            ],
            far_rgba: [(x * y) as f32, 2.0, x as f32 - y as f32, 1.0],
            near_coc: if (x + y) % 3 == 0 {
                0.0
            } else {
                2.0 + x as f32
            },
            far_coc: -3.0 - y as f32,
            near_valid: if (x + y) % 3 == 0 { 0.0 } else { 1.0 },
            far_valid: if x % 4 == 1 { 0.0 } else { 1.0 },
        });
        let pyramid = runner.build_adaptive_pyramid(&prepared).await.unwrap();
        let (mut sw, mut sh) = (w, h);
        let mut expected = prepared.pixels.clone();
        assert_eq!((pyramid.levels[0].width, pyramid.levels[0].height), (w, h));
        let mut level = 0;
        loop {
            let actual: &[f32] = bytemuck::cast_slice(&pyramid.levels[level].pixels);
            let reference: &[f32] = bytemuck::cast_slice(&expected);
            assert_eq!(actual.len(), reference.len(), "{w}x{h} level {level}");
            for (index, (a, e)) in actual.iter().zip(reference).enumerate() {
                assert!(
                    (a - e).abs() <= 1e-5 * (1.0 + e.abs()),
                    "{w}x{h} level {level} value {index}: {a} != {e}"
                );
            }
            if sw == 1 && sh == 1 {
                break;
            }
            let (tw, th) = ((sw / 2).max(1), (sh / 2).max(1));
            expected = reduce_reference(&expected, (sw, sh), (tw, th));
            (sw, sh) = (tw, th);
            level += 1;
            assert_eq!(
                (pyramid.levels[level].width, pyramid.levels[level].height),
                (sw, sh)
            );
        }
        assert_eq!(pyramid.levels.len(), level + 1, "{w}x{h} level count");
    }
}

/// Plan item 7. A lone valid far texel reproduces its colour exactly at its
/// own pixel and leaves the black source everywhere else, including when the
/// texel sits in a frame corner. Same GPU assertions as the original test,
/// without the CPU half. Tolerance 1e-5 absolute.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_high_single_far_texel_is_exact_without_cpu_reference() {
    let runner = runner().await;
    let (w, h) = (13usize, 9usize);
    let source = Array3::zeros((h, w, 4));
    let features = [feature(-5.5, 6.0, 11.5)];
    let expected_color = [0.75, 0.4, 0.15, 1.0];
    for (source_x, source_y) in [(6usize, 4usize), (0, 0), (w - 1, h - 1)] {
        let prepared = uniform_layers((w, h), |x, y| {
            if (x, y) == (source_x, source_y) {
                far_pixel(expected_color, -5.5)
            } else {
                AdaptiveLayerPixel::zeroed()
            }
        });
        let gpu = runner
            .gather_adaptive_frame_with_lens(&source, &prepared, &features, 6.0, high(), None)
            .await
            .unwrap();
        assert_eq!(gpu.dim(), (h, w, 4));
        for (y, x, channel) in (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| (0..4).map(move |channel| (y, x, channel))))
        {
            let expected = if (x, y) == (source_x, source_y) {
                expected_color[channel]
            } else {
                0.0
            };
            assert!(
                (gpu[[y, x, channel]] - expected).abs() < 1e-5,
                "texel ({source_x},{source_y}), output ({x},{y}) channel {channel}: {} != {expected}",
                gpu[[y, x, channel]]
            );
        }
    }
}

/// Port of the tile-class and reach invariance stage tests. At Accurate the exact
/// integral must not depend on the classifier's tile class (0 sharp, 1 smooth,
/// 2 detail), on the tile `CoC` range supplied in the features, or on the
/// `max_radius` cap as long as it is not below the largest `CoC`: the reach is
/// derived from the prepared layers. The scene mixes a near disc, a far
/// region and a focused region over a textured HDR source, and must differ
/// visibly from the input so the equality is not vacuous.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_high_output_is_tile_class_and_reach_invariant() {
    let runner = runner().await;
    let (w, h) = (48usize, 48usize);
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, c)| {
        [
            0.2 + 0.01 * x as f32 + if (x / 6 + y / 6) % 2 == 0 { 0.5 } else { 0.0 },
            0.3 + 0.01 * y as f32,
            0.4 + if x > 30 { 3.0 } else { 0.0 },
            1.0,
        ][c]
    });
    let coc = Array2::from_shape_fn((h, w), |(y, x)| {
        if (x as i32 - 14).pow(2) + (y as i32 - 16).pow(2) < 49 {
            5.0
        } else if x > 28 {
            -(2.0 + y as f32 * 0.1)
        } else {
            0.0
        }
    });
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let features = runner
        .classify_tiles(&source, &coc, [0, 0], 8.0)
        .await
        .unwrap();
    let render = |features: Vec<TileFeatures>, cap: f32| {
        let (runner, source, prepared) = (&runner, &source, &prepared);
        async move {
            runner
                .gather_adaptive_frame_with_budget(source, prepared, &features, cap, high())
                .await
                .unwrap()
        }
    };
    let base = render(features.clone(), 8.0).await;
    assert!(max_abs_diff(&base, &source) > 0.5, "scene is not blurred");
    for class in [0.0, 1.0, 2.0] {
        let classed: Vec<_> = features
            .iter()
            .map(|f| {
                let mut f = *f;
                f.values[9] = class;
                f
            })
            .collect();
        assert!(
            max_abs_diff(&render(classed, 8.0).await, &base) < 1e-6,
            "tile class {class}"
        );
    }
    let wide: Vec<_> = features
        .iter()
        .map(|f| {
            let mut f = *f;
            f.values[0] *= 4.0;
            f.values[1] *= 4.0;
            f
        })
        .collect();
    assert!(
        max_abs_diff(&render(wide, 8.0).await, &base) < 1e-6,
        "4x feature reach"
    );
    for cap in [10.0, 40.0] {
        assert!(
            max_abs_diff(&render(features.clone(), cap).await, &base) < 1e-6,
            "max radius cap {cap}"
        );
    }
}
