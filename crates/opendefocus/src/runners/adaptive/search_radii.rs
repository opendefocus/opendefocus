//! Conservative per-tile gather radii: how far each 16x16 output tile must look for contributing near and far source tiles.
use super::PreparedLayers;
use crate::runners::wgpu::TileFeatures;

/// Compute a conservative near-layer gather radius for each 16x16 output tile.
/// A near source tile contributes its maximum positive `CoC` when its footprint
/// can reach the output tile; the pixel kernel still rejects samples outside
/// each source pixel's actual `CoC`.
#[must_use]
pub fn adaptive_near_search_radii(
    features: &[TileFeatures],
    tiles_x: usize,
    tiles_y: usize,
    max_radius_px: f32,
) -> Vec<f32> {
    adaptive_search_radii(features, tiles_x, tiles_y, max_radius_px, |feature| {
        feature.values[0].max(feature.values[1]).max(0.0)
    })
}

/// Compute a conservative far-layer gather radius for every 16x16 output tile.
/// The renderer's canonical `CoC` convention is positive foreground and negative
/// background, so the far extent is the magnitude of each tile's minimum signed `CoC`.
#[must_use]
pub fn adaptive_far_search_radii(
    features: &[TileFeatures],
    tiles_x: usize,
    tiles_y: usize,
    max_radius_px: f32,
) -> Vec<f32> {
    adaptive_search_radii(features, tiles_x, tiles_y, max_radius_px, |feature| {
        (-feature.values[0].min(feature.values[1])).max(0.0)
    })
}

/// Return tile reaches in kernel buffer order: N near radii followed by N far radii.
pub(crate) fn adaptive_search_radii(
    features: &[TileFeatures],
    tiles_x: usize,
    tiles_y: usize,
    max_radius_px: f32,
    radius_from_feature: impl Fn(&TileFeatures) -> f32,
) -> Vec<f32> {
    /// Tile edge length in pixels, used to turn tile gaps into pixel distances.
    const TILE_SIZE: usize = 16;
    let max_tile_gap = (max_radius_px.max(0.0) / TILE_SIZE as f32).ceil() as usize + 1;
    let source_radii: Vec<f32> = features
        .iter()
        .map(|feature| radius_from_feature(feature).min(max_radius_px.max(0.0)))
        .collect();
    let mut output = vec![0.0; tiles_x * tiles_y];
    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let start_x = tile_x.saturating_sub(max_tile_gap);
            let end_x = (tile_x + max_tile_gap + 1).min(tiles_x);
            let start_y = tile_y.saturating_sub(max_tile_gap);
            let end_y = (tile_y + max_tile_gap + 1).min(tiles_y);
            let mut radius = 0.0f32;
            for source_y in start_y..end_y {
                let gap_y = source_y.abs_diff(tile_y).saturating_sub(1) * TILE_SIZE;
                for source_x in start_x..end_x {
                    let source_radius = source_radii[source_y * tiles_x + source_x];
                    if source_radius <= 0.5 {
                        continue;
                    }
                    let gap_x = source_x.abs_diff(tile_x).saturating_sub(1) * TILE_SIZE;
                    if gap_x as f32 <= source_radius && gap_y as f32 <= source_radius {
                        radius = radius.max(source_radius);
                    }
                }
            }
            output[tile_y * tiles_x + tile_x] = radius;
        }
    }
    output
}

/// Returns the near radii followed by the far radii, in the buffer order the kernel reads.
pub(crate) fn adaptive_near_far_search_radii(
    features: &[TileFeatures],
    tiles_x: usize,
    tiles_y: usize,
    max_radius_px: f32,
) -> Vec<f32> {
    let mut radii = adaptive_near_search_radii(features, tiles_x, tiles_y, max_radius_px);
    radii.extend(adaptive_far_search_radii(
        features,
        tiles_x,
        tiles_y,
        max_radius_px,
    ));
    radii
}

/// Derive reach from the canonical layer metadata so preparation polarity and
/// classifier sign conventions cannot disagree during gather.
/// Returns per-tile gather radii derived from the prepared layers, offset by `tile_origin`.
/// Each tile's extreme CoC comes from its valid pixels, so the reach follows the canonical sign convention.
pub(crate) fn adaptive_layer_search_radii(
    prepared: &PreparedLayers,
    max_radius_px: f32,
    tile_origin: glam::UVec2,
) -> Vec<f32> {
    let tiles_x = (prepared.width + tile_origin.x as usize).div_ceil(16);
    let tiles_y = (prepared.height + tile_origin.y as usize).div_ceil(16);
    let mut bounds = vec![TileFeatures { values: [0.0; 11] }; tiles_x * tiles_y];
    for y in 0..prepared.height {
        for x in 0..prepared.width {
            let pixel = prepared.pixels[y * prepared.width + x];
            let feature = &mut bounds
                [((y + tile_origin.y as usize) / 16) * tiles_x + (x + tile_origin.x as usize) / 16];
            if pixel.near_valid > 0.0 {
                feature.values[1] = feature.values[1].max(pixel.near_coc);
            }
            if pixel.far_valid > 0.0 {
                feature.values[0] = feature.values[0].min(pixel.far_coc);
            }
        }
    }
    adaptive_near_far_search_radii(&bounds, tiles_x, tiles_y, max_radius_px)
}
