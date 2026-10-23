//! Hardware tests for layer preparation: polarity, continuation, alpha, subpixel focus and pyramid energy.
use bytemuck::Zeroable;
use ndarray::{Array2, Array3};

use crate::runners::adaptive::{AdaptiveLayerPixel, CocPolarity, PreparedLayers};

use super::{TileFeatures, WgpuRunner};

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that neutral focus is prepared as a sharp occluding layer.
async fn hardware_prepares_neutral_focus_as_a_sharp_occluding_layer() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let mut rgba = Array3::zeros((16, 16, 4));
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| if x <= 6 { -4.0 } else { 0.0 });
    for y in 0..16 {
        for x in 0..16 {
            rgba[[y, x, 3]] = 1.0;
            if x <= 6 {
                rgba[[y, x, 0]] = 2.0;
            } else {
                rgba[[y, x, 2]] = 0.5;
            }
        }
    }
    let layers = runner
        .prepare_adaptive_layers(&rgba, &coc, 6.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let far_seed = layers.pixels[8 * 16 + 5];
    assert_eq!(far_seed.near_valid, 0.0);
    assert_eq!(far_seed.far_rgba[0], 2.0);
    assert_eq!(far_seed.far_valid, 1.0);
    let focused = layers.pixels[8 * 16 + 8];
    assert_eq!(focused.near_rgba[2], 0.5);
    assert_eq!(focused.near_valid, 1.0);
    assert_eq!(focused.far_rgba[0], 2.0);
    assert_eq!(focused.far_valid, 1.0);
    assert_eq!(layers.near_edge_distance[8 * 16 + 7], 1.0);
    assert_eq!(layers.near_edge_distance[8 * 16 + 8], 2.0);
    assert!(layers.pixels.iter().all(|pixel| {
        pixel
            .near_rgba
            .iter()
            .chain(pixel.far_rgba.iter())
            .all(|v| v.is_finite())
    }));
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that preparation canonicalizes the negative foreground polarity.
async fn hardware_preparation_canonicalizes_negative_foreground_polarity() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = if x < 8 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        };
        color[channel]
    });
    let positive_coc = Array2::from_shape_fn((16, 16), |(_, x)| if x < 8 { 4.0 } else { -4.0 });
    let negative_coc = positive_coc.mapv(|value| -value);

    let positive = runner
        .prepare_adaptive_layers(&source, &positive_coc, 4.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let negative = runner
        .prepare_adaptive_layers(&source, &negative_coc, 4.0, CocPolarity::NegativeForeground)
        .await
        .unwrap();

    for (positive, negative) in positive.pixels.iter().zip(&negative.pixels) {
        assert_eq!(positive.near_rgba, negative.near_rgba);
        assert_eq!(positive.far_rgba, negative.far_rgba);
        assert_eq!(positive.near_coc, negative.near_coc);
        assert_eq!(positive.far_coc, negative.far_coc);
        assert_eq!(positive.near_valid, negative.near_valid);
        assert_eq!(positive.far_valid, negative.far_valid);
    }
    assert!(negative.pixels.iter().all(|pixel| {
        (pixel.near_valid == 0.0 || pixel.near_coc >= 0.0)
            && (pixel.far_valid == 0.0 || pixel.far_coc <= 0.0)
    }));
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that focus continuation fills an HDR point and preserves its energy.
async fn hardware_focus_continuation_fills_hdr_point_and_preserves_energy() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((64, 64, 4), |(y, x, channel)| {
        if (x, y) == (32, 32) {
            [14.244, 14.244, 14.244, 1.0][channel]
        } else {
            [0.0, 0.0, 0.0, 1.0][channel]
        }
    });
    let coc = Array2::from_shape_fn(
        (64, 64),
        |(y, x)| {
            if (x, y) == (32, 32) { 8.0 } else { 0.0 }
        },
    );
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let point = prepared.pixels[32 * 64 + 32];
    assert_eq!(point.near_valid, 1.0);
    assert_eq!(point.far_valid, 1.0);
    assert_eq!(point.far_coc, 0.0);
    assert_eq!(point.far_rgba, [0.0, 0.0, 0.0, 1.0]);
    assert!(
        prepared
            .pixels
            .iter()
            .zip(coc.iter())
            .all(|(pixel, coc)| *coc != 0.0 || pixel.far_valid == 0.0)
    );

    let mut features = vec![TileFeatures { values: [0.0; 11] }; 16];
    for feature in &mut features {
        feature.values[9] = 2.0;
    }
    let output = runner
        .gather_adaptive_frame_with_budget(
            &source,
            &prepared,
            &features,
            8.0,
            crate::AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate),
        )
        .await
        .unwrap();
    assert!(
        output
            .indexed_iter()
            .filter(|((_, _, channel), _)| *channel == 3)
            .all(|(_, alpha)| *alpha > 0.999)
    );
    let red_energy: f32 = output.iter().step_by(4).sum();
    assert!(
        (red_energy - 14.244).abs() < 0.02,
        "focused continuation changed the point RGB energy: {red_energy}"
    );
    let green_energy: f32 = output.iter().skip(1).step_by(4).sum();
    let blue_energy: f32 = output.iter().skip(2).step_by(4).sum();
    assert!((green_energy - red_energy).abs() < 1e-3);
    assert!((blue_energy - red_energy).abs() < 1e-3);
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
#[allow(
    clippy::float_cmp,
    reason = "far validity and CoC flags are exact 0/1 and 0 markers written by the classifier"
)]
/// Hardware check that focused background continuation fills the alpha of a defocused disc.
async fn hardware_focused_background_continuation_fills_defocused_disk_alpha() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((64, 64, 4), |(y, x, channel)| {
        let dx = x as i32 - 32;
        let dy = y as i32 - 32;
        if dx * dx + dy * dy <= 100 {
            [2.0, 1.0, 0.5, 1.0][channel]
        } else {
            [0.0, 0.0, 0.0, 1.0][channel]
        }
    });
    let coc = Array2::from_shape_fn((64, 64), |(y, x)| {
        let dx = x as i32 - 32;
        let dy = y as i32 - 32;
        if dx * dx + dy * dy <= 100 { 8.0 } else { 0.0 }
    });
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let center = prepared.pixels[32 * 64 + 32];
    assert_eq!(center.near_valid, 1.0);
    assert_eq!(
        center.far_valid, 0.0,
        "deep foreground copied distant focus"
    );
    assert!(
        prepared
            .pixels
            .iter()
            .zip(coc.iter())
            .all(|(pixel, coc)| *coc != 0.0 || pixel.far_valid == 0.0)
    );
    assert!(
        prepared
            .pixels
            .iter()
            .zip(coc.iter())
            .any(|(pixel, coc)| *coc > 0.5 && pixel.far_valid == 1.0 && pixel.far_coc == 0.0)
    );

    let mut features = vec![TileFeatures { values: [0.0; 11] }; 16];
    for feature in &mut features {
        feature.values[9] = 2.0;
    }
    let output = runner
        .gather_adaptive_frame_with_budget(
            &source,
            &prepared,
            &features,
            8.0,
            crate::AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate),
        )
        .await
        .unwrap();
    assert!(
        output
            .indexed_iter()
            .filter(|((_, _, channel), _)| *channel == 3)
            .all(|(_, alpha)| *alpha > 0.999)
    );
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that transparent near and far points preserve alpha and colour energy.
async fn hardware_transparent_near_far_points_preserve_alpha_and_color_energy() {
    use crate::datamodel::render::Quality;

    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (width, height) = (33usize, 33usize);
    let point = [0.75, 0.5, 0.25, 1.0];
    let mut source = Array3::zeros((height, width, 4));
    for channel in 0..4 {
        source[[height / 2, width / 2, channel]] = point[channel];
    }

    for radius in [8.0f32, -8.0] {
        let coc = Array2::from_elem((height, width), radius);
        let features = runner
            .classify_tiles(&source, &coc, [0, 0], 8.0)
            .await
            .unwrap();
        let prepared = runner
            .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
            .await
            .unwrap();
        for quality in [Quality::Accurate, Quality::Fast] {
            let output = runner
                .gather_adaptive_frame_with_budget(
                    &source,
                    &prepared,
                    &features,
                    8.0,
                    crate::AdaptiveSampleBudget::for_quality(quality),
                )
                .await
                .unwrap();
            for channel in 0..4 {
                let energy: f32 = output.index_axis(ndarray::Axis(2), channel).iter().sum();
                assert!(
                    (energy - point[channel]).abs() < 0.2,
                    "radius={radius}, quality={quality:?}, channel={channel}: energy={energy}"
                );
            }
            assert!(
                output[[height / 2, width / 2, 3]] < 0.5,
                "radius={radius}, quality={quality:?}: center alpha was not blurred"
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a focused surface stays sharp beside a blurred far layer.
async fn hardware_focused_surface_stays_sharp_beside_blurred_far_layer() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = if x < 8 {
            [0.0, 0.0, 1.0, 1.0]
        } else {
            [1.0, 0.0, 0.0, 1.0]
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| if x < 8 { -8.0 } else { 0.0 });
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let focused_edge = prepared.pixels[8 * 16 + 8];
    assert_eq!(focused_edge.near_valid, 1.0);
    assert_eq!(focused_edge.near_rgba[0], 1.0);
    assert_eq!(focused_edge.near_rgba[2], 0.0);
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = -8.0;
    feature.values[1] = 0.0;
    feature.values[9] = 2.0;
    let output = runner
        .gather_adaptive_frame(&source, &prepared, &[feature], 8.0)
        .await
        .unwrap();

    let focused_near_edge = output[[8, 11, 0]];
    assert!(
        focused_near_edge > 0.98,
        "foreground interior was blurred: {focused_near_edge}"
    );
    assert!(output[[8, 8, 0]] > 0.98, "focused silhouette was softened");
    assert!(
        output[[8, 8, 2]] < 0.02,
        "far color bled across the silhouette"
    );
    let focused = output[[8, 12, 0]];
    assert!(focused > 0.98, "focused red surface was blurred: {focused}");
    assert!(output[[8, 12, 2]] < 0.02, "far blue bled onto focus plane");
    let far = output[[8, 3, 2]];
    assert!(far > 0.98, "far background lost its blue: {far}");
    assert!(output[[8, 3, 0]] < 0.02, "focused red bled into far blur");
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that an uncertain silhouette and focus crossing keep full coverage.
async fn hardware_uncertain_silhouette_and_focus_crossing_keep_full_coverage() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    for pattern in 0..3 {
        let coc = Array2::from_shape_fn((16, 16), |(_, x)| {
            if x < 8 {
                -8.0
            } else if pattern == 2 && x > 8 {
                8.0
            } else {
                0.0
            }
        });
        let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
            let color = if pattern == 0 {
                [0.25, 0.25, 0.25, 1.0]
            } else if x < 8 || (pattern == 2 && x == 8) {
                [0.0, 0.0, 1.0, 1.0]
            } else if x == 8 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0, 1.0]
            };
            color[channel]
        });
        let prepared = runner
            .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
            .await
            .unwrap();
        let edge = prepared.pixels[8 * 16 + 8];
        assert_eq!(edge.near_valid, 1.0, "invented edge coverage: {edge:?}");
        for channel in 0..4 {
            assert_eq!(edge.near_rgba[channel], source[[8, 8, channel]]);
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a subpixel focus crossing leaves no unfiltered column.
async fn hardware_subpixel_focus_crossing_has_no_unfiltered_column() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = if x == 8 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| match x {
        0..8 => -16.0,
        8 => 0.0,
        _ => 16.0,
    });
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, 16.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = -16.0;
    feature.values[1] = 16.0;
    feature.values[9] = 2.0;
    let gpu = runner
        .gather_adaptive_frame(&source, &prepared, &[feature], 16.0)
        .await
        .unwrap();
    let neighbors = f32::midpoint(gpu[[8, 7, 0]], gpu[[8, 9, 0]]);
    assert!(
        (gpu[[8, 8, 0]] - neighbors).abs() < 0.005,
        "unfiltered focus column: {} vs neighbors {neighbors}",
        gpu[[8, 8, 0]]
    );
    for y in 0..16 {
        for x in 0..16 {
            assert!(
                (gpu[[y, x, 3]] - 1.0).abs() < 1e-5,
                "opaque focus-crossing input became translucent at ({x}, {y}): {}",
                gpu[[y, x, 3]],
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a focused edge without a CoC step preserves the source and composites the far plate.
async fn hardware_focused_edge_without_coc_step_preserves_source_and_composites_far_plate() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
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

    let edge = layers.pixels[8 * 16 + 8];
    assert_eq!(edge.near_valid, 1.0);
    assert_eq!(edge.near_rgba[1], 0.25);
    assert_eq!(edge.near_rgba[2], 0.75, "edge layer: {edge:?}");
    assert_eq!(edge.far_rgba[2], 1.0);
    assert_eq!(edge.far_valid, 1.0);

    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = 0.0;
    feature.values[1] = -8.0;
    feature.values[9] = 2.0;
    let output = runner
        .gather_adaptive_frame(&source, &layers, &[feature], 8.0)
        .await
        .unwrap();
    let focused_edge = [output[[8, 8, 0]], output[[8, 8, 1]], output[[8, 8, 2]]];
    assert!(
        (focused_edge[1] - source[[8, 8, 1]]).abs() < 0.05,
        "focused source green changed: {focused_edge:?}"
    );
    assert!(
        (focused_edge[2] - source[[8, 8, 2]]).abs() < 0.05,
        "focused source blue changed: {focused_edge:?}"
    );
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a defocused foreground edge keeps fractional coverage.
async fn hardware_defocused_foreground_edge_keeps_fractional_coverage() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = match x {
            0..8 => [0.0, 1.0, 0.0, 1.0],
            8 => [0.0, 0.5, 0.5, 1.0],
            _ => [0.0, 0.0, 1.0, 1.0],
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| match x {
        0..8 => 8.0,
        8 => 5.0,
        _ => -8.0,
    });
    let layers = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();

    let edge = layers.pixels[8 * 16 + 8];
    assert_eq!(edge.near_coc, 5.0);
    assert_eq!(edge.near_valid, 0.5);
    assert_eq!(edge.near_rgba[1], 0.5);
    assert_eq!(edge.near_rgba[2], 0.0);
    let corrected = layers.decontaminated_source(&source).unwrap();
    assert_eq!(corrected[[8, 8, 1]], 1.0);
    assert_eq!(corrected[[8, 8, 2]], 0.0);
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that near coverage does not depend on the foreground colour gradient.
async fn hardware_near_coverage_is_independent_of_foreground_color_gradient() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let coc = Array2::from_shape_fn((16, 32), |(_, x)| if x < 16 { -8.0 } else { 8.0 });
    let flat = Array3::from_shape_fn((16, 32, 4), |(_, x, channel)| {
        let color = if x < 16 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0, 1.0]
        };
        color[channel]
    });
    // This is a fully opaque foreground intensity gradient: the near
    // surface is still present at x=16, but its green channel ramps from
    // 0.5 to 1.0. RGB projection against the black far seed mistakes the
    // edge color for half geometric coverage.
    let gradient = Array3::from_shape_fn((16, 32, 4), |(_, x, channel)| {
        let color = match x {
            0..16 => [0.0, 0.0, 0.0, 1.0],
            16 => [0.0, 0.5, 0.0, 1.0],
            _ => [0.0, 1.0, 0.0, 1.0],
        };
        color[channel]
    });
    let flat_layers = runner
        .prepare_adaptive_layers(&flat, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let gradient_layers = runner
        .prepare_adaptive_layers(&gradient, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let flat_edge = flat_layers.pixels[8 * 32 + 16];
    let gradient_edge = gradient_layers.pixels[8 * 32 + 16];

    // A smoothly changing CoC is not independent geometric evidence for
    // partial coverage either; preserve the genuine half-intensity edge.
    let smooth_coc = Array2::from_shape_fn((16, 32), |(_, x)| {
        if x < 16 {
            -8.0
        } else {
            8.0 - ((x - 16) as f32 * 0.5).min(4.0)
        }
    });
    let smooth_layers = runner
        .prepare_adaptive_layers(&gradient, &smooth_coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let smooth_edge = smooth_layers.pixels[8 * 32 + 16];

    // Fully focused pixels preserve their actual premultiplied RGBA,
    // including alpha, even next to a color transition.
    let focused_source = Array3::from_shape_fn((16, 32, 4), |(y, x, channel)| {
        if (y, x) == (8, 20) {
            [0.0, 0.25, 0.0, 0.5][channel]
        } else {
            gradient[[y, x, channel]]
        }
    });
    let focused_coc = Array2::zeros((16, 32));
    let focused = runner
        .prepare_adaptive_layers(
            &focused_source,
            &focused_coc,
            8.0,
            CocPolarity::PositiveForeground,
        )
        .await
        .unwrap();
    for y in 0..16 {
        for x in 0..32 {
            let pixel = focused.pixels[y * 32 + x];
            assert_eq!(pixel.near_valid, 1.0);
            for channel in 0..4 {
                assert_eq!(pixel.near_rgba[channel], focused_source[[y, x, channel]]);
            }
        }
    }

    assert_eq!(flat_edge.near_valid, 1.0);
    assert_eq!(
        gradient_edge.near_valid, 1.0,
        "opaque foreground geometry changed with RGB gradient: {gradient_edge:?}"
    );
    assert_eq!(gradient_edge.near_rgba, [0.0, 0.5, 0.0, 1.0]);
    assert_eq!(smooth_edge.near_valid, 1.0);
    assert_eq!(smooth_edge.near_rgba, [0.0, 0.5, 0.0, 1.0]);
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that the far layer uses uncontaminated colour at depth edges.
async fn hardware_far_layer_uses_uncontaminated_color_at_depth_edges() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let source = Array3::from_shape_fn((16, 16, 4), |(_, x, channel)| {
        let color = match x {
            0..8 => [0.0, 0.0, 1.0, 1.0],
            8 => [1.0, 0.0, 0.0, 1.0],
            _ => [0.0, 1.0, 0.0, 1.0],
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((16, 16), |(_, x)| if x < 8 { 0.0 } else { -8.0 });
    let layers = runner
        .prepare_adaptive_layers(&source, &coc, 8.0, CocPolarity::PositiveForeground)
        .await
        .unwrap();

    let edge = layers.pixels[8 * 16 + 8];
    assert_eq!(edge.near_valid, 0.0);
    assert_eq!(edge.far_rgba[0], 0.0);
    assert_eq!(edge.far_rgba[1], 1.0);
    assert_eq!(edge.far_valid, 1.0);
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that the pyramid preserves layer energy and odd edges.
async fn hardware_pyramid_preserves_layers_energy_and_odd_edges() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let mut pixels = vec![AdaptiveLayerPixel::zeroed(); 9];
    for pixel in &mut pixels {
        pixel.far_rgba = [0.0, 0.0, 0.4, 1.0];
        pixel.far_coc = 2.0;
        pixel.far_valid = 1.0;
    }
    pixels[0].near_rgba = [4.0, 0.0, 0.0, 1.0];
    pixels[0].near_coc = -4.0;
    pixels[0].near_valid = 1.0;
    pixels[0].far_rgba = [0.0; 4];
    pixels[0].far_valid = 0.0;
    let prepared = PreparedLayers {
        width: 3,
        height: 3,
        near_edge_distance: vec![-1.0; 9],
        pixels,
    };
    let pyramid = runner.build_adaptive_pyramid(&prepared).await.unwrap();
    assert_eq!(
        pyramid
            .levels
            .iter()
            .map(|l| (l.width, l.height))
            .collect::<Vec<_>>(),
        [(3, 3), (1, 1)]
    );
    let mixed = pyramid.levels[1].pixels[0];
    assert!((mixed.near_rgba[0] - 4.0 / 9.0).abs() < 1e-6);
    assert_eq!(mixed.near_coc, -4.0);
    assert!((mixed.near_valid - 1.0 / 9.0).abs() < 1e-6);
    assert!((mixed.far_rgba[2] - 0.4 * 8.0 / 9.0).abs() < 1e-6);
    assert_eq!(mixed.far_coc, 2.0);
    assert!((mixed.far_valid - 8.0 / 9.0).abs() < 1e-6);
    assert!(pyramid.levels.iter().flat_map(|l| &l.pixels).all(|pixel| {
        pixel
            .near_rgba
            .iter()
            .chain(pixel.far_rgba.iter())
            .all(|v| v.is_finite())
    }));
}
