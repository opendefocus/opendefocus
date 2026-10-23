//! Tests for the streaming planner: source cache selection, halos, tile layout, region planning and crops.
use super::*;
use crate::runners::adaptive::AdaptiveLayerPixel;
use bytemuck::Zeroable;

#[test]
/// Verifies cache selection never exceeds the aggregate byte budget.
fn source_resource_cache_selection_respects_aggregate_budget() {
    let resource_bytes = [640, 320, 256, 0, 128];
    assert_eq!(plan_stream_source_cache(&resource_bytes, 0), [false; 5]);
    let selected = plan_stream_source_cache(&resource_bytes, 640);
    assert_eq!(selected, [true, false, false, false, false]);

    let selected = plan_stream_source_cache(&resource_bytes, 640 + 256 + 128);
    assert_eq!(selected, [true, true, false, false, false]);
    assert_eq!(
        plan_stream_source_cache(&resource_bytes, 800),
        [true, false, false, false, true]
    );
    let retained: u64 = selected
        .iter()
        .zip(resource_bytes)
        .filter_map(|(&keep, bytes)| keep.then_some(bytes))
        .sum();
    assert!(retained <= 640 + 256 + 128);
}

#[test]
/// Verifies the halo covers every jump-flood step and the cleanup reach.
fn preparation_halo_covers_all_jump_flood_and_cleanup_reach() {
    assert_eq!(preparation_halo(8.0).unwrap(), 34);
    assert_eq!(preparation_halo(0.0).unwrap(), 4);
    assert!(preparation_halo(f32::INFINITY).is_err());
}

#[test]
/// Verifies the tile layout keeps global alignment for negative origins.
fn tile_layout_preserves_negative_global_alignment() {
    let full = IVector4 {
        x: -17,
        y: -33,
        z: 15,
        w: 20,
    };
    assert_eq!(global_tile_layout(&full).unwrap(), (-32, -48, 3, 5));
}

#[test]
/// Verifies source regions cover a negative origin without overlap and respect the budgets.
fn source_regions_cover_negative_origin_without_overlap_and_respect_budgets() {
    let full = IVector4 {
        x: -37,
        y: -22,
        z: 263,
        w: 158,
    };
    let halo = 20;
    let max_pixels = 8_000;
    let max_core_pixels = 1_024;
    let max_dimension = 100;
    let regions = plan_source_regions_with_core_limit(
        &full,
        halo,
        max_pixels,
        max_core_pixels,
        max_dimension,
    )
    .unwrap();

    for (index, region) in regions.iter().enumerate() {
        assert!(region.core.x >= full.x && region.core.y >= full.y);
        assert!(region.core.z <= full.z && region.core.w <= full.w);
        assert!(region.context.x <= region.core.x && region.context.y <= region.core.y);
        assert!(region.context.z >= region.core.z && region.context.w >= region.core.w);
        let context_width = (region.context.z - region.context.x) as usize;
        let context_height = (region.context.w - region.context.y) as usize;
        assert!(context_width <= max_dimension && context_height <= max_dimension);
        assert!(context_width * context_height <= max_pixels);
        assert!(
            (region.core.z - region.core.x) as usize * (region.core.w - region.core.y) as usize
                <= max_core_pixels
        );
        for other in regions.iter().skip(index + 1) {
            let overlap_x = region.core.x < other.core.z && other.core.x < region.core.z;
            let overlap_y = region.core.y < other.core.w && other.core.y < region.core.w;
            assert!(!(overlap_x && overlap_y), "core rectangles overlap");
        }
    }
    let area: i64 = regions
        .iter()
        .map(|region| {
            i64::from(region.core.z - region.core.x) * i64::from(region.core.w - region.core.y)
        })
        .sum();
    assert_eq!(
        area,
        i64::from(full.z - full.x) * i64::from(full.w - full.y)
    );
}

#[test]
/// Verifies cache selection favours the largest resource and breaks equal sizes by lower index.
fn source_cache_selection_prefers_largest_then_lowest_index() {
    assert_eq!(
        plan_stream_source_cache(&[4, 0, 7, 6], 10),
        [false, false, true, false]
    );
    assert_eq!(plan_stream_source_cache(&[5, 5], 5), [true, false]);
    assert_eq!(plan_stream_source_cache(&[], 10), vec![]);
}

