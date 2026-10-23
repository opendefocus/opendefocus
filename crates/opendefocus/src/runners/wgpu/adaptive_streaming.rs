//! Streamed adaptive gather for large frames: per-chunk GPU resources, the frame-level resource cache,
//! and the compaction pass that reduces finalized output to 16 bytes per pixel.
/// Aperture sampler and bounds resources for streamed gathers.
mod aperture;
#[cfg(test)]
/// Tests for the compaction pass.
mod compact_tests;
/// Dispatch of the streamed gather passes.
mod gather;
/// GPU resource allocation for streamed source chunks.
mod resources;
/// Submission of streamed gather and compaction passes.
mod submission;

use crate::{
    AdaptiveSampleBudget, convolve_settings::ConvolveSettings, datamodel::IVector4,
    renders::resize::MipmapBuffer,
};
use crate::{
    error::{Error, Result},
    runners::{
        adaptive_streaming::{
            ADAPTIVE_OUTPUT_BYTES_PER_PIXEL, AdaptiveSourceChunk, plan_stream_source_cache,
        },
        wgpu::{TileFeatures, WgpuRunner},
    },
};
use ndarray::Array3;

/// Returns true when the source has four channels and every alpha value is at least `1 - 1e-6`.
fn source_is_opaque(source: &Array3<f32>) -> bool {
    if source.dim().2 != 4 {
        return false;
    }
    source
        .index_axis(ndarray::Axis(2), 3)
        .iter()
        .all(|alpha| *alpha >= 1.0 - 1.0e-6)
}

/// Returns true when every chunk's source is opaque, so the opaque coverage restore can run across all cores.
fn stream_sources_are_opaque(chunks: &[AdaptiveSourceChunk]) -> bool {
    chunks.iter().all(|chunk| source_is_opaque(&chunk.source))
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Uniform parameters of one streamed gather pass.
struct StreamParams {
    /// Origin of the source region in full-frame pixels.
    source_origin: [u32; 2],
    /// Origin of the destination region in full-frame pixels.
    destination_origin: [u32; 2],
    /// Size of the source region in pixels.
    source_extent: [u32; 2],
    /// Size of the output region in pixels.
    output_extent: [u32; 2],
    /// Pass phase; selects which accumulation phase this dispatch computes.
    phase: u32,
    /// Explicit padding that keeps the uniform layout aligned.
    _padding0: u32,
    /// Explicit padding that keeps the uniform layout aligned.
    _padding1: u32,
    /// Explicit padding that keeps the uniform layout aligned.
    _padding2: u32,
}

/// Source chunks of a streamed frame with the global tile classification computed from them.
pub(crate) struct StreamSource<'a> {
    /// Prepared source partitions.
    pub(crate) chunks: &'a [AdaptiveSourceChunk],
    /// Classifier features of the global tile grid.
    pub(crate) features: &'a [TileFeatures],
    /// Origin and size of the global tile grid.
    pub(crate) tile_layout: (i32, i32, usize, usize),
}

/// Render parameters of one streamed Accurate gather.
pub(crate) struct StreamGather<'a> {
    /// Maximum CoC radius in pixels.
    pub(crate) max_radius_px: f32,
    /// Sample budget; streamed gathers support only the exact Accurate budget.
    pub(crate) sample_budget: AdaptiveSampleBudget,
    /// Lens settings of the gather.
    pub(crate) lens: &'a ConvolveSettings,
    /// Aperture mip chain, or `None` for the simple disc.
    pub(crate) aperture: Option<&'a MipmapBuffer>,
}

/// Everything one destination shares across its chunk phases: geometry, destination buffers and pipeline.
struct StreamDestination<'a> {
    /// Full-frame region.
    full: &'a IVector4,
    /// Number of global tiles along x.
    tiles_x: usize,
    /// Number of global tiles along y.
    tiles_y: usize,
    /// Offset of the frame inside the global tile grid.
    tile_origin: [u32; 2],
    /// Origin of the destination region in full-frame pixels.
    origin: [u32; 2],
    /// Size of the destination region in pixels.
    extent: [u32; 2],
    /// Render parameters.
    gather: &'a StreamGather<'a>,
    /// Accumulator the phases write into.
    accumulator: &'a wgpu::Buffer,
    /// Global tile classes.
    classes: &'a wgpu::Buffer,
    /// Packed near/far search radii per tile.
    search_radii: &'a wgpu::Buffer,
    /// Sample offset table.
    sample_offsets: &'a wgpu::Buffer,
    /// Aperture resources.
    aperture: &'a StreamAperture,
    /// Streamed convolution pipeline.
    pipeline: &'a wgpu::ComputePipeline,
    /// Workgroup counts covering the destination.
    dispatch: [u32; 2],
}

/// Bind group for one streamed dispatch and the buffers it keeps alive.
struct StreamBindings {
    /// Bind group referencing the dispatch resources.
    bind_group: wgpu::BindGroup,
    /// Buffers kept alive for as long as the bind group is used.
    _resources: Vec<wgpu::Buffer>,
}

/// Per-frame cache of source GPU resources. Chunks chosen by the budget keep their resources for the whole frame; the rest are uploaded when needed.
pub(crate) struct StreamFrameCache {
    /// Whether each chunk's resources are retained for the frame.
    selected: Vec<bool>,
    /// Cached GPU resources per chunk, created on first use.
    resources: Vec<Option<StreamSourceResources>>,
    /// Whether every source chunk is fully opaque.
    source_opaque: bool,
    /// Byte budget for retained resources.
    budget: u64,
    /// Total bytes of the chunks selected for retention.
    selected_bytes: u64,
}

