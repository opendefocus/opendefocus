//! Streaming planner for large frames: partitions the source into disjoint cores with halo context,
//! prepares each partition and crops the prepared layers back to its core.
use crate::datamodel::IVector4;
use ndarray::{Array2, Array3};

use crate::{
    error::{Error, Result},
    runners::{WgpuRunner, adaptive::CocPolarity},
};

use super::adaptive::{PreparedLayers, build_gather_integral_nodes};
use super::wgpu::{TileFeatures, integral_subtree_ends};

/// Bytes of GPU storage the streamed gather accumulator needs per output pixel: 72 floats.
pub(crate) const ADAPTIVE_OUTPUT_BYTES_PER_PIXEL: u64 = 72 * 4;

/// One source partition. `context` is prepared with its halo; `core` is the
/// non-overlapping rectangle contributed to the global source hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdaptiveSourceRegion {
    /// Non-overlapping rectangle this partition contributes to the global hierarchy, as (x0, y0, x1, y1).
    pub core: IVector4,
    /// Prepared rectangle including the halo, as (x0, y0, x1, y1).
    pub context: IVector4,
}

/// One streamed source partition: its prepared layers and the integral trees built from them.
/// The trees are built once per source core and reused by every streamed destination gather.
#[derive(Debug)]
pub(crate) struct AdaptiveSourceChunk {
    /// Core and context rectangles of this partition.
    pub region: AdaptiveSourceRegion,
    /// Pristine RGBA source pixels of the context rectangle.
    pub source: Array3<f32>,
    /// Prepared near and far layers of the context rectangle.
    pub prepared: PreparedLayers,
    /// Near-layer integral tree in packed GPU layout.
    pub near_integral_nodes: Vec<[u32; 16]>,
    /// Far-layer integral tree in packed GPU layout.
    pub far_integral_nodes: Vec<[u32; 16]>,
    /// Subtree end indices of the packed trees.
    pub subtree_ends: Vec<u32>,
}

/// Select immutable source GPU resources to retain for one streamed render.
/// Favor the largest reusable chunks first because their setup cost is
/// proportional to their pixel and node counts. The byte estimates include
/// the source texture, four base-level prepared textures, and packed trees.
pub(crate) fn plan_stream_source_cache(resource_bytes: &[u64], budget: u64) -> Vec<bool> {
    let mut order: Vec<usize> = (0..resource_bytes.len()).collect();
    order.sort_by_key(|&index| (std::cmp::Reverse(resource_bytes[index]), index));
    let mut selected = vec![false; resource_bytes.len()];
    let mut remaining = budget;
    for index in order {
        let bytes = resource_bytes[index];
        if bytes != 0 && bytes <= remaining {
            selected[index] = true;
            remaining -= bytes;
        }
    }
    selected
}

/// A core receives exactly the same jump-flood candidate chain as the full
/// frame when its halo covers the sum of all pass offsets. The shader starts
/// at the first power of two strictly greater than ceil(radius).
///
/// # Errors
/// Returns `InvalidParameter` when the radius is negative or not finite.
pub(crate) fn preparation_halo(max_radius_px: f32) -> Result<i32> {
    if !max_radius_px.is_finite() || max_radius_px < 0.0 {
        return Err(Error::InvalidParameter);
    }
    let radius = max_radius_px.ceil() as u64;
    let mut step = 1u64;
    while step <= radius {
        step = step.checked_mul(2).ok_or(Error::SizeLimitExceeded)?;
    }
    let dependency = step
        .checked_mul(2)
        .and_then(|sum| sum.checked_sub(1))
        // The first seeded frontier examines the 3x3 neighborhood, then the
        // CPU cleanup pass may inspect two additional pixels.
        .and_then(|sum| sum.checked_add(3))
        .ok_or(Error::SizeLimitExceeded)?;
    i32::try_from(dependency).map_err(|_| Error::SizeLimitExceeded)
}

/// Return the base tile coordinate and dimensions for a possibly cropped
/// frame, preserving classifier alignment to negative global origins.
///
/// # Errors
/// Returns `InvalidDimensions` when the full region is empty, or `SizeLimitExceeded` when its tile counts do not fit in `usize`.
pub(crate) fn global_tile_layout(full: &IVector4) -> Result<(i32, i32, usize, usize)> {
    let _width = full
        .z
        .checked_sub(full.x)
        .filter(|value| *value > 0)
        .ok_or(Error::InvalidDimensions)?;
    let _height = full
        .w
        .checked_sub(full.y)
        .filter(|value| *value > 0)
        .ok_or(Error::InvalidDimensions)?;
    let tile_x = full.x.div_euclid(16) * 16;
    let tile_y = full.y.div_euclid(16) * 16;
    let tiles_x = (usize::try_from(full.z - tile_x)
        .map_err(|_| Error::SizeLimitExceeded)?
        .saturating_add(15))
        / 16;
    let tiles_y = (usize::try_from(full.w - tile_y)
        .map_err(|_| Error::SizeLimitExceeded)?
        .saturating_add(15))
        / 16;
    if tiles_x == 0 || tiles_y == 0 {
        return Err(Error::InvalidDimensions);
    }
    Ok((tile_x, tile_y, tiles_x, tiles_y))
}