#[test]
/// Verifies the preparation halo is the next power of two above the ceiling radius, plus the dependency margin.
fn preparation_halo_follows_next_power_of_two() {
    assert_eq!(preparation_halo(0.0).unwrap(), 4);
    assert_eq!(preparation_halo(1.0).unwrap(), 6);
    assert_eq!(preparation_halo(2.5).unwrap(), 10);
    assert_eq!(preparation_halo(4.0).unwrap(), 18);
    assert!(matches!(
        preparation_halo(-0.5),
        Err(Error::InvalidParameter)
    ));
    assert!(matches!(
        preparation_halo(f32::NAN),
        Err(Error::InvalidParameter)
    ));
}

#[test]
/// Verifies the tile layout aligns the origin down to the 16-pixel grid, including negative origins, and counts partial tiles.
fn global_tile_layout_aligns_negative_origins_and_counts_partial_tiles() {
    let layout = global_tile_layout(&IVector4 {
        x: -20,
        y: -5,
        z: 30,
        w: 40,
    });
    assert_eq!(layout.unwrap(), (-32, -16, 4, 4));
    assert!(matches!(
        global_tile_layout(&IVector4 {
            x: 4,
            y: 0,
            z: 4,
            w: 16,
        }),
        Err(Error::InvalidDimensions)
    ));
}

#[test]
/// Verifies a tile feature buffer that does not match the global tile count is rejected.
fn owned_tile_features_reject_a_mismatched_global_buffer() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 32,
        w: 32,
    };
    let mut global = vec![None; 3];
    assert!(matches!(
        copy_owned_tile_features(&[], &full, &full, &full, (0, 0, 2, 2), &mut global),
        Err(Error::InvalidDimensions)
    ));
}

#[test]
/// Verifies a frame that fits every limit becomes one region and an oversized frame is split into aligned disjoint cores.
fn source_regions_keep_small_frames_whole_and_tile_large_ones() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 64,
        w: 32,
    };
    let whole = plan_source_regions_with_core_limit(&full, 0, 4096, 4096, 64).unwrap();
    assert_eq!(
        whole,
        [AdaptiveSourceRegion {
            core: full,
            context: full
        }]
    );

    let split = plan_source_regions_with_core_limit(&full, 0, 1024, 1024, 64).unwrap();
    assert_eq!(split.len(), 8);
    assert!(split.iter().all(|region| region.core == region.context));
    assert!(split.iter().all(|region| {
        (region.core.z - region.core.x) * (region.core.w - region.core.y) == 256
    }));
}

#[test]
/// Verifies halo contexts extend each core by the halo and are clipped at the frame edges.
fn source_region_contexts_extend_cores_by_halo_and_clip_at_edges() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 64,
        w: 64,
    };
    let regions = plan_source_regions_with_core_limit(&full, 4, 1024, 1024, 64).unwrap();
    assert_eq!(regions.len(), 8);
    assert_eq!(
        regions[0].core,
        IVector4 {
            x: 0,
            y: 0,
            z: 32,
            w: 16
        }
    );
    assert_eq!(
        regions[0].context,
        IVector4 {
            x: 0,
            y: 0,
            z: 36,
            w: 20
        }
    );
    assert_eq!(
        regions[1].context,
        IVector4 {
            x: 28,
            y: 0,
            z: 64,
            w: 20
        }
    );
}

#[test]
/// Verifies invalid planner inputs are rejected: negative halo, zero limits and an empty frame.
fn source_region_planner_rejects_invalid_inputs() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 64,
        w: 64,
    };
    let empty = IVector4 {
        x: 5,
        y: 0,
        z: 5,
        w: 64,
    };
    assert!(plan_source_regions_with_core_limit(&full, -1, 1024, 1024, 64).is_err());
    assert!(plan_source_regions_with_core_limit(&full, 0, 0, 1024, 64).is_err());
    assert!(plan_source_regions_with_core_limit(&full, 0, 1024, 1024, 0).is_err());
    assert!(plan_source_regions_with_core_limit(&empty, 0, 1024, 1024, 64).is_err());
}

