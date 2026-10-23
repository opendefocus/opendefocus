//! Creates the per-chunk GPU source resources and the bind groups that reference them for a streamed dispatch.
use crate::{
    error::{Error, Result},
    runners::{
        adaptive_streaming::AdaptiveSourceChunk,
        wgpu::{GatherParams, WgpuRunner, buffer_entry, disc_shift, view_entry},
    },
};
use wgpu::util::DeviceExt;

use super::{StreamBindings, StreamDestination, StreamParams, StreamSourceResources};

impl WgpuRunner {
    /// Uploads the source texture, prepared layer textures and integral node buffers of one chunk.
    /// # Errors
    /// Returns `InvalidDimensions` when the chunk size is zero or does not match its source, and `SizeLimitExceeded` when the integral node buffers exceed the storage limit.
    pub(super) fn make_stream_source_resources(
        &self,
        chunk: &AdaptiveSourceChunk,
    ) -> Result<StreamSourceResources> {
        let width = u32::try_from(chunk.prepared.width).map_err(|_| Error::SizeLimitExceeded)?;
        let height = u32::try_from(chunk.prepared.height).map_err(|_| Error::SizeLimitExceeded)?;
        if width == 0 || height == 0 || chunk.source.dim() != (height as usize, width as usize, 4) {
            return Err(Error::InvalidDimensions);
        }
        let prepared = self.resident_adaptive_base_textures(&chunk.prepared)?;
        let prepared_views = std::array::from_fn(|index| {
            prepared.textures[index].create_view(&wgpu::TextureViewDescriptor::default())
        });
        let source_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Adaptive streamed source chunk"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            source_texture.as_image_copy(),
            bytemuck::cast_slice(chunk.source.as_slice().ok_or(Error::NonContiguousArray)?),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 16),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let source_view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let max_storage = self.device.limits().max_storage_buffer_binding_size;
        let node_bytes = |nodes: &[[u32; 16]]| (nodes.len() as u64).saturating_mul(64);
        if node_bytes(&chunk.near_integral_nodes) > max_storage
            || node_bytes(&chunk.far_integral_nodes) > max_storage
            || (chunk.subtree_ends.len() as u64).saturating_mul(4) > max_storage
        {
            return Err(Error::SizeLimitExceeded);
        }
        let near_nodes = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive streamed near integral chunk"),
                contents: bytemuck::cast_slice(&chunk.near_integral_nodes),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let far_nodes = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive streamed far integral chunk"),
                contents: bytemuck::cast_slice(&chunk.far_integral_nodes),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let subtree_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive streamed integral subtree ends"),
                contents: bytemuck::cast_slice(&chunk.subtree_ends),
                usage: wgpu::BufferUsages::STORAGE,
            });
        Ok(StreamSourceResources {
            _source_texture: source_texture,
            source_view,
            prepared,
            prepared_views,
            near_nodes,
            far_nodes,
            subtree_buffer,
        })
    }

    /// Creates the bind group for one streamed dispatch of a chunk, referencing its resources and the destination buffers.
    /// # Errors
    /// Returns an error when the bindings cannot be created.
    pub(super) fn make_stream_chunk_bindings(
        &self,
        source: &StreamSourceResources,
        chunk: &AdaptiveSourceChunk,
        destination: &StreamDestination<'_>,
        phase: u32,
        source_opaque: bool,
    ) -> Result<StreamBindings> {
        let width = u32::try_from(chunk.prepared.width).map_err(|_| Error::SizeLimitExceeded)?;
        let height = u32::try_from(chunk.prepared.height).map_err(|_| Error::SizeLimitExceeded)?;
        if width == 0 || height == 0 || chunk.source.dim() != (height as usize, width as usize, 4) {
            return Err(Error::InvalidDimensions);
        }
        let prepared = &source.prepared;
        let views = &source.prepared_views;
        let source_view = &source.source_view;
        let source_origin = [
            u32::try_from(chunk.region.core.x - destination.full.x)
                .map_err(|_| Error::InvalidDimensions)?,
            u32::try_from(chunk.region.core.y - destination.full.y)
                .map_err(|_| Error::InvalidDimensions)?,
        ];
        let lens = destination.gather.lens;
        let stream_params = self.uniform_buffer(
            "Adaptive streamed gather coordinates",
            bytemuck::bytes_of(&StreamParams {
                source_origin,
                destination_origin: destination.origin,
                source_extent: [width, height],
                output_extent: destination.extent,
                phase,
                _padding0: 0,
                _padding1: 0,
                _padding2: 0,
            }),
        );
        let disc_shift = disc_shift(Some(lens));
        let params = self.uniform_buffer(
            "Adaptive streamed gather parameters",
            bytemuck::bytes_of(&GatherParams {
                frame: [
                    width,
                    height,
                    destination.tiles_x as u32,
                    destination.tiles_y as u32,
                ],
                tile_origin: destination.tile_origin,
                source_opaque: u32::from(source_opaque),
                _tile_padding: 0,
                samples: destination.gather.sample_budget.kernel_params(),
                radius: [
                    destination.gather.max_radius_px,
                    prepared.max_mip,
                    disc_shift.x,
                    disc_shift.y,
                ],
            }),
        );
        let lens_buffer = self.uniform_buffer(
            "Adaptive streamed lens parameters",
            bytemuck::bytes_of(lens),
        );
        let entries = vec![
            buffer_entry(0, &params),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&destination.aperture.sampler),
            },
            buffer_entry(2, destination.accumulator),
            view_entry(3, source_view),
            view_entry(4, &views[0]),
            view_entry(5, &views[1]),
            view_entry(6, &views[2]),
            view_entry(7, &views[3]),
            buffer_entry(8, destination.classes),
            buffer_entry(9, destination.sample_offsets),
            buffer_entry(10, destination.search_radii),
            buffer_entry(11, &lens_buffer),
            buffer_entry(12, &source.near_nodes),
            buffer_entry(13, &source.far_nodes),
            view_entry(
                14,
                destination.aperture.view.as_ref().unwrap_or(source_view),
            ),
            buffer_entry(15, &destination.aperture.bounds),
            buffer_entry(16, &source.subtree_buffer),
            buffer_entry(17, &stream_params),
        ];
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Adaptive streamed Accurate bindings"),
            layout: &self
                .adaptive_convolution_stream_high
                .get_or_init(|| Self::create_adaptive_streaming_convolution(&self.device))
                .get_bind_group_layout(0),
            entries: &entries,
        });
        Ok(StreamBindings {
            bind_group,
            _resources: vec![params, lens_buffer, stream_params],
        })
    }
}
