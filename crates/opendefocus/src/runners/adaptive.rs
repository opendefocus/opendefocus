//! Adaptive renderer types: prepared near and far layers, the source pyramid, CoC polarity and tile-class packing.
//! The submodules hold the integral, search-radius, subpixel and preview stages.
/// Integral-tree construction for the exact Accurate gather.
mod integral;
/// Preview and overlay images for prepared layers and classifier tiles.
mod previews;
/// Gather search radii for near and far source tiles.
mod search_radii;
/// Subpixel focus resolution.
mod subpixel;
#[cfg(test)]
/// Tests for the adaptive stage helpers.
mod tests;

pub(crate) use integral::build_gather_integral_nodes;
pub use previews::classifier_overlay;
pub use search_radii::{adaptive_far_search_radii, adaptive_near_search_radii};
pub(crate) use search_radii::{adaptive_layer_search_radii, adaptive_near_far_search_radii};
pub(crate) use subpixel::resolve_subpixel_focus_with;

use super::wgpu::TileFeatures;

/// Pack the classifier's blur class and depth-discontinuity bit for the gather.
/// The high bit is reserved so the kernel can select the edge-only sample tier.
pub(crate) fn adaptive_gather_tile_class(feature: &TileFeatures) -> u32 {
    let class = feature.values[9].round().clamp(0.0, 2.0) as u32;
    let depth_edge = (feature.values[8].round() as u32) & 1 != 0;
    class | ((depth_edge as u32) << 31)
}

/// Sign convention for the explicitly supplied `CoC` buffer. Preparation
/// canonicalizes either convention to positive foreground and negative far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CocPolarity {
    /// The input CoC is positive in front of focus; the sign is already canonical.
    PositiveForeground,
    /// The input CoC is negative in front of focus; preparation flips it to the canonical sign.
    NegativeForeground,
}

/// Prepared per-pixel near and far layers. RGB remains premultiplied linear HDR;
/// valid near `CoC` is canonicalized positive and valid far `CoC` negative.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct AdaptiveLayerPixel {
    /// Premultiplied linear RGBA of the near layer.
    pub near_rgba: [f32; 4],
    /// Premultiplied linear RGBA of the far layer.
    pub far_rgba: [f32; 4],
    /// Near-layer CoC radius in pixels, canonically positive.
    pub near_coc: f32,
    /// Far-layer CoC radius in pixels, canonically negative.
    pub far_coc: f32,
    /// Near-layer validity: 1 where the layer has geometry at this pixel, 0 otherwise.
    pub near_valid: f32,
    /// Far-layer validity: 1 where the layer has geometry at this pixel, 0 otherwise.
    pub far_valid: f32,
}

/// CPU-visible diagnostic result from the adaptive near/far preparation pass.
#[derive(Debug, Clone)]
pub struct PreparedLayers {
    /// Width of the prepared layers in pixels.
    pub width: usize,
    /// Height of the prepared layers in pixels.
    pub height: usize,
    /// One entry per pixel, `width * height` in total.
    pub pixels: Vec<AdaptiveLayerPixel>,
    /// Distance from each foreground pixel to the estimated silhouette where
    /// near-layer coverage changes locally; -1 where no such edge is present.
    pub near_edge_distance: Vec<f32>,
}

#[derive(Debug, Clone)]
/// One level of the source pyramid, with its resolution and per-pixel layers.
pub struct AdaptivePyramidLevel {
    /// Width of this level in pixels.
    pub width: usize,
    /// Height of this level in pixels.
    pub height: usize,
    /// Per-pixel near and far layer data for this level.
    pub pixels: Vec<AdaptiveLayerPixel>,
}

#[derive(Debug, Clone)]
/// Mip pyramid of prepared layers used by the adaptive gather.
pub struct AdaptiveSourcePyramid {
    /// Level zero is the original prepared input; subsequent levels halve each
    /// dimension, folding an odd edge into the final cell to match GPU mip sizes.
    /// The near/far layers remain independent.
    pub levels: Vec<AdaptivePyramidLevel>,
}