/// GPU textures and buffers of one source chunk.
struct StreamSourceResources {
    /// Source texture, kept alive while its view is used.
    _source_texture: wgpu::Texture,
    /// View of the source texture.
    source_view: wgpu::TextureView,
    /// Prepared base textures and mip levels of the chunk.
    prepared: super::adaptive_gpu::GatherTextures,
    /// Views of the four prepared layer textures.
    prepared_views: [wgpu::TextureView; 4],
    /// Packed near-layer integral tree buffer.
    near_nodes: wgpu::Buffer,
    /// Packed far-layer integral tree buffer.
    far_nodes: wgpu::Buffer,
    /// Buffer of subtree end indices.
    subtree_buffer: wgpu::Buffer,
}

/// Estimates the GPU bytes a chunk needs: its source and four base-level layers at 16 bytes per pixel, its integral nodes at 64 bytes each, its subtree ends at 4 bytes each, and 512 KiB for allocation rounding.
fn stream_source_resource_bytes(chunk: &AdaptiveSourceChunk) -> u64 {
    let pixels = (chunk.prepared.width as u64).saturating_mul(chunk.prepared.height as u64);
    let texture_bytes = pixels.saturating_mul(5 * 16); // source + four base-level layers
    let near_nodes = (chunk.near_integral_nodes.len() as u64).saturating_mul(64);
    let far_nodes = (chunk.far_integral_nodes.len() as u64).saturating_mul(64);
    let subtree_ends = (chunk.subtree_ends.len() as u64).saturating_mul(4);
    let resources = texture_bytes
        .saturating_add(near_nodes)
        .saturating_add(far_nodes)
        .saturating_add(subtree_ends);
    // Reserve one 64 KiB allocation granule per retained GPU resource to
    // conservatively account for texture/buffer allocation rounding.
    resources.saturating_add(8 * 64 * 1024)
}

/// Sampler, view, texture and bounds buffer of a streamed aperture.
struct StreamAperture {
    /// Sampler for the aperture texture.
    sampler: wgpu::Sampler,
    /// View of the aperture texture, when an aperture image is used.
    view: Option<wgpu::TextureView>,
    /// Aperture texture kept alive for its view.
    _texture: Option<wgpu::Texture>,
    /// Aperture bounds buffer.
    bounds: wgpu::Buffer,
}

impl WgpuRunner {
    /// Records a compute pass that compacts the accumulated output (72 floats per pixel) into 4 RGBA floats per pixel.
    /// # Errors
    /// Returns `InvalidDimensions` when `pixels` is zero or a buffer has the wrong size, and `SizeLimitExceeded` when the dispatch exceeds the workgroup limit.
    fn encode_stream_compaction(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        accumulator: &wgpu::Buffer,
        compact: &wgpu::Buffer,
        pixels: u64,
    ) -> Result<()> {
        let accumulator_bytes = pixels
            .checked_mul(ADAPTIVE_OUTPUT_BYTES_PER_PIXEL)
            .ok_or(Error::SizeLimitExceeded)?;
        let compact_bytes = pixels
            .checked_mul(4 * std::mem::size_of::<f32>() as u64)
            .ok_or(Error::SizeLimitExceeded)?;
        if pixels == 0 || accumulator.size() < accumulator_bytes || compact.size() != compact_bytes
        {
            return Err(Error::InvalidDimensions);
        }
        let workgroups =
            u32::try_from(pixels.div_ceil(64)).map_err(|_| Error::SizeLimitExceeded)?;
        if workgroups > self.device.limits().max_compute_workgroups_per_dimension {
            return Err(Error::SizeLimitExceeded);
        }
        let pipeline = self
            .adaptive_stream_compact
            .get_or_init(|| Self::create_adaptive_stream_compact(&self.device));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Adaptive streamed compact output bindings"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: accumulator.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: compact.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Compact adaptive streamed output"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
        Ok(())
    }

    /// Creates a frame cache with the largest budget the device allows.
    pub(crate) fn new_stream_frame_cache(
        &self,
        chunks: &[AdaptiveSourceChunk],
    ) -> StreamFrameCache {
        self.new_stream_frame_cache_with_budget(chunks, u64::MAX)
    }

    /// Creates a frame cache for `chunks`. The budget is capped at 1 GiB and at twice the largest single buffer the device allows; chunks are then selected for retention, largest first.
    pub(crate) fn new_stream_frame_cache_with_budget(
        &self,
        chunks: &[AdaptiveSourceChunk],
        budget: u64,
    ) -> StreamFrameCache {
        /// Upper bound on the streamed frame cache: 1 GiB.
        const MAX_CACHE_BYTES: u64 = 1024 * 1024 * 1024;
        let limits = self.device.limits();
        let max_individual_buffer = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);
        let budget = budget.min(MAX_CACHE_BYTES.min(max_individual_buffer.saturating_mul(2)));
        let resource_bytes: Vec<u64> = chunks.iter().map(stream_source_resource_bytes).collect();
        let selected = plan_stream_source_cache(&resource_bytes, budget);
        let selected_bytes = resource_bytes
            .iter()
            .zip(&selected)
            .filter_map(|(&bytes, &selected)| selected.then_some(bytes))
            .sum();
        StreamFrameCache {
            selected,
            resources: (0..chunks.len()).map(|_| None).collect(),
            source_opaque: stream_sources_are_opaque(chunks),
            budget,
            selected_bytes,
        }
    }
}