/// Copy the final classifier values for global tiles whose clamped center is
/// owned by this core. The context halo includes the propagation reach, so
/// these values match a full-frame classification.
///
/// # Errors
/// Returns `InvalidDimensions` when a tile index falls outside the feature buffer.
pub(crate) fn copy_owned_tile_features(
    local: &[TileFeatures],
    context: &IVector4,
    core: &IVector4,
    full: &IVector4,
    tile_layout: (i32, i32, usize, usize),
    global: &mut [Option<TileFeatures>],
) -> Result<()> {
    let (base_x, base_y, tiles_x, tiles_y) = tile_layout;
    if global.len() != tiles_x * tiles_y {
        return Err(Error::InvalidDimensions);
    }
    let context_base_x = context.x.div_euclid(16) * 16;
    let context_base_y = context.y.div_euclid(16) * 16;
    let local_tiles_x = (usize::try_from(context.z - context_base_x)
        .map_err(|_| Error::SizeLimitExceeded)?
        .saturating_add(15))
        / 16;
    let local_tiles_y = (usize::try_from(context.w - context_base_y)
        .map_err(|_| Error::SizeLimitExceeded)?
        .saturating_add(15))
        / 16;
    if local.len() != local_tiles_x * local_tiles_y {
        return Err(Error::InvalidDimensions);
    }
    for tile_y in 0..tiles_y {
        let origin_y = base_y + tile_y as i32 * 16;
        let owner_y = origin_y
            .saturating_add(8)
            .clamp(full.y, full.w.saturating_sub(1));
        if owner_y < core.y || owner_y >= core.w {
            continue;
        }
        let local_y = (origin_y - context_base_y).div_euclid(16);
        if local_y < 0 || local_y >= local_tiles_y as i32 {
            return Err(Error::InvalidDimensions);
        }
        for tile_x in 0..tiles_x {
            let origin_x = base_x + tile_x as i32 * 16;
            let owner_x = origin_x
                .saturating_add(8)
                .clamp(full.x, full.z.saturating_sub(1));
            if owner_x < core.x || owner_x >= core.z {
                continue;
            }
            let local_x = (origin_x - context_base_x).div_euclid(16);
            if local_x < 0 || local_x >= local_tiles_x as i32 {
                return Err(Error::InvalidDimensions);
            }
            let target = tile_y * tiles_x + tile_x;
            if global[target].is_some() {
                return Err(Error::InvalidDimensions);
            }
            global[target] = Some(local[local_y as usize * local_tiles_x + local_x as usize]);
        }
    }
    Ok(())
}

/// Prepare each complete frame partition once. The halo is used only for
/// preparation and classification; source integrals receive the disjoint
/// core so overlapping contexts are never counted twice.
///
/// # Errors
/// Returns an error when a source chunk cannot be planned, prepared or uploaded.
pub(crate) async fn prepare_source_chunks(
    runner: &WgpuRunner,
    rgba: &Array3<f32>,
    coc: &Array2<f32>,
    full: &IVector4,
    regions: &[AdaptiveSourceRegion],
    max_radius_px: f32,
) -> Result<(
    Vec<AdaptiveSourceChunk>,
    Vec<TileFeatures>,
    (i32, i32, usize, usize),
)> {
    let tile_layout = global_tile_layout(full)?;
    let (_, _, tiles_x, tiles_y) = tile_layout;
    let mut tile_features = vec![None; tiles_x * tiles_y];
    let mut chunks = Vec::with_capacity(regions.len());
    for region in regions {
        let context = &region.context;
        let y0 = (context.y - full.y) as usize;
        let y1 = (context.w - full.y) as usize;
        let x0 = (context.x - full.x) as usize;
        let x1 = (context.z - full.x) as usize;
        let source_context = rgba.slice(ndarray::s![y0..y1, x0..x1, ..]).to_owned();
        let coc_context = coc.slice(ndarray::s![y0..y1, x0..x1]).to_owned();
        let local_features = runner
            .classify_tiles(
                &source_context,
                &coc_context,
                [context.x, context.y],
                max_radius_px,
            )
            .await?;
        copy_owned_tile_features(
            &local_features,
            context,
            &region.core,
            full,
            tile_layout,
            &mut tile_features,
        )?;
        let local_prepared = runner
            .prepare_adaptive_layers(
                &source_context,
                &coc_context,
                max_radius_px,
                CocPolarity::PositiveForeground,
            )
            .await?;
        let prepared = crop_prepared_layers(&local_prepared, context, &region.core)?;
        let source = crop_rgba(&source_context, context, &region.core)?;
        // Release halo and context allocations before constructing the retained
        // packed trees for this core to keep the cache's peak bounded.
        drop(local_prepared);
        drop(local_features);
        drop(source_context);
        drop(coc_context);
        let (near_integral_nodes, far_integral_nodes) = build_gather_integral_nodes(&prepared)?;
        let subtree_ends = integral_subtree_ends(&near_integral_nodes)?;
        chunks.push(AdaptiveSourceChunk {
            region: *region,
            source,
            prepared,
            near_integral_nodes,
            far_integral_nodes,
            subtree_ends,
        });
    }
    let tile_features = tile_features
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::InvalidDimensions)?;
    Ok((chunks, tile_features, tile_layout))
}