#[test]
/// Verifies a crop of a prepared context returns exactly the core pixels in row-major order.
fn crop_prepared_layers_extracts_the_core_rows() {
    let pixels: Vec<AdaptiveLayerPixel> = (0..8)
        .map(|index| AdaptiveLayerPixel {
            near_coc: index as f32,
            ..AdaptiveLayerPixel::zeroed()
        })
        .collect();
    let prepared = PreparedLayers {
        width: 4,
        height: 2,
        near_edge_distance: (0..8).map(|index| index as f32).collect(),
        pixels,
    };
    let context = IVector4 {
        x: 10,
        y: 20,
        z: 14,
        w: 22,
    };
    let core = IVector4 {
        x: 11,
        y: 21,
        z: 13,
        w: 22,
    };
    let cropped = crop_prepared_layers(&prepared, &context, &core).unwrap();
    assert_eq!((cropped.width, cropped.height), (2, 1));
    let near: Vec<f32> = cropped.pixels.iter().map(|pixel| pixel.near_coc).collect();
    assert_eq!(near, [5.0, 6.0]);
    assert_eq!(cropped.near_edge_distance, [5.0, 6.0]);

    let outside = IVector4 {
        x: 13,
        y: 21,
        z: 15,
        w: 22,
    };
    assert!(crop_prepared_layers(&prepared, &context, &outside).is_err());
    let mismatched = PreparedLayers {
        width: 3,
        ..prepared
    };
    assert!(crop_prepared_layers(&mismatched, &context, &core).is_err());
}

#[test]
/// Verifies RGBA cropping keeps the core rows and columns and rejects a source that does not match its context.
fn crop_rgba_returns_core_region_and_rejects_mismatched_source() {
    let source = Array3::from_shape_fn((2, 4, 4), |(y, x, c)| (y * 100 + x * 10 + c) as f32);
    let context = IVector4 {
        x: 10,
        y: 20,
        z: 14,
        w: 22,
    };
    let core = IVector4 {
        x: 11,
        y: 21,
        z: 13,
        w: 22,
    };
    let cropped = crop_rgba(&source, &context, &core).unwrap();
    assert_eq!(cropped.dim(), (1, 2, 4));
    assert_eq!(cropped[[0, 0, 0]], 110.0);
    assert_eq!(cropped[[0, 1, 0]], 120.0);
    assert_eq!(cropped[[0, 1, 3]], 123.0);

    let wrong_height = Array3::<f32>::zeros((3, 4, 4));
    assert!(crop_rgba(&wrong_height, &context, &core).is_err());
    let empty_core = IVector4 {
        x: 11,
        y: 21,
        z: 11,
        w: 22,
    };
    assert!(crop_rgba(&source, &context, &empty_core).is_err());
}

#[test]
/// Verifies the candidate core sizes are 16-aligned, sorted and unique, with powers of two between the endpoints.
fn aligned_candidates_are_sorted_unique_and_16_aligned() {
    assert_eq!(aligned_candidates(40), [16, 32]);
    assert_eq!(aligned_candidates(10), [16]);
    assert_eq!(aligned_candidates(100), [16, 32, 64, 96]);
}

#[test]
/// Verifies the helper arithmetic: Euclidean alignment, clipped context extent and ceiling core counts.
fn planner_axis_helpers_clip_and_round_correctly() {
    assert_eq!(align_down(-1, 16), -16);
    assert_eq!(align_down(17, 16), 16);
    assert_eq!(max_context_axis(0, 100, 20, 4), 28);
    assert_eq!(max_context_axis(0, 10, 20, 4), 10);
    assert_eq!(max_context_axis(0, 10, 2, -100), 0);
    assert_eq!(axis_count(100, 32), 4);
    assert_eq!(axis_count(0, 16), 0);
    assert_eq!(axis_count(5, 0), 5);
}

#[test]
/// Verifies each tile is copied once from the context whose core owns its centre, and a second copy into the same slot is refused.
fn owned_tile_features_copy_only_tiles_owned_by_the_core() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 32,
        w: 32,
    };
    let local: Vec<TileFeatures> = (0..4)
        .map(|index| TileFeatures {
            values: [index as f32; 11],
        })
        .collect();
    let mut global = vec![None; 4];
    let left_half = IVector4 {
        x: 0,
        y: 0,
        z: 16,
        w: 32,
    };
    copy_owned_tile_features(&local, &full, &left_half, &full, (0, 0, 2, 2), &mut global).unwrap();
    assert_eq!(global[0].map(|tile| tile.values[0]), Some(0.0));
    assert!(global[1].is_none());
    assert_eq!(global[2].map(|tile| tile.values[0]), Some(2.0));
    assert!(global[3].is_none());

    let mut global = vec![None; 4];
    copy_owned_tile_features(&local, &full, &full, &full, (0, 0, 2, 2), &mut global).unwrap();
    let values: Vec<f32> = global.iter().map(|tile| tile.unwrap().values[0]).collect();
    assert_eq!(values, [0.0, 1.0, 2.0, 3.0]);
    assert!(
        copy_owned_tile_features(&local, &full, &full, &full, (0, 0, 2, 2), &mut global).is_err()
    );
}
