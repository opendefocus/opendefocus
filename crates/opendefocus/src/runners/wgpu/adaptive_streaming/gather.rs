//! Accurate gather over streamed source chunks: runs each chunk's contribution to one destination and returns the composed output.
use crate::datamodel::IVector4;
use crate::{
    AdaptiveSampleBudget,
    error::{Error, Result},
    runners::{
        adaptive::{adaptive_gather_tile_class, adaptive_near_far_search_radii},
        adaptive_streaming::ADAPTIVE_OUTPUT_BYTES_PER_PIXEL,
        wgpu::WgpuRunner,
    },
};
use ndarray::Array3;

use super::{StreamDestination, StreamFrameCache, StreamGather, StreamSource};

/// Edge length in pixels of a global gather tile.
const GATHER_TILE_SIZE: i32 = 16;
/// Threads per workgroup edge of the streamed gather.
const WORKGROUP_EDGE: u32 = 8;
/// Bytes of one compacted output pixel: four floats.
const COMPACT_BYTES_PER_PIXEL: u64 = 4 * std::mem::size_of::<f32>() as u64;
/// Phase that initializes every centre sample and global tile radius.
const PHASE_INITIALIZE: u32 = 2;
/// Phase that gathers every front source and finds the global support radius.
const PHASE_GATHER_FRONT: u32 = 0;
/// Phase that integrates the potential image.
const PHASE_INTEGRATE: u32 = 1;
/// Phase that finalizes the accumulated output.
const PHASE_FINALIZE: u32 = 3;

impl WgpuRunner {
    /// Gathers one destination from the streamed source chunks with the Accurate tier. Each chunk contributes through the cache, and the output is returned for `output_region`.
    /// # Errors
    /// Returns `InvalidDimensions` when the destination index or a region is invalid, `SizeLimitExceeded` when the output exceeds device limits, or a GPU error when a resource cannot be created.
    pub(crate) async fn gather_adaptive_streamed_high(
        &self,
        source: &StreamSource<'_>,
        cache: &mut StreamFrameCache,
        destination_index: usize,
        full: &IVector4,
        output_region: &IVector4,
        gather: &StreamGather<'_>,
    ) -> Result<Array3<f32>> {
        let (output_width, output_height) =
            self.check_stream_request(source, cache, destination_index, output_region, gather)?;
        let output_pixels = (output_width as u64) * (output_height as u64);
        let accumulator_bytes = output_pixels * ADAPTIVE_OUTPUT_BYTES_PER_PIXEL;
        let (_, _, tiles_x, tiles_y) = source.tile_layout;
        let classes: Vec<u32> = source
            .features
            .iter()
            .map(adaptive_gather_tile_class)
            .collect();
        let search_radii =
            adaptive_near_far_search_radii(source.features, tiles_x, tiles_y, gather.max_radius_px);
        let classes_buffer = self.storage_buffer(
            "Adaptive streamed global tile classes",
            bytemuck::cast_slice(&classes),
        );
        let search_radii_buffer = self.storage_buffer(
            "Adaptive streamed global search radii",
            bytemuck::cast_slice(&search_radii),
        );
        let accumulator = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive streamed exact accumulators"),
            size: accumulator_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.clear_stream_accumulator(&accumulator)?;
        let aperture = self.make_stream_aperture(gather.aperture, gather.lens)?;
        let output_extent = [output_width as u32, output_height as u32];
        let destination = StreamDestination {
            full,
            tiles_x,
            tiles_y,
            tile_origin: [
                full.x.rem_euclid(GATHER_TILE_SIZE) as u32,
                full.y.rem_euclid(GATHER_TILE_SIZE) as u32,
            ],
            origin: [
                u32::try_from(output_region.x - full.x).map_err(|_| Error::InvalidDimensions)?,
                u32::try_from(output_region.y - full.y).map_err(|_| Error::InvalidDimensions)?,
            ],
            extent: output_extent,
            gather,
            accumulator: &accumulator,
            classes: &classes_buffer,
            search_radii: &search_radii_buffer,
            sample_offsets: self
                .adaptive_sample_offsets
                .as_ref()
                .ok_or(Error::AdaptiveClassifierUnavailable)?,
            aperture: &aperture,
            pipeline: self
                .adaptive_convolution_stream_high
                .get_or_init(|| Self::create_adaptive_streaming_convolution(&self.device)),
            dispatch: [
                output_extent[0].div_ceil(WORKGROUP_EDGE),
                output_extent[1].div_ceil(WORKGROUP_EDGE),
            ],
        };
        self.run_stream_phases(source, cache, destination_index, &destination)?;
        self.read_stream_output(&accumulator, output_pixels, (output_height, output_width))
            .await
    }

