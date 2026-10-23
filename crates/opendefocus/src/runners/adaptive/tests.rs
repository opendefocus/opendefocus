//! Unit tests for the adaptive stage helpers: input checks, decontamination, subpixel focus, search radii and integral trees.
use super::integral::gather_integral_pixel;
use super::*;
use bytemuck::Zeroable;
use ndarray::{Array2, Array3, s};

#[test]
/// Verifies the classifier input check rejects out-of-range and non-finite CoC values.
fn adaptive_inputs_reject_out_of_range_and_nonfinite_values() {
    use crate::runners::wgpu::validate_adaptive_inputs;
    let rgba = Array3::from_elem((2, 2, 4), 1.0);
    let bad_coc = Array2::from_elem((2, 2), f32::NAN);
    assert!(validate_adaptive_inputs(None, &bad_coc, 4.0).is_err());
    assert!(validate_adaptive_inputs(Some(&rgba), &Array2::zeros((2, 2)), 4.0).is_ok());
    assert!(validate_adaptive_inputs(Some(&rgba), &Array2::from_elem((2, 2), 5.0), 4.0).is_err());
    let mut bad_rgba = rgba;
    bad_rgba[[0, 0, 0]] = f32::INFINITY;
    assert!(validate_adaptive_inputs(Some(&bad_rgba), &Array2::zeros((2, 2)), 4.0).is_err());
}

#[test]
/// Verifies the gather tile class keeps the class and sets the edge bit only at depth discontinuities.
fn gather_tile_class_preserves_class_and_marks_only_depth_discontinuities() {
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[9] = 2.0;
    feature.values[8] = 2.0;
    assert_eq!(adaptive_gather_tile_class(&feature), 2);
    feature.values[8] = 3.0;
    assert_eq!(adaptive_gather_tile_class(&feature), 0x8000_0002);
}

#[test]
/// Verifies only fractional foreground edge pixels are replaced by their clean colour.
fn decontaminated_source_replaces_only_fractional_foreground_edges() {
    let source = Array3::from_shape_fn((1, 3, 4), |(_, x, channel)| {
        [
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ][x][channel]
    });
    let mut pixels = vec![AdaptiveLayerPixel::zeroed(); 3];
    pixels[1].near_rgba = [0.0, 0.5, 0.0, 0.5];
    pixels[1].near_coc = 8.0;
    pixels[1].near_valid = 0.5;
    let prepared = PreparedLayers {
        width: 3,
        height: 1,
        near_edge_distance: vec![-1.0; 3],
        pixels,
    };

    let corrected = prepared.decontaminated_source(&source).unwrap();
    assert_eq!(corrected[[0, 0, 1]], source[[0, 0, 1]]);
    assert_eq!(corrected[[0, 1, 1]], 1.0);
    assert_eq!(corrected[[0, 1, 2]], 0.0);
    assert_eq!(corrected[[0, 2, 2]], source[[0, 2, 2]]);
}

#[test]
/// Verifies a frame that needs no correction is returned without a copy.
fn decontaminated_source_skips_unchanged_frames() {
    let source = Array3::from_elem((1, 1, 4), 1.0);
    let prepared = PreparedLayers {
        width: 1,
        height: 1,
        near_edge_distance: vec![-1.0; 1],
        pixels: vec![AdaptiveLayerPixel::zeroed()],
    };

    assert!(prepared.decontaminated_source(&source).is_none());
}

#[test]
/// Verifies a one-pixel focus line is removed while focused objects stay sharp.
fn subpixel_focus_resolve_removes_a_line_without_softening_focused_objects() {
    for (before, after, expected) in [
        (-16.0, 16.0, 0.0),
        (-0.05, 0.05, 0.0),
        (-1.0, 1.0, 0.0),
        (-16.0, 0.0, 1.0),
        (-16.0, -16.0, 1.0),
    ] {
        for vertical in [false, true] {
            let mut pixels = vec![AdaptiveLayerPixel::zeroed(); 9];
            for (position, pixel) in pixels.iter_mut().enumerate() {
                if position < 4 {
                    pixel.far_coc = before;
                    pixel.far_valid = 1.0;
                } else {
                    pixel.near_valid = 1.0;
                    pixel.near_coc = if position == 4 { 0.0 } else { after };
                }
            }
            let (width, height) = if vertical { (1, 9) } else { (9, 1) };
            let prepared = PreparedLayers {
                width,
                height,
                pixels,
                near_edge_distance: vec![-1.0; 9],
            };
            let mut output = Array3::from_shape_fn((height, width, 4), |(y, x, c)| {
                if c == 3 || (x + y == 4 && c == 0) {
                    1.0
                } else if (3..=5).contains(&(x + y)) && c == 0 {
                    0.2
                } else {
                    0.0
                }
            });
            let original = output.clone();
            prepared.resolve_subpixel_focus(&mut output);
            let (x, y) = if vertical { (0, 4) } else { (4, 0) };
            assert_eq!(output[[y, x, 0]], expected);
            assert_eq!(output[[y, x, 3]], 1.0);
            if expected == 0.0 {
                assert!(
                    output
                        .indexed_iter()
                        .all(|((_, _, c), value)| { c == 3 || *value == 0.0 })
                );
            } else {
                assert_eq!(output, original);
            }
        }
    }
}

