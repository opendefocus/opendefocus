//! Hardware tests for the adaptive gather: fused circles, constant HDR, opaque near layers and ROI output.
use ndarray::Array3;

use crate::runners::adaptive::{AdaptiveLayerPixel, CocPolarity, PreparedLayers};

use super::{TileFeatures, WgpuRunner};

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that the fused capped-circle path matches the scalar path for fractional and partial near data.
async fn hardware_fused_capped_circle_matches_scalar_for_fractional_and_partial_near_data() {
    use crate::datamodel::{IVector4, Settings, UVector2, render::RenderSpecs};

    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (w, h) = (19usize, 15usize);
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, c)| match c {
        0 => 1.75 + (x + y) as f32 * 0.125,
        1 => 0.375 + x as f32 * 0.01,
        2 => 0.125 + y as f32 * 0.015,
        _ => 0.25 + ((x + y) % 5) as f32 * 0.125,
    });
    let region = IVector4 {
        x: 0,
        y: 0,
        z: w as i32,
        w: h as i32,
    };
    let specs = RenderSpecs {
        full_region: region,
        render_region: region,
    };
    let budget =
        crate::AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate);
    for inverse_foreground in [false, true] {
        let prepared = PreparedLayers {
            width: w,
            height: h,
            near_edge_distance: vec![-1.0; w * h],
            pixels: (0..w * h)
                .map(|index| {
                    let x = index % w;
                    let y = index / w;
                    let near_valid = if x % 11 == 0 {
                        0.0
                    } else if x % 5 == 0 {
                        0.4
                    } else {
                        1.0
                    };
                    AdaptiveLayerPixel {
                        near_rgba: [
                            1.25 + index as f32 * 0.01,
                            0.25 + x as f32 * 0.02,
                            0.125 + y as f32 * 0.01,
                            0.2 + (index % 7) as f32 * 0.1,
                        ],
                        far_rgba: [0.0; 4],
                        near_coc: if x % 3 == 0 { 2.25 } else { 3.5 },
                        far_coc: 0.0,
                        near_valid,
                        far_valid: 0.0,
                    }
                })
                .collect(),
        };
        let mut feature = TileFeatures { values: [0.0; 11] };
        feature.values[0] = 0.0;
        feature.values[1] = 3.5;
        feature.values[9] = 2.0;
        let features = vec![feature; w.div_ceil(16) * h.div_ceil(16)];
        let mut settings = Settings::default();
        settings.render.resolution = UVector2 {
            x: w as u32,
            y: h as u32,
        };
        settings.non_uniform.inverse_foreground = inverse_foreground;
        let lens = crate::datamodel::settings_to_convolve_settings(
            &settings,
            &specs,
            glam::UVec2::splat(256),
            (w * h) as u32,
        );
        let fused = runner
            .gather_adaptive_frame_impl(
                &source,
                &prepared,
                &features,
                3.5,
                budget,
                Some(&lens),
                None,
                true,
            )
            .await
            .unwrap();
        // The scalar gather is the exact reference with identical data,
        // cap, tree, and inverse-foreground phase.
        let scalar = runner
            .gather_adaptive_frame_impl(
                &source,
                &prepared,
                &features,
                3.5,
                budget,
                Some(&lens),
                None,
                false,
            )
            .await
            .unwrap();
        let mut fused_energy = [0.0f64; 4];
        let mut scalar_energy = [0.0f64; 4];
        for (index, (fused_value, scalar_value)) in fused.iter().zip(scalar.iter()).enumerate() {
            assert!(fused_value.is_finite(), "fused element {index}");
            assert!(scalar_value.is_finite(), "scalar element {index}");
            let delta = (fused_value - scalar_value).abs();
            assert!(
                delta <= 2.0e-6 * scalar_value.abs().max(1.0),
                "inverse_foreground={inverse_foreground}, element={index}: {fused_value} != {scalar_value}"
            );
            let channel = index % 4;
            fused_energy[channel] += f64::from(*fused_value);
            scalar_energy[channel] += f64::from(*scalar_value);
        }
        for channel in 0..4 {
            let delta = (fused_energy[channel] - scalar_energy[channel]).abs();
            assert!(
                delta <= 2.0e-6 * scalar_energy[channel].abs().max(1.0),
                "inverse_foreground={inverse_foreground}, channel={channel} energy delta {delta}"
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that constant HDR survives the gather and the budgets hold.
async fn hardware_adaptive_gather_preserves_constant_hdr_and_budgets() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, _, c)| [128.0, 0.5, 0.25, 1.0][c]);
    let pixels = vec![
        AdaptiveLayerPixel {
            near_rgba: [0.0; 4],
            far_rgba: [128.0, 0.5, 0.25, 1.0],
            near_coc: 0.0,
            far_coc: 4.0,
            near_valid: 0.0,
            far_valid: 1.0,
        };
        16 * 16
    ];
    let prepared = PreparedLayers {
        width: 16,
        height: 16,
        near_edge_distance: vec![-1.0; 256],
        pixels,
    };
    for class in [0.0, 1.0, 2.0] {
        let mut feature = TileFeatures { values: [0.0; 11] };
        feature.values[9] = class;
        let output = runner
            .gather_adaptive_frame(&source, &prepared, &[feature], 8.0)
            .await
            .unwrap();
        assert!((output[[8, 8, 0]] - 128.0).abs() < 1e-3);
        assert!((output[[8, 8, 1]] - 0.5).abs() < 1e-5);
        assert!((output[[8, 8, 2]] - 0.25).abs() < 1e-5);
        assert!((output[[8, 8, 3]] - 1.0).abs() < 1e-5);
    }
    let small_blur = PreparedLayers {
        width: 16,
        height: 16,
        near_edge_distance: vec![-1.0; 256],
        pixels: vec![
            AdaptiveLayerPixel {
                near_rgba: [0.0; 4],
                far_rgba: [128.0, 0.5, 0.25, 1.0],
                near_coc: 0.0,
                far_coc: -1.0,
                near_valid: 0.0,
                far_valid: 1.0,
            };
            16 * 16
        ],
    };
    let mut detail = TileFeatures { values: [0.0; 11] };
    detail.values[9] = 2.0;
    let gpu_small = runner
        .gather_adaptive_frame(&source, &small_blur, &[detail], 105.0)
        .await
        .unwrap();
    assert!((gpu_small[[8, 8, 0]] - 128.0).abs() < 1e-2);

    let mut focused = prepared.clone();
    focused.pixels[8 * 16 + 8].far_coc = 0.0;
    let mut focused_source = source.clone();
    focused_source[[8, 8, 0]] = 12.0;
    let mut detail = TileFeatures { values: [0.0; 11] };
    detail.values[9] = 2.0;
    let output = runner
        .gather_adaptive_frame(&focused_source, &focused, &[detail], 8.0)
        .await
        .unwrap();
    assert_eq!(output[[8, 8, 0]], 12.0);
    assert_eq!(output[[8, 8, 1]], 0.5);
    assert_eq!(output[[8, 8, 3]], 1.0);

    let odd_source_a = Array3::from_shape_fn((11, 13, 4), |(_, _, c)| [3.0, 0.25, 0.5, 1.0][c]);
    let odd_source_b = Array3::from_shape_fn((11, 13, 4), |(_, _, c)| [7.0, 0.75, 0.125, 1.0][c]);
    let odd_prepared = |color| PreparedLayers {
        width: 13,
        height: 11,
        near_edge_distance: vec![-1.0; 143],
        pixels: vec![
            AdaptiveLayerPixel {
                near_rgba: [0.0; 4],
                far_rgba: color,
                near_coc: 0.0,
                far_coc: 4.0,
                near_valid: 0.0,
                far_valid: 1.0,
            };
            13 * 11
        ],
    };
    let mut detail = TileFeatures { values: [0.0; 11] };
    detail.values[9] = 2.0;
    let odd_prepared_a = odd_prepared([3.0, 0.25, 0.5, 1.0]);
    let odd_prepared_b = odd_prepared([7.0, 0.75, 0.125, 1.0]);
    let detail_features = [detail];
    let (output_a, output_b) = tokio::join!(
        runner.gather_adaptive_frame(&odd_source_a, &odd_prepared_a, &detail_features, 8.0),
        runner.gather_adaptive_frame(&odd_source_b, &odd_prepared_b, &detail_features, 8.0),
    );
    let output_a = output_a.unwrap();
    let output_b = output_b.unwrap();
    assert_eq!(output_a.dim(), (11, 13, 4));
    assert_eq!(output_b.dim(), (11, 13, 4));
    assert!(
        (output_a[[10, 12, 0]] - 3.0).abs() < 1e-3,
        "actual {}",
        output_a[[10, 12, 0]]
    );
    assert!((output_b[[10, 12, 0]] - 7.0).abs() < 1e-3);
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that an opaque near layer hides the source and the filled far layer shows.
async fn hardware_adaptive_opaque_near_layer_hides_source_and_filled_far() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, _, c)| [1.0, 0.0, 0.0, 1.0][c]);
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = 1.0;
    feature.values[1] = 8.0;
    feature.values[9] = 2.0;
    for varying_coc in [false, true] {
        let prepared = PreparedLayers {
            width: 16,
            height: 16,
            near_edge_distance: vec![-1.0; 256],
            pixels: (0..256)
                .map(|index| AdaptiveLayerPixel {
                    near_rgba: [0.0, 0.0, 1.0, 1.0],
                    far_rgba: [1.0, 0.0, 0.0, 1.0],
                    near_coc: if varying_coc {
                        1.0 + (index % 16) as f32 / 4.0
                    } else {
                        4.0
                    },
                    far_coc: -0.1,
                    near_valid: 1.0,
                    far_valid: 1.0,
                })
                .collect(),
        };
        let output = runner
            .gather_adaptive_frame_with_budget(
                &source,
                &prepared,
                &[feature],
                8.0,
                crate::AdaptiveSampleBudget::for_quality(
                    crate::datamodel::render::Quality::Accurate,
                ),
            )
            .await
            .unwrap();
        for y in 0..16 {
            for x in 0..16 {
                assert!(
                    output[[y, x, 0]] < 1e-4,
                    "sharp/background leak at ({x}, {y})"
                );
                assert!((output[[y, x, 2]] - 1.0).abs() < 1e-4);
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a near edge keeps the far background sharp.
async fn hardware_adaptive_near_edge_keeps_sharp_far_background() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source =
        Array3::from_shape_fn((16, 16, 4), |(_, _, channel)| [0.0, 0.0, 0.0, 1.0][channel]);
    let mut pixels = Vec::with_capacity(16 * 16);
    for y in 0..16 {
        for x in 0..16 {
            let near = (x as f32 - 4.0).powi(2) + (y as f32 - 8.0).powi(2) <= 64.0;
            pixels.push(AdaptiveLayerPixel {
                near_rgba: if near { [1.0, 0.0, 0.0, 1.0] } else { [0.0; 4] },
                far_rgba: [0.0, 0.0, 1.0, 1.0],
                near_coc: if near { 8.0 } else { 0.0 },
                far_coc: 0.0,
                near_valid: if near { 1.0 } else { 0.0 },
                far_valid: 1.0,
            });
        }
    }
    let prepared = PreparedLayers {
        width: 16,
        height: 16,
        near_edge_distance: vec![-1.0; 256],
        pixels,
    };
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[9] = 2.0;
    feature.values[0] = 8.0;
    feature.values[1] = 8.0;
    let gpu = runner
        .gather_adaptive_frame(&source, &prepared, &[feature], 8.0)
        .await
        .unwrap();
    let edge = (8, 11);
    assert!(
        gpu[[edge.0, edge.1, 0]] > 0.1,
        "near red: {}",
        gpu[[edge.0, edge.1, 0]]
    );
    assert!(
        gpu[[edge.0, edge.1, 2]] > 0.1,
        "far blue: {}",
        gpu[[edge.0, edge.1, 2]]
    );

    let stripe_source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = if x == 8 {
            [1.0, 0.8, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        };
        color[channel]
    });
    let stripe_prepared = PreparedLayers {
        width: 16,
        height: 16,
        near_edge_distance: vec![-1.0; 256],
        pixels: (0..16 * 16)
            .map(|index| {
                let x = index % 16;
                let near = x == 8;
                AdaptiveLayerPixel {
                    near_rgba: if near { [1.0, 0.8, 0.0, 1.0] } else { [0.0; 4] },
                    far_rgba: [0.0, 0.0, 1.0, 1.0],
                    near_coc: if near { 1.5 } else { 0.0 },
                    far_coc: 0.0,
                    near_valid: if near { 1.0 } else { 0.0 },
                    far_valid: 1.0,
                }
            })
            .collect(),
    };
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[9] = 2.0;
    feature.values[0] = 1.5;
    feature.values[1] = 1.5;
    let gpu_stripe = runner
        .gather_adaptive_frame(&stripe_source, &stripe_prepared, &[feature], 105.0)
        .await
        .unwrap();
    assert!(
        gpu_stripe[[8, 8, 0]] > 0.15,
        "red: {}",
        gpu_stripe[[8, 8, 0]]
    );
    assert!(
        gpu_stripe[[8, 8, 1]] > 0.1,
        "green: {}",
        gpu_stripe[[8, 8, 1]]
    );
    assert!(
        gpu_stripe[[8, 8, 2]] > 0.1,
        "blue: {}",
        gpu_stripe[[8, 8, 2]]
    );
}

#[tokio::test]
#[ignore = "requires a hardware GPU"]
/// Hardware check that an output ROI keeps the complete spatial source.
async fn hardware_output_roi_preserves_complete_spatial_source() {
    use crate::datamodel::{Settings, UVector2, Vector2, render::Quality};
    let runner = WgpuRunner::new_adaptive().await.unwrap();
    let (width, height) = (96, 80);
    let source = Array3::from_shape_fn((height, width, 4), |(y, x, c)| {
        if c == 3 {
            1.0
        } else {
            ((x * 17 + y * 11 + c * 29) % 101) as f32 / 50.0
        }
    });
    let coc = ndarray::Array2::from_shape_fn((height, width), |(y, x)| {
        if x < 47 {
            4.0
        } else if y < 38 {
            -4.0
        } else {
            0.0
        }
    });
    let origin = glam::IVec2::new(-37, 23);
    let features = runner
        .classify_tiles(&source, &coc, origin.to_array(), 4.0)
        .await
        .unwrap();
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 4.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 { x: 192, y: 128 };
    settings.render.center = Vector2 { x: 96.0, y: 64.0 };
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.astigmatism.amount = 2.0;
    settings.non_uniform.astigmatism.gamma = 1.4;
    let full = glam::IVec4::new(
        origin.x,
        origin.y,
        origin.x + width as i32,
        origin.y + height as i32,
    );
    let specs = crate::datamodel::render::RenderSpecs::from_rects(full, full);
    let mut lens = crate::datamodel::settings_to_convolve_settings(
        &settings,
        &specs,
        glam::UVec2::splat(256),
        0,
    );
    for quality in [Quality::Fast, Quality::Accurate] {
        let budget = crate::AdaptiveSampleBudget::for_quality(quality);
        lens.process_region = full;
        let reference = runner
            .gather_adaptive_frame_with_lens(
                &source,
                &prepared,
                &features,
                4.0,
                budget,
                Some(&lens),
            )
            .await
            .unwrap();
        lens.process_region =
            glam::IVec4::new(origin.x + 45, origin.y + 34, origin.x + 62, origin.y + 51);
        let cropped = runner
            .gather_adaptive_frame_with_lens(
                &source,
                &prepared,
                &features,
                4.0,
                budget,
                Some(&lens),
            )
            .await
            .unwrap();
        for (y, x, c) in
            (0..height).flat_map(|y| (0..width).flat_map(move |x| (0..4).map(move |c| (y, x, c))))
        {
            if (45..62).contains(&x) && (34..51).contains(&y) {
                assert!(
                    (reference[[y, x, c]] - cropped[[y, x, c]]).abs() < 1e-5,
                    "ROI mismatch {quality:?} at ({x},{y},{c})"
                );
            } else {
                assert_eq!(cropped[[y, x, c]], 0.0, "unrequested output was gathered");
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
/// Verifies on hardware that an aborted request makes both the adaptive gather and the pyramid build return `AdaptiveCancelled`.
async fn aborted_gather_and_pyramid_return_cancelled() {
    use crate::datamodel::render::Quality;
    use crate::error::Error;

    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (w, h) = (8usize, 8usize);
    let source = Array3::from_elem((h, w, 4), 0.5_f32);
    let pixel = AdaptiveLayerPixel {
        near_rgba: [0.0; 4],
        far_rgba: [0.5; 4],
        near_coc: 0.0,
        far_coc: -12.0,
        near_valid: 0.0,
        far_valid: 1.0,
    };
    let prepared = PreparedLayers {
        width: w,
        height: h,
        near_edge_distance: vec![-1.0; w * h],
        pixels: vec![pixel; w * h],
    };
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = -12.0;
    feature.values[1] = 12.0;
    feature.values[9] = 2.0;
    let features = vec![feature; w.div_ceil(16) * h.div_ceil(16)];
    let budget = crate::AdaptiveSampleBudget::for_quality(Quality::Fast);

    crate::abort::set_aborted(true);
    let gather = runner
        .gather_adaptive_frame_with_budget(&source, &prepared, &features, 12.0, budget)
        .await;
    let pyramid = runner.build_adaptive_pyramid(&prepared).await;
    crate::abort::set_aborted(false);

    assert!(
        matches!(gather, Err(Error::AdaptiveCancelled)),
        "{gather:?}"
    );
    assert!(
        matches!(pyramid, Err(Error::AdaptiveCancelled)),
        "{pyramid:?}"
    );
}
