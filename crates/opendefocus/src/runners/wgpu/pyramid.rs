//! Dispatches the layer pyramid reduction and packing passes on the WGPU runner.
use super::{PyramidParams, WgpuRunner};
use crate::abort::get_aborted;
use crate::error::{Error, Result};
use crate::runners::adaptive::{AdaptiveLayerPixel, AdaptiveSourcePyramid, PreparedLayers};
use wgpu::util::DeviceExt;
impl WgpuRunner {
    /// Build a GPU-reduced source pyramid from prepared layers. Near and far
    /// colors, `CoC` and validity are reduced independently, so opposite signed
    /// layers never bleed together. RGBA is averaged over each in-bounds 2x2
    /// footprint to retain area energy; odd image edges use their actual child
    /// count, and empty validity neighborhoods keep zero `CoC`.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid, or when a GPU resource cannot be created or read back.
    pub async fn build_adaptive_pyramid(
        &self,
        prepared: &PreparedLayers,
    ) -> Result<AdaptiveSourcePyramid> {
        let pipeline = self
            .adaptive_pyramid
            .as_ref()
            .ok_or(Error::AdaptiveClassifierUnavailable)?;
        if prepared.width == 0
            || prepared.height == 0
            || prepared.pixels.len() != prepared.width * prepared.height
        {
            return Err(Error::InvalidDimensions);
        }
        let mut levels = vec![crate::runners::adaptive::AdaptivePyramidLevel {
            width: prepared.width,
            height: prepared.height,
            pixels: prepared.pixels.clone(),
        }];
        let mut source_width =
            u32::try_from(prepared.width).map_err(|_| Error::SizeLimitExceeded)?;
        let mut source_height =
            u32::try_from(prepared.height).map_err(|_| Error::SizeLimitExceeded)?;
        let source_size = source_width as u64 * source_height as u64 * 12 * 4;
        let max_storage = self.device.limits().max_storage_buffer_binding_size;
        if source_size > max_storage {
            return Err(Error::SizeLimitExceeded);
        }
        let source_base = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive pyramid level zero"),
                contents: bytemuck::cast_slice(&prepared.pixels),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let mut gpu_levels = vec![source_base];
        let mut staging_levels = Vec::new();
        let mut params_levels = Vec::new();
        let mut bind_groups = Vec::new();
        let mut dimensions = Vec::new();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive near/far pyramid encoder"),
            });
        while source_width > 1 || source_height > 1 {
            if get_aborted() {
                return Err(Error::AdaptiveCancelled);
            }
            let target_width = (source_width / 2).max(1);
            let target_height = (source_height / 2).max(1);
            let output_size = target_width as u64 * target_height as u64 * 12 * 4;
            if output_size > max_storage {
                return Err(Error::SizeLimitExceeded);
            }
            let target = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Adaptive pyramid reduced level"),
                size: output_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Adaptive pyramid level readback"),
                size: output_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let params = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Adaptive pyramid reduction parameters"),
                    contents: bytemuck::bytes_of(&PyramidParams {
                        frame: [source_width, source_height, target_width, target_height],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let source_buffer = gpu_levels.last().ok_or(Error::InvalidDimensions)?;
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Adaptive pyramid reduction bindings"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: source_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: target.as_entire_binding(),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Adaptive near/far pyramid reduction"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(target_width.div_ceil(8), target_height.div_ceil(8), 1);
            }
            encoder.copy_buffer_to_buffer(&target, 0, &staging, 0, output_size);
            gpu_levels.push(target);
            staging_levels.push(staging);
            params_levels.push(params);
            bind_groups.push(bind_group);
            dimensions.push((target_width, target_height));
            source_width = target_width;
            source_height = target_height;
        }
        let (senders, receivers): (Vec<_>, Vec<_>) = (0..staging_levels.len())
            .map(|_| futures_channel::oneshot::channel())
            .unzip();
        for (staging, sender) in staging_levels.iter().zip(senders) {
            encoder.map_buffer_on_submit(staging, wgpu::MapMode::Read, .., move |result| {
                crate::runners::wgpu::complete_readback(sender, result);
            });
        }
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()?;
        for ((receiver, staging), (width, height)) in receivers
            .into_iter()
            .zip(staging_levels.iter())
            .zip(dimensions.iter())
        {
            receiver.await??;
            let view = staging.slice(..).get_mapped_range()?;
            let pixels = bytemuck::cast_slice::<u8, AdaptiveLayerPixel>(&view).to_vec();
            drop(view);
            staging.unmap();
            levels.push(crate::runners::adaptive::AdaptivePyramidLevel {
                width: *width as usize,
                height: *height as usize,
                pixels,
            });
        }
        drop((gpu_levels, params_levels, bind_groups));
        Ok(AdaptiveSourcePyramid { levels })
    }
}