#[test]
/// Verifies a repair across a chunk seam matches the same region taken from an expanded ROI.
fn subpixel_focus_repair_matches_an_expanded_roi_across_a_chunk_seam() {
    let (width, height) = (32usize, 9usize);
    let pixels = (0..height)
        .flat_map(|_| {
            (0..width).map(|x| {
                if x < 16 {
                    AdaptiveLayerPixel {
                        far_coc: -16.0,
                        far_valid: 1.0,
                        ..AdaptiveLayerPixel::zeroed()
                    }
                } else {
                    AdaptiveLayerPixel {
                        near_coc: if x == 16 || x == 24 { 0.0 } else { 16.0 },
                        near_valid: 1.0,
                        ..AdaptiveLayerPixel::zeroed()
                    }
                }
            })
        })
        .collect();
    let prepared = PreparedLayers {
        width,
        height,
        pixels,
        near_edge_distance: vec![-1.0; width * height],
    };
    let make_output = || {
        Array3::from_shape_fn((height, width, 4), |(_, x, channel)| match channel {
            0 => {
                if x == 16 {
                    1.0
                } else {
                    x as f32 * 0.25
                }
            }
            1 => 0.5,
            2 => 3.0,
            _ => 1.0,
        })
    };
    let mut full = make_output();
    prepared.resolve_subpixel_focus(&mut full);

    // Model a rendered ROI whose requested core straddles x=16. Its
    // output includes the seven-pixel repair context around the core.
    let origin = [8usize, 0usize];
    let mut roi = make_output()
        .slice(ndarray::s![.., origin[0].., ..])
        .to_owned();
    resolve_subpixel_focus_with(
        width,
        height,
        origin,
        |x, y| prepared.pixels[y * width + x],
        &mut roi,
    );
    for y in 0..height {
        for x in 13..20 {
            for channel in 0..4 {
                assert_eq!(roi[[y, x - origin[0], channel]], full[[y, x, channel]]);
            }
        }
    }
    for y in 0..height {
        assert_eq!(roi[[y, 24 - origin[0], 0]], 6.0);
    }
}

#[test]
/// Verifies full-coverage edge pixels keep their clean colour.
fn decontaminated_source_uses_clean_color_for_full_coverage_edges() {
    let source = Array3::from_shape_vec((1, 1, 4), vec![0.0, 0.9, 0.1, 1.0]).unwrap();
    let prepared = PreparedLayers {
        width: 1,
        height: 1,
        near_edge_distance: vec![-1.0; 1],
        pixels: vec![AdaptiveLayerPixel {
            near_rgba: [0.0, 1.0, 0.0, 1.0],
            near_coc: 8.0,
            near_valid: 1.0,
            far_rgba: [0.0; 4],
            far_coc: 0.0,
            far_valid: 0.0,
        }],
    };

    let corrected = prepared.decontaminated_source(&source).unwrap();
    assert_eq!(corrected[[0, 0, 1]], 1.0);
    assert_eq!(corrected[[0, 0, 2]], 0.0);
}

#[test]
/// Verifies the classifier overlay keeps the source and maps tiles correctly under an offset origin.
fn overlay_preserves_source_and_maps_offset_tiles() {
    let source = Array3::from_shape_fn((17, 20, 4), |(y, x, c)| (y + x + c) as f32);
    let mut features = vec![TileFeatures { values: [0.0; 11] }; 6];
    features[0].values[9] = 0.0;
    features[1].values[9] = 1.0;
    features[4].values[9] = 2.0;
    let overlay = classifier_overlay(&source, &features, [3, 2], [15, 15]);
    assert_eq!(overlay.slice(s![.., ..20, ..]), source.view());
    assert_eq!(overlay[[0, 20, 3]], 1.0);
    assert_eq!(overlay[[1, 21, 0]], 1.0);
}

#[test]
/// Verifies near search reach keeps small features and nearby large occluders.
fn near_search_reach_keeps_small_features_and_adjacent_large_occluders() {
    let mut features = vec![TileFeatures { values: [0.0; 11] }; 3];
    features[0].values[0] = 1.5;
    features[0].values[1] = 1.5;
    let small = adaptive_near_search_radii(&features, 3, 1, 105.0);
    assert_eq!(small, [1.5, 1.5, 0.0]);

    features[0].values[0] = 8.0;
    features[0].values[1] = 8.0;
    let large = adaptive_near_search_radii(&features, 3, 1, 105.0);
    assert_eq!(large, [8.0, 8.0, 0.0]);
}