    /// Validates the request against the cache, the destination chunk and the device limits.
    /// Returns the output width and height.
    ///
    /// # Errors
    /// Returns `InvalidDimensions` for an inconsistent request and `SizeLimitExceeded` when the output exceeds the device.
    fn check_stream_request(
        &self,
        source: &StreamSource<'_>,
        cache: &StreamFrameCache,
        destination_index: usize,
        output_region: &IVector4,
        gather: &StreamGather<'_>,
    ) -> Result<(usize, usize)> {
        let destination = source
            .chunks
            .get(destination_index)
            .ok_or(Error::InvalidDimensions)?;
        let output_width = usize::try_from(output_region.z - output_region.x)
            .map_err(|_| Error::SizeLimitExceeded)?;
        let output_height = usize::try_from(output_region.w - output_region.y)
            .map_err(|_| Error::SizeLimitExceeded)?;
        let (_, _, tiles_x, tiles_y) = source.tile_layout;
        let core = destination.region.core;
        if source.chunks.is_empty()
            || cache.selected.len() != source.chunks.len()
            || cache.resources.len() != source.chunks.len()
            || cache.selected_bytes > cache.budget
            || output_width == 0
            || output_height == 0
            || output_region.x < core.x
            || output_region.y < core.y
            || output_region.z > core.z
            || output_region.w > core.w
            || source.features.len() != tiles_x * tiles_y
            || gather.sample_budget
                != AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate)
        {
            return Err(Error::InvalidDimensions);
        }
        let accumulator_bytes = (output_width as u64)
            .checked_mul(output_height as u64)
            .and_then(|pixels| pixels.checked_mul(ADAPTIVE_OUTPUT_BYTES_PER_PIXEL))
            .ok_or(Error::SizeLimitExceeded)?;
        let limits = self.device.limits();
        if accumulator_bytes > limits.max_storage_buffer_binding_size
            || output_width > limits.max_texture_dimension_2d as usize
            || output_height > limits.max_texture_dimension_2d as usize
        {
            return Err(Error::SizeLimitExceeded);
        }
        Ok((output_width, output_height))
    }

    /// Zeroes the accumulator and waits for the clear to finish.
    ///
    /// # Errors
    /// Returns an error when waiting for the GPU fails.
    fn clear_stream_accumulator(&self, accumulator: &wgpu::Buffer) -> Result<()> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clear adaptive streamed accumulators"),
            });
        encoder.clear_buffer(accumulator, 0, None);
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()
    }

    /// Runs the four phases of the streamed gather. The first and last phases run on the destination chunk only;
    /// the two middle phases visit every source chunk.
    ///
    /// # Errors
    /// Returns an error when a phase cannot be recorded or submitted.
    fn run_stream_phases(
        &self,
        source: &StreamSource<'_>,
        cache: &mut StreamFrameCache,
        destination_index: usize,
        destination: &StreamDestination<'_>,
    ) -> Result<()> {
        let destination_chunk = source
            .chunks
            .get(destination_index)
            .ok_or(Error::InvalidDimensions)?;
        // All center samples and global tile radii are initialized before
        // any source contribution reads them.
        self.submit_stream_chunk_phase(
            cache,
            destination_index,
            destination_chunk,
            destination,
            PHASE_INITIALIZE,
        )?;
        // Phase 0 gathers every front source and discovers the true global
        // support radius before phase 1 integrates the potential image.
        for phase in [PHASE_GATHER_FRONT, PHASE_INTEGRATE] {
            for (chunk_index, chunk) in source.chunks.iter().enumerate() {
                self.submit_stream_chunk_phase(cache, chunk_index, chunk, destination, phase)?;
            }
        }
        self.submit_stream_chunk_phase(
            cache,
            destination_index,
            destination_chunk,
            destination,
            PHASE_FINALIZE,
        )
    }

    /// Compacts the accumulator to four floats per pixel and reads the result back as a `height` by `width` RGBA array.
    ///
    /// # Errors
    /// Returns an error when compaction, the readback or the array construction fails.
    async fn read_stream_output(
        &self,
        accumulator: &wgpu::Buffer,
        output_pixels: u64,
        (height, width): (usize, usize),
    ) -> Result<Array3<f32>> {
        let compact_bytes = output_pixels
            .checked_mul(COMPACT_BYTES_PER_PIXEL)
            .ok_or(Error::SizeLimitExceeded)?;
        let compact = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive streamed compact output"),
            size: compact_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive streamed exact readback"),
            size: compact_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive streamed Accurate compact readback"),
            });
        self.encode_stream_compaction(&mut encoder, accumulator, &compact, output_pixels)?;
        encoder.copy_buffer_to_buffer(&compact, 0, &staging, 0, compact_bytes);
        let (sender, receiver) = futures_channel::oneshot::channel();
        encoder.map_buffer_on_submit(&staging, wgpu::MapMode::Read, .., |result| {
            crate::runners::wgpu::complete_readback(sender, result);
        });
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()?;
        receiver.await??;
        let view = staging.slice(..).get_mapped_range()?;
        let values: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&view).to_vec();
        drop(view);
        staging.unmap();
        Ok(Array3::from_shape_vec((height, width, 4), values)?)
    }
}