/// Split a complete source frame into globally aligned cores. The context
/// rectangles overlap by `halo` but never exceed the per-operation limits.
///
/// # Errors
/// Returns `InvalidParameter` when `halo` is negative or the limits are zero.
#[cfg(test)]
pub(crate) fn plan_source_regions(
    full: &IVector4,
    halo: i32,
    max_pixels: usize,
    max_dimension: usize,
) -> Result<Vec<AdaptiveSourceRegion>> {
    plan_source_regions_with_core_limit(full, halo, max_pixels, max_pixels, max_dimension)
}

/// Splits the full frame into disjoint cores of at most `max_core_pixels` pixels, each with `halo` pixels of context.
/// Contexts stay within `max_pixels` and `max_dimension` per side, and cores align to the global 16-pixel tile grid.
/// # Errors
/// Returns `InvalidParameter` when `halo` is negative or a limit is zero.
pub(crate) fn plan_source_regions_with_core_limit(
    full: &IVector4,
    halo: i32,
    max_pixels: usize,
    max_core_pixels: usize,
    max_dimension: usize,
) -> Result<Vec<AdaptiveSourceRegion>> {
    if halo < 0 || max_pixels == 0 || max_core_pixels == 0 || max_dimension == 0 {
        return Err(Error::InvalidParameter);
    }
    let width = full
        .z
        .checked_sub(full.x)
        .filter(|value| *value > 0)
        .ok_or(Error::InvalidDimensions)?;
    let height = full
        .w
        .checked_sub(full.y)
        .filter(|value| *value > 0)
        .ok_or(Error::InvalidDimensions)?;
    let full_pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(Error::SizeLimitExceeded)?;
    if width as usize <= max_dimension
        && height as usize <= max_dimension
        && full_pixels <= max_pixels
        && full_pixels <= max_core_pixels
    {
        return Ok(vec![AdaptiveSourceRegion {
            core: *full,
            context: *full,
        }]);
    }

    let dimension_core = i32::try_from(max_dimension)
        .unwrap_or(i32::MAX)
        .saturating_sub(halo.saturating_mul(2));
    let mut best = None;
    for core_height in aligned_candidates(height.min(dimension_core.max(16))) {
        for core_width in aligned_candidates(width.min(dimension_core.max(16))) {
            if (core_width as usize)
                .checked_mul(core_height as usize)
                .is_none_or(|pixels| pixels > max_core_pixels)
            {
                continue;
            }
            let context_width = max_context_axis(full.x, full.z, core_width, halo);
            let context_height = max_context_axis(full.y, full.w, core_height, halo);
            let pixels = (context_width as usize).checked_mul(context_height as usize);
            if context_width as usize > max_dimension
                || context_height as usize > max_dimension
                || (core_width as usize)
                    .checked_mul(core_height as usize)
                    .is_none_or(|count| count > max_core_pixels)
                || pixels.is_none_or(|count| count > max_pixels)
            {
                continue;
            }
            let count =
                axis_count(width, core_width).saturating_mul(axis_count(height, core_height));
            let cost = pixels.unwrap_or(usize::MAX) as u128 * count as u128;
            if best.is_none_or(|(best_cost, _, _)| cost < best_cost) {
                best = Some((cost, core_width, core_height));
            }
        }
    }
    let (_, core_width, core_height) = best.ok_or(Error::SizeLimitExceeded)?;
    let mut regions = Vec::new();
    let mut core_y = align_down(full.y, core_height);
    while core_y < full.w {
        let core_y_end = core_y.saturating_add(core_height).min(full.w);
        let mut core_x = align_down(full.x, core_width);
        while core_x < full.z {
            let core_x_end = core_x.saturating_add(core_width).min(full.z);
            let core = IVector4 {
                x: core_x.max(full.x),
                y: core_y.max(full.y),
                z: core_x_end.min(full.z),
                w: core_y_end.min(full.w),
            };
            let context = IVector4 {
                x: core_x.saturating_sub(halo).max(full.x),
                y: core_y.saturating_sub(halo).max(full.y),
                z: core_x_end.saturating_add(halo).min(full.z),
                w: core_y_end.saturating_add(halo).min(full.w),
            };
            let context_width = (context.z - context.x) as usize;
            let context_height = (context.w - context.y) as usize;
            if context_width > max_dimension
                || context_height > max_dimension
                || context_width
                    .checked_mul(context_height)
                    .is_none_or(|count| count > max_pixels)
            {
                return Err(Error::SizeLimitExceeded);
            }
            regions.push(AdaptiveSourceRegion { core, context });
            core_x = core_x_end;
        }
        core_y = core_y_end;
    }
    Ok(regions)
}

