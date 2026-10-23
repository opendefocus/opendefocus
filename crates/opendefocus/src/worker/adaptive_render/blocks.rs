//! Block planning for the adaptive renderer: sizes of the globally aligned output cores and the context each core needs.
use super::{ADAPTIVE_PREPARE_BYTES_PER_PIXEL, ADAPTIVE_TILE_SIZE};
use crate::datamodel::IVector4;
use crate::{
    error::{Error, Result},
    runners::WgpuRunner,
};

/// Largest buffer one streamed preparation or output accumulator may use: 512 MiB. The device limits can only lower it.
const ADAPTIVE_GPU_WORKING_BUFFER_BYTES: u64 = 512 * 1024 * 1024;

/// Returns the per-buffer byte limit for streamed work: the smaller of the device storage binding and buffer limits, and [`ADAPTIVE_GPU_WORKING_BUFFER_BYTES`].
pub(super) fn adaptive_gpu_working_buffer_bytes(runner: &WgpuRunner) -> u64 {
    let limits = runner.device.limits();
    limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size)
        .min(ADAPTIVE_GPU_WORKING_BUFFER_BYTES)
}

/// Returns how many source pixels one block may hold, from the device buffer limits and the preparation bytes per pixel. Saturates at `usize::MAX`.
pub(super) fn adaptive_block_pixel_limit(runner: &WgpuRunner) -> usize {
    let limits = runner.device.limits();
    let bytes = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    pixels_per_budget(bytes, ADAPTIVE_PREPARE_BYTES_PER_PIXEL)
}

/// Returns how many pixels of `bytes_per_pixel` fit in `budget_bytes`. Saturates at `usize::MAX`.
pub(super) fn pixels_per_budget(budget_bytes: u64, bytes_per_pixel: u64) -> usize {
    usize::try_from(budget_bytes / bytes_per_pixel).unwrap_or(usize::MAX)
}