#[test]
/// Verifies the packed far reach covers focused tiles from large negative CoC sources.
fn packed_far_reach_covers_focused_tiles_from_large_negative_coc_sources() {
    let mut features = vec![TileFeatures { values: [0.0; 11] }; 4];
    // Canonical adaptive CoC is positive for foreground and negative for
    // background. A large background source in tile 0 can cover focused
    // pixels two tiles away, whose own tile CoC remains zero.
    features[0].values[0] = -40.0;
    features[0].values[1] = -12.0;
    features[1].values[0] = -0.25;
    features[1].values[1] = 0.0;

    let packed = adaptive_near_far_search_radii(&features, 4, 1, 105.0);
    assert_eq!(&packed[..4], &[0.0; 4]);
    assert_eq!(&packed[4..], &[40.0, 40.0, 40.0, 40.0]);
}

#[test]
/// Verifies prepared far metadata reaches focused output tiles.
fn prepared_far_metadata_reaches_focused_output_tiles() {
    let mut pixels = vec![AdaptiveLayerPixel::zeroed(); 64 * 16];
    pixels[8 * 64].far_coc = -40.0;
    pixels[8 * 64].far_valid = 1.0;
    pixels[8 * 64 + 40].near_valid = 1.0;

    let prepared = PreparedLayers {
        width: 64,
        height: 16,
        pixels,
        near_edge_distance: vec![-1.0; 64 * 16],
    };
    let packed = adaptive_layer_search_radii(&prepared, 105.0, glam::UVec2::ZERO);

    assert_eq!(&packed[..4], &[0.0; 4]);
    assert_eq!(packed[4 + 2], 40.0);
}

#[test]
/// Verifies layer search radii stay correct for offset classifier tiles.
fn layer_search_radii_preserve_offset_classifier_tiles() {
    let mut pixels = vec![AdaptiveLayerPixel::zeroed(); 32];
    pixels[0].near_coc = 1.0;
    pixels[0].near_valid = 1.0;
    pixels[17].far_coc = -1.0;
    pixels[17].far_valid = 1.0;
    let prepared = PreparedLayers {
        width: 32,
        height: 1,
        pixels,
        near_edge_distance: vec![-1.0; 32],
    };
    let radii = adaptive_layer_search_radii(&prepared, 1.0, glam::UVec2::new(15, 0));
    assert_eq!(radii.len(), 6);
    assert_eq!(radii[0], 1.0);
    assert_eq!(radii[3 + 2], 1.0);
}

#[test]
/// Verifies integral trees keep the prepared coverage and the canonical far radius.
fn integral_trees_preserve_prepared_coverage_and_canonical_far_radius() {
    let mut pixels = vec![AdaptiveLayerPixel::zeroed(); 4];
    for pixel in &mut pixels {
        pixel.near_rgba = [0.25, 0.125, 0.0, 0.5];
        pixel.near_coc = 1.5;
        pixel.near_valid = 0.5;
        pixel.far_rgba = [0.0, 0.0, 0.75, 1.0];
        pixel.far_coc = -8.0;
        pixel.far_valid = 1.0;
    }
    let prepared = PreparedLayers {
        width: 2,
        height: 2,
        pixels,
        near_edge_distance: vec![-1.0; 4],
    };

    let (near_nodes, far_nodes) = build_gather_integral_nodes(&prepared).unwrap();
    // Root is packed node 0. Layout: premul 0..4, bounds 4..8,
    // children 8..12, coc_min 12, coc_max 13, weighted_valid 14,
    // occupied_area 15.
    let field = |node: &[u32; 16], index: usize| f32::from_bits(node[index]);
    let (near_root, far_root) = (&near_nodes[0], &far_nodes[0]);

    // smoothstep(1,2,1.5) = 0.5, applied to the prepared coverage.
    assert!((field(near_root, 15) - 1.0).abs() < 1e-6);
    assert!((field(near_root, 14) - 4.0 * 0.25 / 2.25).abs() < 1e-6);
    // Near RGBA is premultiplied, so the node stores recovered straight color.
    assert!((field(near_root, 0) - field(near_root, 14) * 0.5).abs() < 1e-6);
    assert_eq!(field(far_root, 12), 8.0);
    assert_eq!(field(far_root, 13), 8.0);
    assert_eq!(field(far_root, 14), 4.0 / 64.0);
}

#[test]
/// Verifies a one-pixel hierarchy still produces a non-empty GPU binding payload.
fn empty_single_pixel_hierarchy_has_a_nonempty_gpu_binding_payload() {
    let prepared = PreparedLayers {
        width: 1,
        height: 1,
        pixels: vec![AdaptiveLayerPixel::zeroed()],
        near_edge_distance: vec![-1.0],
    };
    let pixel = prepared.pixels[0];
    assert_eq!(
        crate::kernel::build_integral_tree_packed(1, 1, |_| gather_integral_pixel(pixel, true))
            .len(),
        0
    );
    assert_eq!(
        crate::kernel::build_integral_tree_packed(1, 1, |_| gather_integral_pixel(pixel, false))
            .len(),
        0
    );
    assert_eq!(
        build_gather_integral_nodes(&prepared).unwrap(),
        (vec![[0; 16]], vec![[0; 16]])
    );
}