/// Extract a prepared core from a halo-prepared context so overlapping halos
/// cannot contribute twice to exact source integrals.
///
/// # Errors
/// Returns `InvalidDimensions` when the core is not inside the context or the prepared layers do not match it.
pub(crate) fn crop_prepared_layers(
    prepared: &PreparedLayers,
    context: &IVector4,
    core: &IVector4,
) -> Result<PreparedLayers> {
    if core.x < context.x
        || core.y < context.y
        || core.z > context.z
        || core.w > context.w
        || core.z <= core.x
        || core.w <= core.y
        || prepared.pixels.len() != prepared.width * prepared.height
        || prepared.near_edge_distance.len() != prepared.pixels.len()
        || (context.z - context.x) as usize != prepared.width
        || (context.w - context.y) as usize != prepared.height
    {
        return Err(Error::InvalidDimensions);
    }
    let left = (core.x - context.x) as usize;
    let top = (core.y - context.y) as usize;
    let width = (core.z - core.x) as usize;
    let height = (core.w - core.y) as usize;
    let mut pixels = Vec::with_capacity(width * height);
    let mut near_edge_distance = Vec::with_capacity(width * height);
    for y in top..top + height {
        let start = y * prepared.width + left;
        pixels.extend_from_slice(&prepared.pixels[start..start + width]);
        near_edge_distance.extend_from_slice(&prepared.near_edge_distance[start..start + width]);
    }
    Ok(PreparedLayers {
        width,
        height,
        pixels,
        near_edge_distance,
    })
}

/// Crops `source`, which covers the `context` rectangle, down to the `core` rectangle. Both rectangles are (x0, y0, x1, y1) in full-frame pixels.
/// # Errors
/// Returns `InvalidDimensions` when `core` is empty or outside `context`, or when `source` does not match `context`.
pub(crate) fn crop_rgba(
    source: &Array3<f32>,
    context: &IVector4,
    core: &IVector4,
) -> Result<Array3<f32>> {
    if core.x < context.x
        || core.y < context.y
        || core.z > context.z
        || core.w > context.w
        || core.z <= core.x
        || core.w <= core.y
        || source.dim().0 != (context.w - context.y) as usize
        || source.dim().1 != (context.z - context.x) as usize
    {
        return Err(Error::InvalidDimensions);
    }
    Ok(source
        .slice(ndarray::s![
            (core.y - context.y) as usize..(core.w - context.y) as usize,
            (core.x - context.x) as usize..(core.z - context.x) as usize,
            ..
        ])
        .to_owned())
}

/// Returns the sorted, unique candidate core sizes: 16, the largest 16-aligned size up to `maximum`, and the powers of two between them.
fn aligned_candidates(maximum: i32) -> Vec<i32> {
    let maximum = align_down(maximum.max(16), 16);
    let mut values = vec![16, maximum];
    let mut value = 32;
    while value < maximum {
        values.push(value);
        value = value.saturating_mul(2);
        if value <= 0 {
            break;
        }
    }
    values.sort_unstable();
    values.dedup();
    values
}

/// Rounds `value` down to a multiple of `alignment`, using Euclidean division.
fn align_down(value: i32, alignment: i32) -> i32 {
    value.div_euclid(alignment) * alignment
}

/// Returns the context extent along one axis: the frame span, limited to the core plus the halo on both sides, and never negative.
fn max_context_axis(full_min: i32, full_max: i32, core: i32, halo: i32) -> i32 {
    full_max
        .saturating_sub(full_min)
        .min(core.saturating_add(halo.saturating_mul(2)))
        .max(0)
}

/// Returns how many cores of size `core` are needed to cover `length` along one axis. A core size below 1 is treated as 1.
fn axis_count(length: i32, core: i32) -> usize {
    (length.max(0) as usize).div_ceil(core.max(1) as usize)
}
#[cfg(test)]
/// Tests for the streaming planner.
mod tests;
