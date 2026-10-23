//! Input validation, device-limit checks and CPU-side decisions made before the adaptive gather allocates GPU resources.
use super::super::{TileFeatures, WgpuRunner};
use crate::abort::get_aborted;
use crate::convolve_settings::ConvolveSettings;
use crate::error::{Error, Result};
use crate::runners::adaptive::{PreparedLayers, build_gather_integral_nodes};
use ndarray::Array3;

/// Edge length in pixels of a gather tile.
const GATHER_TILE_SIZE: i32 = 16;
/// Floats per pixel of the gather output: RGBA plus one auxiliary channel.
const OUTPUT_FLOATS_PER_PIXEL: u64 = 5;
/// Floats per pixel of the prepared layer input.
const LAYER_FLOATS_PER_PIXEL: u64 = 12;
/// Bytes per float.
const FLOAT_BYTES: u64 = 4;
/// Bytes of one packed integral tree node.
const INTEGRAL_NODE_BYTES: u64 = 64;
/// CoC radius in pixels above which the fused capped-circle specialization can pay off.
const FUSED_MIN_RADIUS_PX: f32 = 2.0;

/// Frame dimensions and the sizes derived from them for one gather.
pub(super) struct GatherGeometry {
    /// Frame width in pixels.
    pub(super) width: u32,
    /// Frame height in pixels.
    pub(super) height: u32,
    /// Number of gather tiles along x.
    pub(super) tiles_x: u32,
    /// Number of gather tiles along y.
    pub(super) tiles_y: u32,
    /// Offset of the frame inside the global tile grid.
    pub(super) tile_origin: glam::UVec2,
    /// Byte size of the gather output buffer.
    pub(super) output_size: u64,
    /// Largest storage buffer binding the device allows.
    pub(super) max_storage: u64,
}

/// Returns true when every prepared pixel has finite CoC and coverage values in range.
fn prepared_pixels_are_valid(prepared: &PreparedLayers) -> bool {
    prepared.pixels.iter().all(|pixel| {
        pixel.near_coc.is_finite()
            && pixel.far_coc.is_finite()
            && pixel.near_valid.is_finite()
            && pixel.far_valid.is_finite()
            && (0.0..=1.0).contains(&pixel.near_valid)
            && (0.0..=1.0).contains(&pixel.far_valid)
    })
}

/// Returns true when `source`, `prepared` and `features` describe the same `width` by `height` frame
/// with `tile_count` tiles and `max_radius_px` is usable.
fn inputs_are_consistent(
    source: &Array3<f32>,
    prepared: &PreparedLayers,
    features: &[TileFeatures],
    max_radius_px: f32,
    tile_count: usize,
) -> bool {
    let (height, width, channels) = source.dim();
    channels == 4
        && width != 0
        && height != 0
        && width == prepared.width
        && height == prepared.height
        && prepared.pixels.len() == width * height
        && prepared.near_edge_distance.len() == width * height
        && prepared_pixels_are_valid(prepared)
        && max_radius_px.is_finite()
        && max_radius_px >= 0.0
        && features.len() == tile_count
}

impl WgpuRunner {
    /// Validates the gather inputs against each other and the device limits and returns the frame geometry.
    ///
    /// # Errors
    /// Returns `InvalidDimensions` for inconsistent inputs, `AdaptiveCancelled` when an abort is requested,
    /// and `SizeLimitExceeded` when the frame does not fit the device.
    pub(super) fn gather_geometry(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
        lens: Option<&ConvolveSettings>,
    ) -> Result<GatherGeometry> {
        let (height, width, _) = source.dim();
        let tile_origin = lens.map_or(glam::UVec2::ZERO, |lens| {
            glam::UVec2::new(
                lens.full_region.x.rem_euclid(GATHER_TILE_SIZE) as u32,
                lens.full_region.y.rem_euclid(GATHER_TILE_SIZE) as u32,
            )
        });
        let tile_size = GATHER_TILE_SIZE as usize;
        let tiles_x = (width + tile_origin.x as usize).div_ceil(tile_size);
        let tiles_y = (height + tile_origin.y as usize).div_ceil(tile_size);
        if !inputs_are_consistent(source, prepared, features, max_radius_px, tiles_x * tiles_y) {
            return Err(Error::InvalidDimensions);
        }
        if get_aborted() {
            return Err(Error::AdaptiveCancelled);
        }
        let width = u32::try_from(width).map_err(|_| Error::SizeLimitExceeded)?;
        let height = u32::try_from(height).map_err(|_| Error::SizeLimitExceeded)?;
        let tiles_x = u32::try_from(tiles_x).map_err(|_| Error::SizeLimitExceeded)?;
        let tiles_y = u32::try_from(tiles_y).map_err(|_| Error::SizeLimitExceeded)?;
        let pixels = u64::from(width) * u64::from(height);
        let output_size = pixels * OUTPUT_FLOATS_PER_PIXEL * FLOAT_BYTES;
        let tile_count = u64::from(tiles_x) * u64::from(tiles_y);
        let search_radii_size = tile_count * 2 * FLOAT_BYTES;
        let limits = self.device.limits();
        let max_storage = limits.max_storage_buffer_binding_size;
        if width > limits.max_texture_dimension_2d
            || height > limits.max_texture_dimension_2d
            || output_size > max_storage
            || pixels * LAYER_FLOATS_PER_PIXEL * FLOAT_BYTES > max_storage
            || search_radii_size > max_storage
        {
            return Err(Error::SizeLimitExceeded);
        }
        Ok(GatherGeometry {
            width,
            height,
            tiles_x,
            tiles_y,
            tile_origin,
            output_size,
            max_storage,
        })
    }
}

/// Builds the packed near and far integral trees for exact Accurate gathers. Other budgets get a placeholder
/// root, because the shader still binds both buffers.
///
/// # Errors
/// Returns `SizeLimitExceeded` when a tree is empty or larger than one storage binding.
pub(super) fn integral_trees(
    prepared: &PreparedLayers,
    use_integrals: bool,
    max_storage: u64,
) -> Result<(Vec<[u32; 16]>, Vec<[u32; 16]>)> {
    if !use_integrals {
        return Ok((vec![[0u32; 16]], vec![[0u32; 16]]));
    }
    let (near, far) = build_gather_integral_nodes(prepared)?;
    let fits = |nodes: &[[u32; 16]]| {
        u64::try_from(nodes.len())
            .ok()
            .and_then(|len| len.checked_mul(INTEGRAL_NODE_BYTES))
            .is_some_and(|size| size != 0 && size <= max_storage)
    };
    if fits(&near) && fits(&far) {
        Ok((near, far))
    } else {
        Err(Error::SizeLimitExceeded)
    }
}

/// Returns true when most valid near pixels sit at the radius cap, where the fused capped-circle
/// specialization is cheaper than the general circle gather.
pub(super) fn capped_pixels_dominate(prepared: &PreparedLayers, max_radius_px: f32) -> bool {
    if max_radius_px <= FUSED_MIN_RADIUS_PX {
        return false;
    }
    let (capped, valid) =
        prepared
            .pixels
            .iter()
            .fold((0usize, 0usize), |(capped, valid), pixel| {
                let near_valid = pixel.near_valid > 0.0;
                #[allow(
                    clippy::float_cmp,
                    reason = "exact equality against the radius limit identifies capped CoC samples"
                )]
                let is_capped = near_valid && pixel.near_coc == max_radius_px;
                (
                    capped + usize::from(is_capped),
                    valid + usize::from(near_valid),
                )
            });
    valid > 0 && capped * 2 >= valid
}
