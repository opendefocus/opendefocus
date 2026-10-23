//! Native WGSL kernel sources and the CPU helpers that feed them: sample offsets, integral trees and aperture bounds.
//! The shader sources are embedded at compile time and validated by `cargo xtask --compile --gpu`.
/// Sample offset tables for the adaptive gather patterns.
mod adaptive_gather_offsets;
/// Host-side builders for integral trees and aperture bounds.
mod stages;

pub(crate) use adaptive_gather_offsets::{
    ACCURATE_DETAIL_OFFSET, ACCURATE_DETAIL_POINTS, ACCURATE_SMOOTH_OFFSET, ACCURATE_SMOOTH_POINTS,
    FAST_DETAIL_OFFSET, FAST_DETAIL_POINTS, FAST_SMOOTH_OFFSET, FAST_SMOOTH_POINTS,
    adaptive_gather_sample_offsets,
};

pub use stages::adaptive_aperture_integral::{
    ApertureMip, build_aperture_bounds, build_aperture_bounds_layout,
};
pub use stages::adaptive_integral::{IntegralPixel, build_integral_tree_packed};

/// WGSL source of the convolution (gather) kernel, embedded at compile time.
pub const ADAPTIVE_CONVOLUTION_WGSL: &str = include_str!("shaders/adaptive_convolution.wgsl");
/// WGSL source of the mip-pyramid reduction kernel, embedded at compile time.
pub const ADAPTIVE_PYRAMID_WGSL: &str = include_str!("shaders/adaptive_pyramid.wgsl");
/// WGSL source of the tile classifier kernel, embedded at compile time.
pub const ADAPTIVE_CLASSIFIER_WGSL: &str = include_str!("shaders/adaptive_classifier.wgsl");
/// WGSL source of the layer preparation kernel, embedded at compile time.
pub const ADAPTIVE_PREPARE_WGSL: &str = include_str!("shaders/adaptive_prepare.wgsl");
/// WGSL source of the focal and bokeh preview kernel, embedded at compile time.
pub const ADAPTIVE_PREVIEW_WGSL: &str = include_str!("shaders/adaptive_preview.wgsl");

/// Pack a prepared layer and its optional per-pixel foreground-edge distance.
#[allow(
    clippy::inline_always,
    reason = "keeps the existing forced inlining of the packing path"
)]
#[inline(always)]
#[cfg(test)]
pub(crate) fn adaptive_pack_pixel_with_edge_distance(
    source: &[f32],
    index: usize,
    near_edge_distance: f32,
) -> [[f32; 4]; 4] {
    let base = index * 12;
    let near_valid = source[base + 10];
    let far_valid = source[base + 11];
    [
        [
            source[base],
            source[base + 1],
            source[base + 2],
            source[base + 3],
        ],
        [
            source[base + 4],
            source[base + 5],
            source[base + 6],
            source[base + 7],
        ],
        [
            source[base + 8] * near_valid,
            near_valid,
            near_edge_distance.max(0.0),
            (near_edge_distance >= 0.0) as u32 as f32,
        ],
        [source[base + 9] * far_valid, far_valid, 0.0, 0.0],
    ]
}

#[cfg(test)]
/// Tests for the packed layer layout.
mod adaptive_pack_tests {
    use super::adaptive_pack_pixel_with_edge_distance;

    #[test]
    /// Verifies that the edge distance is stored in the near metadata word.
    fn pack_stores_optional_edge_distance_in_near_metadata() {
        let mut source = [0.0; 12];
        source[8] = 2.0;
        source[9] = -4.0;
        source[10] = 1.0;
        source[11] = 1.0;

        let packed = adaptive_pack_pixel_with_edge_distance(&source, 0, 3.5);
        assert_eq!(packed[2], [2.0, 1.0, 3.5, 1.0]);
    }
}
