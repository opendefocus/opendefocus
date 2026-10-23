//! Hardware tests for lens effects on the adaptive gather: visible change, HDR preservation and edge integrals.
use bytemuck::Zeroable;
use ndarray::Array3;

use crate::runners::adaptive::{AdaptiveLayerPixel, PreparedLayers};

use super::{TileFeatures, WgpuRunner};

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that lens effects change the frame and preserve HDR.
async fn hardware_adaptive_lens_effects_change_frame_and_preserve_hdr() {
    use crate::datamodel::{
        IVector4, Settings, UVector2,
        render::{Quality, RenderSpecs},
    };
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (w, h) = (64usize, 48usize);
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, c)| {
        let bright = (x as i32 - 50).pow(2) + (y as i32 - 12).pow(2) < 16;
        if bright {
            [8.0, 4.0, 2.0, 1.0][c]
        } else {
            [0.05, 0.05, 0.05, 1.0][c]
        }
    });
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 {
        x: w as u32,
        y: h as u32,
    };
    settings.render.center.x = w as f32 * 0.5;
    settings.render.center.y = h as f32 * 0.5;
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
    let budget = crate::AdaptiveSampleBudget::for_quality(Quality::Accurate);
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = -12.0;
    feature.values[1] = 12.0;
    feature.values[9] = 2.0;
    let features = vec![feature; w.div_ceil(16) * h.div_ceil(16)];
    for foreground in [false, true] {
        let prepared = PreparedLayers {
            width: w,
            height: h,
            near_edge_distance: vec![-1.0; w * h],
            pixels: (0..w * h)
                .map(|i| {
                    let color = std::array::from_fn(|c| source[[i / w, i % w, c]]);
                    AdaptiveLayerPixel {
                        near_rgba: if foreground { color } else { [0.0; 4] },
                        far_rgba: if foreground { [0.0; 4] } else { color },
                        near_coc: if foreground { 12.0 } else { 0.0 },
                        far_coc: if foreground { 0.0 } else { -12.0 },
                        near_valid: if foreground { 1.0 } else { 0.0 },
                        far_valid: if foreground { 0.0 } else { 1.0 },
                    }
                })
                .collect(),
        };
        let plain = runner
            .gather_adaptive_frame_with_budget(&source, &prepared, &features, 12.0, budget)
            .await
            .unwrap();
        for effect in 0..6 {
            let axial_amount = if effect == 4 { 1.5 } else { -1.5 };
            let mut settings = settings.clone();
            match effect {
                0 => settings.non_uniform.catseye.enable = true,
                1 => settings.non_uniform.barndoors.enable = true,
                2 => settings.non_uniform.astigmatism.enable = true,
                _ => {
                    settings.non_uniform.axial_aberration.enable = true;
                    if effect >= 4 {
                        settings.non_uniform.axial_aberration.amount = axial_amount;
                    }
                }
            }
            let lens = crate::datamodel::settings_to_convolve_settings(
                &settings,
                &specs,
                glam::UVec2::splat(256),
                (w * h) as u32,
            );
            let gpu = runner
                .gather_adaptive_frame_with_lens(
                    &source,
                    &prepared,
                    &features,
                    12.0,
                    budget,
                    Some(&lens),
                )
                .await
                .unwrap();
            assert_eq!(gpu.dim(), plain.dim());
            let mut delta = 0.0f32;
            for (gpu, plain) in gpu.iter().zip(plain.iter()) {
                assert!(gpu.is_finite(), "effect {effect}, foreground {foreground}");
                delta = delta.max((gpu - plain).abs());
            }
            assert!(
                delta > 0.01,
                "effect {effect} did not change the rendered image"
            );
            let constant_source =
                Array3::from_shape_fn((h, w, 4), |(_, _, c)| [3.0, 0.5, 0.25, 1.0][c]);
            let mut constant = prepared.clone();
            for pixel in &mut constant.pixels {
                if foreground {
                    pixel.near_rgba = [3.0, 0.5, 0.25, 1.0];
                } else {
                    pixel.far_rgba = [3.0, 0.5, 0.25, 1.0];
                }
            }
            let gpu = runner
                .gather_adaptive_frame_with_lens(
                    &constant_source,
                    &constant,
                    &features,
                    12.0,
                    budget,
                    Some(&lens),
                )
                .await
                .unwrap();
            for (actual, expected) in gpu.iter().zip(constant_source.iter()) {
                assert!(
                    (actual - expected).abs() < 1e-3,
                    "constant HDR changed: {actual} != {expected}"
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that extreme barndoors keep opaque constant frames covered.
async fn hardware_extreme_barndoors_keep_opaque_constant_frames_covered() {
    use crate::datamodel::{
        IVector4, Settings, UVector2, Vector2,
        render::{Quality, RenderSpecs},
    };

    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (width, height) = (64usize, 48usize);
    let color = [3.0, 0.5, 0.25, 1.0];
    let source = Array3::from_shape_fn((height, width, 4), |(_, _, channel)| color[channel]);
    let prepared = PreparedLayers {
        width,
        height,
        near_edge_distance: vec![-1.0; width * height],
        pixels: vec![
            AdaptiveLayerPixel {
                far_rgba: color,
                far_coc: -12.0,
                far_valid: 1.0,
                ..AdaptiveLayerPixel::zeroed()
            };
            width * height
        ],
    };
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = -12.0;
    feature.values[1] = 0.0;
    feature.values[9] = 2.0;
    let features = vec![feature; width.div_ceil(16) * height.div_ceil(16)];
    let region = IVector4 {
        x: 0,
        y: 0,
        z: width as i32,
        w: height as i32,
    };
    let specs = RenderSpecs {
        full_region: region,
        render_region: region,
    };
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 {
        x: width as u32,
        y: height as u32,
    };
    settings.render.center = Vector2 {
        x: width as f32 * 0.5,
        y: height as f32 * 0.5,
    };
    settings.non_uniform.barndoors.enable = true;
    settings.non_uniform.barndoors.amount = 2.0;
    settings.non_uniform.barndoors.gamma = 1.0;
    let lens = crate::datamodel::settings_to_convolve_settings(
        &settings,
        &specs,
        glam::UVec2::splat(256),
        (width * height) as u32,
    );

    let output = runner
        .gather_adaptive_frame_with_lens(
            &source,
            &prepared,
            &features,
            12.0,
            crate::AdaptiveSampleBudget::for_quality(Quality::Accurate),
            Some(&lens),
        )
        .await
        .unwrap();
    for y in 0..height {
        for x in 0..width {
            assert!(
                (output[[y, x, 3]] - 1.0).abs() < 1e-5,
                "opaque input became transparent at ({x}, {y}): {}",
                output[[y, x, 3]],
            );
            for channel in 0..3 {
                assert!(
                    (output[[y, x, channel]] - color[channel]).abs() < 1e-3,
                    "constant channel {channel} changed at ({x}, {y})",
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that Accurate lens integrals at the edges and at zero centre are finite and effective.
async fn hardware_high_lens_integrals_at_edges_and_zero_center_are_finite_and_effective() {
    use crate::datamodel::{
        IVector4, Settings, UVector2,
        render::{Quality, RenderSpecs},
    };

    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (w, h) = (33usize, 25usize);
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, channel)| {
        let foreground = (x as i32 - 15).pow(2) + (y as i32 - 12).pow(2) <= 81;
        let base = if foreground {
            [6.0, 0.75, 1.5, 1.0]
        } else {
            [0.2 + x as f32 * 0.03, 0.35 + y as f32 * 0.02, 0.6, 1.0]
        };
        base[channel]
    });
    let prepared = PreparedLayers {
        width: w,
        height: h,
        near_edge_distance: vec![-1.0; w * h],
        pixels: (0..w * h)
            .map(|index| {
                let x = index % w;
                let y = index / w;
                let foreground = (x as i32 - 15).pow(2) + (y as i32 - 12).pow(2) <= 81;
                AdaptiveLayerPixel {
                    near_rgba: if foreground {
                        [6.0, 0.75, 1.5, 1.0]
                    } else {
                        [0.0; 4]
                    },
                    far_rgba: [0.2 + x as f32 * 0.03, 0.35 + y as f32 * 0.02, 0.6, 1.0],
                    near_coc: if foreground {
                        if x == 15 { 1.25 } else { 6.0 }
                    } else {
                        0.0
                    },
                    far_coc: -5.5,
                    near_valid: if foreground { 1.0 } else { 0.0 },
                    far_valid: 1.0,
                }
            })
            .collect(),
    };
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = -5.5;
    feature.values[1] = 6.0;
    feature.values[9] = 11.5;
    let features = vec![feature; w.div_ceil(16) * h.div_ceil(16)];
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
    let budget = crate::AdaptiveSampleBudget::for_quality(Quality::Accurate);
    let baseline = runner
        .gather_adaptive_frame_with_budget(&source, &prepared, &features, 6.0, budget)
        .await
        .unwrap();
    let cases = [
        ("none", 0, 1.0, UVector2 { x: 0, y: 0 }),
        ("catseye", 1, 1.0, UVector2 { x: 0, y: 0 }),
        ("barndoors", 2, 1.0, UVector2 { x: 0, y: 0 }),
        ("astigmatism", 4, 1.0, UVector2 { x: 0, y: 0 }),
        ("astigmatism-gamma", 4, 1.3, UVector2 { x: 0, y: 0 }),
        ("axial", 8, 1.0, UVector2 { x: 0, y: 0 }),
        ("combined", 15, 1.3, UVector2 { x: 16, y: 12 }),
        ("combined-tight", 15, 0.4, UVector2 { x: 3, y: 20 }),
        ("combined-wide", 15, 3.0, UVector2 { x: 29, y: 3 }),
        ("catseye-inverse-aspect", 1, 1.0, UVector2 { x: 3, y: 20 }),
        ("barndoors-inverse-aspect", 2, 1.0, UVector2 { x: 29, y: 3 }),
    ];
    for (name, mask, astig_gamma, center) in cases {
        let mut settings = Settings::default();
        settings.render.resolution = UVector2 {
            x: w as u32,
            y: h as u32,
        };
        settings.render.center.x = center.x as f32;
        settings.render.center.y = center.y as f32;
        settings.non_uniform.catseye.enable = mask & 1 != 0;
        settings.non_uniform.catseye.amount = 0.4;
        settings.non_uniform.catseye.gamma = if name == "combined-tight" {
            0.4
        } else if name == "combined-wide" {
            3.0
        } else {
            1.2
        };
        settings.non_uniform.catseye.softness = 0.2;
        settings.non_uniform.barndoors.enable = mask & 2 != 0;
        settings.non_uniform.barndoors.amount = 0.8;
        settings.non_uniform.barndoors.gamma = settings.non_uniform.catseye.gamma;
        if name.ends_with("inverse-aspect") {
            settings.non_uniform.catseye.inverse = true;
            settings.non_uniform.barndoors.inverse = true;
            settings.defocus.circle_of_confusion.pixel_aspect = 1.8;
        }
        settings.non_uniform.barndoors.top = 55.0;
        settings.non_uniform.barndoors.bottom = 100.0;
        settings.non_uniform.barndoors.left = 100.0;
        settings.non_uniform.barndoors.right = 100.0;
        settings.non_uniform.astigmatism.enable = mask & 4 != 0;
        settings.non_uniform.astigmatism.amount = 0.7;
        settings.non_uniform.astigmatism.gamma = astig_gamma;
        settings.non_uniform.axial_aberration.enable = mask & 8 != 0;
        settings.non_uniform.axial_aberration.amount = 8.0;
        settings.non_uniform.axial_aberration.color_type = 1;
        let lens = crate::datamodel::settings_to_convolve_settings(
            &settings,
            &specs,
            glam::UVec2::splat(256),
            (w * h) as u32,
        );
        let gpu = runner
            .gather_adaptive_frame_with_lens(
                &source,
                &prepared,
                &features,
                6.0,
                budget,
                Some(&lens),
            )
            .await
            .unwrap();
        let mut effect_delta = 0.0f32;
        for (index, (gpu_value, baseline_value)) in gpu.iter().zip(baseline.iter()).enumerate() {
            assert!(gpu_value.is_finite(), "{name}, element={index}");
            effect_delta = effect_delta.max((gpu_value - baseline_value).abs());
        }
        if mask != 0 {
            assert!(
                effect_delta > 1e-3,
                "{name} did not affect the rendered frame"
            );
        }
    }
}
