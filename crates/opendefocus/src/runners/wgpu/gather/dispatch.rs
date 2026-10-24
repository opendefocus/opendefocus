//! Binds the adaptive gather resources, selects its pipeline and runs the pass with readback.
use super::super::{WgpuRunner, buffer_entry, view_entry};
use crate::error::Result;
use ndarray::Array3;

/// Every resource the gather shader binds, borrowed for the lifetime of one dispatch.
pub(super) struct GatherBindings<'a> {
    /// Uniform with frame, tile and sample parameters (binding 0).
    pub(super) params: &'a wgpu::Buffer,
    /// Clamped linear sampler (binding 1).
    pub(super) sampler: &'a wgpu::Sampler,
    /// Output storage buffer (binding 2).
    pub(super) output: &'a wgpu::Buffer,
    /// Source image view (binding 3, and 14 when there is no aperture).
    pub(super) source_view: &'a wgpu::TextureView,
    /// Near layer colour view (binding 4).
    pub(super) near_color_view: &'a wgpu::TextureView,
    /// Far layer colour view (binding 5).
    pub(super) far_color_view: &'a wgpu::TextureView,
    /// Near layer metadata view (binding 6).
    pub(super) near_metadata_view: &'a wgpu::TextureView,
    /// Far layer metadata view (binding 7).
    pub(super) far_metadata_view: &'a wgpu::TextureView,
    /// Tile classes (binding 8).
    pub(super) classes: &'a wgpu::Buffer,
    /// Sample offset table (binding 9).
    pub(super) sample_offsets: &'a wgpu::Buffer,
    /// Packed near/far search radii per tile (binding 10).
    pub(super) search_radii: &'a wgpu::Buffer,
    /// Lens uniform (binding 11).
    pub(super) lens: &'a wgpu::Buffer,
    /// Near and far integral hierarchies (bindings 12 and 13).
    pub(super) integral_nodes: [&'a wgpu::Buffer; 2],
    /// Aperture mip chain view, or the source view when there is no aperture (binding 14).
    pub(super) aperture_view: &'a wgpu::TextureView,
    /// Aperture integral bounds, or the unused circle bounds (binding 15).
    pub(super) aperture_bounds: &'a wgpu::Buffer,
    /// Exclusive subtree end of each integral node (binding 16).
    pub(super) subtree_ends: &'a wgpu::Buffer,
}

impl<'a> GatherBindings<'a> {
    /// Returns the bind group entries in binding order.
    fn entries(&self) -> Vec<wgpu::BindGroupEntry<'a>> {
        vec![
            buffer_entry(0, self.params),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(self.sampler),
            },
            buffer_entry(2, self.output),
            view_entry(3, self.source_view),
            view_entry(4, self.near_color_view),
            view_entry(5, self.far_color_view),
            view_entry(6, self.near_metadata_view),
            view_entry(7, self.far_metadata_view),
            buffer_entry(8, self.classes),
            buffer_entry(10, self.search_radii),
            buffer_entry(9, self.sample_offsets),
            buffer_entry(11, self.lens),
            view_entry(14, self.aperture_view),
            buffer_entry(12, self.integral_nodes[0]),
            buffer_entry(13, self.integral_nodes[1]),
            buffer_entry(15, self.aperture_bounds),
            buffer_entry(16, self.subtree_ends),
        ]
    }
}

/// The pipelines of one gather variant.
#[derive(Debug, Clone)]
pub(in crate::runners::wgpu) struct GatherPipelines {
    /// The gather over every requested pixel.
    pub(in crate::runners::wgpu) gather: wgpu::ComputePipeline,
    /// The exact far occlusion pass, run before the gather: it leaves in each pixel's output slots the far sources
    /// that a nearer far surface hides there, which the gather subtracts. It is a separate pipeline so that the
    /// exact gather keeps its register budget; Fast has none.
    pub(in crate::runners::wgpu) far_occlusion: Option<wgpu::ComputePipeline>,
}

impl WgpuRunner {
    /// Returns the convolution pipelines for the gather variant, creating them on first use.
    /// Exact Accurate gathers use the integral traversal, with specialisations for the plain circle.
    pub(super) fn gather_pipeline(
        &self,
        use_integrals: bool,
        plain_circle: bool,
        fused_capped_circle: bool,
    ) -> &GatherPipelines {
        let (cell, high, plain, fused) = if use_integrals && plain_circle {
            if fused_capped_circle {
                (&self.adaptive_convolution_circle_fused, true, true, true)
            } else {
                (&self.adaptive_convolution_circle, true, true, false)
            }
        } else if use_integrals {
            (&self.adaptive_convolution_high, true, false, false)
        } else {
            (&self.adaptive_convolution_sampled, false, false, false)
        };
        cell.get_or_init(|| Self::create_gather_pipelines(&self.device, high, plain, fused))
    }

    /// Dispatches the gather over a `width` by `height` frame and reads the five-channel result back.
    ///
    /// # Errors
    /// Returns an error when the GPU work fails or the readback cannot be mapped.
    pub(super) async fn run_gather(
        &self,
        pipelines: &GatherPipelines,
        bindings: &GatherBindings<'_>,
        (width, height): (u32, u32),
        output_size: u64,
    ) -> Result<Array3<f32>> {
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive gather readback"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive circular gather encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Adaptive smooth/detail gathers"),
                timestamp_writes: None,
            });
            // The gather reads what the far occlusion pass leaves in its output slots, so that pass runs first.
            for pipeline in pipelines.far_occlusion.iter().chain([&pipelines.gather]) {
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Adaptive gather bindings"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &bindings.entries(),
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
            }
        }
        encoder.copy_buffer_to_buffer(bindings.output, 0, &staging, 0, output_size);
        let (sender, receiver) = futures_channel::oneshot::channel();
        encoder.map_buffer_on_submit(&staging, wgpu::MapMode::Read, .., |result| {
            crate::runners::wgpu::complete_readback(sender, result);
        });
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()?;
        receiver.await??;
        let view = staging.slice(..).get_mapped_range()?;
        let output = Array3::from_shape_vec(
            (height as usize, width as usize, 5),
            bytemuck::cast_slice::<u8, f32>(&view).to_vec(),
        )?;
        drop(view);
        staging.unmap();
        Ok(output)
    }
}