/// Rounds `value` down to a multiple of `alignment`. An alignment of `i32::MAX` leaves `value` unchanged.
pub(super) fn align_down(value: i32, alignment: i32) -> i32 {
    if alignment == i32::MAX {
        value
    } else {
        value.div_euclid(alignment) * alignment
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Output core size chosen for a frame, and whether one pass covers the whole requested region.
pub(super) struct AdaptiveBlockPlan {
    /// Core width in pixels.
    pub(super) core_width: i32,
    /// Core height in pixels.
    pub(super) core_height: i32,
    /// True when one pass covers the whole requested region.
    pub(super) single_pass: bool,
}

/// Pick globally aligned rectangular output cores whose padded source context
/// fits the GPU's per-operation pixel budget. Use the actual full/requested
/// rectangles when measuring the halo so image and ROI edges can use the
/// context they really have instead of paying for a hypothetical square block.
///
/// # Errors
/// Returns `SizeLimitExceeded` when even the smallest aligned core needs more source context than the storage budget allows.
pub(super) fn plan_adaptive_blocks(
    full: &IVector4,
    requested: &IVector4,
    halo: i32,
    max_pixels: usize,
    max_dimension: usize,
) -> Result<AdaptiveBlockPlan> {
    if max_pixels == 0 || max_dimension == 0 || halo < 0 {
        return Err(Error::InvalidParameter);
    }
    if full.z <= full.x
        || full.w <= full.y
        || requested.z <= requested.x
        || requested.w <= requested.y
    {
        return Err(Error::InvalidDimensions);
    }
    let full_width = (full.z - full.x) as usize;
    let full_height = (full.w - full.y) as usize;
    if full_width <= max_dimension
        && full_height <= max_dimension
        && full_width
            .checked_mul(full_height)
            .is_some_and(|pixels| pixels <= max_pixels)
    {
        return Ok(AdaptiveBlockPlan {
            core_width: i32::MAX,
            core_height: i32::MAX,
            single_pass: true,
        });
    }
    let request_width = requested.z - requested.x;
    let request_height = requested.w - requested.y;
    let max_width = align_up_tile(request_width);
    let max_height = align_up_tile(request_height);
    let mut widths = candidate_core_sizes(max_width);
    let mut heights = candidate_core_sizes(max_height);
    let dimension_core = align_down(
        i32::try_from(max_dimension)
            .unwrap_or(i32::MAX)
            .saturating_sub(halo.saturating_mul(2)),
        ADAPTIVE_TILE_SIZE,
    );
    if dimension_core >= ADAPTIVE_TILE_SIZE {
        widths.push(dimension_core.min(max_width));
        heights.push(dimension_core.min(max_height));
    }

    // Add aspect-aware candidates. The axis-aligned variants are especially
    // useful for panoramic frames and narrow ROIs, where a square core wastes
    // most of the available storage budget on an unnecessary halo dimension.
    for &core_height in &heights.clone() {
        let source_height =
            max_axis_context(full.y, full.w, requested.y, requested.w, core_height, halo);
        if source_height > 0 {
            let source_width = (max_pixels / source_height as usize).min(i32::MAX as usize) as i32;
            let core_width = if source_width >= full_width as i32 {
                max_width
            } else {
                align_down(
                    source_width.saturating_sub(halo.saturating_mul(2)),
                    ADAPTIVE_TILE_SIZE,
                )
            };
            if core_width >= ADAPTIVE_TILE_SIZE {
                widths.push(core_width.min(max_width));
            }
        }
    }
    for &core_width in &widths.clone() {
        let source_width =
            max_axis_context(full.x, full.z, requested.x, requested.z, core_width, halo);
        if source_width > 0 {
            let source_height = (max_pixels / source_width as usize).min(i32::MAX as usize) as i32;
            let core_height = if source_height >= full_height as i32 {
                max_height
            } else {
                align_down(
                    source_height.saturating_sub(halo.saturating_mul(2)),
                    ADAPTIVE_TILE_SIZE,
                )
            };
            if core_height >= ADAPTIVE_TILE_SIZE {
                heights.push(core_height.min(max_height));
            }
        }
    }
    widths.sort_unstable();
    widths.dedup();
    heights.sort_unstable();
    heights.dedup();

    let mut best: Option<(u128, usize, i32, i32)> = None;
    for &core_height in &heights {
        let context_height =
            max_axis_context(full.y, full.w, requested.y, requested.w, core_height, halo);
        for &core_width in &widths {
            let context_width =
                max_axis_context(full.x, full.z, requested.x, requested.z, core_width, halo);
            let source_pixels = (context_width as usize).checked_mul(context_height as usize);
            if context_width as usize > max_dimension
                || context_height as usize > max_dimension
                || source_pixels.is_none_or(|pixels| pixels > max_pixels)
            {
                continue;
            }
            let blocks_x = axis_block_count(requested.x, requested.z, core_width);
            let blocks_y = axis_block_count(requested.y, requested.w, core_height);
            let block_count = blocks_x.saturating_mul(blocks_y);
            let work = source_pixels.unwrap_or(usize::MAX) as u128 * block_count as u128;
            let candidate = (work, block_count, core_width, core_height);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
    }

    if let Some((_, _, core_width, core_height)) = best {
        Ok(AdaptiveBlockPlan {
            core_width,
            core_height,
            single_pass: false,
        })
    } else {
        // Even the smallest aligned core needs more source context than the
        // storage budget permits. This is a GPU resource limit, not an invalid
        // defocus setting.
        Err(Error::SizeLimitExceeded)
    }
}

/// Returns the sorted, unique core sizes to try: one tile, the largest tile-aligned size up to `max_size`, and doubling sizes between them.
fn candidate_core_sizes(max_size: i32) -> Vec<i32> {
    let max_size = align_down(max_size.max(ADAPTIVE_TILE_SIZE), ADAPTIVE_TILE_SIZE);
    let mut sizes = vec![ADAPTIVE_TILE_SIZE, max_size];
    let mut size = ADAPTIVE_TILE_SIZE;
    while size < max_size {
        sizes.push(size);
        size = size.saturating_mul(2);
        if size <= 0 {
            break;
        }
    }
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

/// Rounds `value` (at least 1) up to a whole number of tiles, clamped to the largest tile multiple that fits in `i32`.
fn align_up_tile(value: i32) -> i32 {
    let value = value.max(1) as i64;
    (((value + ADAPTIVE_TILE_SIZE as i64 - 1) / ADAPTIVE_TILE_SIZE as i64)
        * ADAPTIVE_TILE_SIZE as i64)
        .min(i32::MAX as i64 - i32::MAX as i64 % ADAPTIVE_TILE_SIZE as i64) as i32
}

/// Returns the largest context extent along one axis over the blocks that cover `requested_min..requested_max` with cores of `core_size`.
/// Each block adds `halo` on both sides, clipped to `full_min..full_max`.
pub(super) fn max_axis_context(
    full_min: i32,
    full_max: i32,
    requested_min: i32,
    requested_max: i32,
    core_size: i32,
    halo: i32,
) -> i32 {
    let first = align_down(requested_min, core_size);
    let last = align_down(requested_max.saturating_sub(1), core_size);
    let mut starts = [
        first,
        first.saturating_add(core_size),
        last.saturating_sub(core_size),
        last,
    ];
    starts.sort_unstable();
    starts
        .into_iter()
        .filter(|start| *start <= last && start.saturating_add(core_size) > requested_min)
        .map(|start| {
            let core_end = start.saturating_add(core_size).min(requested_max);
            let block_min = start.saturating_sub(halo).max(full_min);
            let block_max = core_end.saturating_add(halo).min(full_max);
            block_max.saturating_sub(block_min).max(0)
        })
        .max()
        .unwrap_or(0)
}

/// Returns how many cores of `core_size` cover `requested_min..requested_max`, counted from the aligned start.
fn axis_block_count(requested_min: i32, requested_max: i32, core_size: i32) -> usize {
    let first = align_down(requested_min, core_size);
    let span = requested_max.saturating_sub(first).max(0) as usize;
    span.div_ceil(core_size as usize)
}
