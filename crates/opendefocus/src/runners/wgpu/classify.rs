//! Dispatches the adaptive tile classifier and reads back per-tile features.
use super::validate_adaptive_inputs;
use super::{ClassifierParams, TileFeatures, WgpuRunner};
use crate::error::{Error, Result};
use ndarray::Array3;
use wgpu::util::DeviceExt;
impl WgpuRunner {
    /// Classifies the frame into 16x16 tiles and returns their features, in the order documented on [`TileFeatures`].
    /// The inputs are linear premultiplied RGBA32F, signed CoC in full-resolution pixels, the frame origin for offset regions and the maximum radius in pixels.
    /// # Errors
    /// Returns `AdaptiveClassifierUnavailable` when the classifier pipeline was not created, or a GPU error when dispatch or readback fails.
    pub async fn classify_tiles(
        &self,
        rgba: &Array3<f32>,
        coc: &ndarray::Array2<f32>,
        origin: [i32; 2],
        max_radius_px: f32,
    ) -> Result<Vec<TileFeatures>> {
        /// Tile edge length in pixels used by the classifier dispatch.
        const TILE: u32 = 16;
        let classifier = self
            .adaptive_classifier
            .as_ref()
            .ok_or(Error::AdaptiveClassifierUnavailable)?;
        let propagation = self
            .adaptive_propagation
            .as_ref()
            .ok_or(Error::AdaptiveClassifierUnavailable)?;
        let (height, width, channels) = rgba.dim();
        if channels != 4 || coc.dim() != (height, width) || width == 0 || height == 0 {
            return Err(Error::InvalidDimensions);
        }
        validate_adaptive_inputs(None, coc, max_radius_px)?;
        let tiles_x = (width as u32 + (origin[0].rem_euclid(TILE as i32) as u32))
            .div_ceil(TILE)
            .max(1);
        let tiles_y = (height as u32 + (origin[1].rem_euclid(TILE as i32) as u32))
            .div_ceil(TILE)
            .max(1);
        let params = ClassifierParams {
            width: width as u32,
            height: height as u32,
            tiles_x,
            tiles_y,
            origin_x: origin[0],
            origin_y: origin[1],
            max_search_tiles: (max_radius_px.max(0.0) / TILE as f32).ceil() as u32 + 1,
            padding: 0,
        };
        let input_color = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive classifier color"),
                contents: bytemuck::cast_slice(rgba.as_slice().ok_or(Error::NonContiguousArray)?),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let input_coc = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive classifier CoC"),
                contents: bytemuck::cast_slice(coc.as_slice().ok_or(Error::NonContiguousArray)?),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let params_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive classifier params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let output_size = (tiles_x as u64) * (tiles_y as u64) * 11 * 4;
        let raw = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive classifier raw features"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive propagated tile features"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive classifier readback"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Adaptive classifier bindings"),
            layout: &classifier.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: input_color.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: input_coc.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: raw.as_entire_binding(),
                },
            ],
        });
        let propagation_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Adaptive propagation bindings"),
            layout: &propagation.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: raw.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive classifier encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Adaptive classifier pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(classifier);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(tiles_x, tiles_y, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Adaptive blur reach propagation"),
                timestamp_writes: None,
            });
            pass.set_pipeline(propagation);
            pass.set_bind_group(1, &propagation_bind_group, &[]);
            pass.dispatch_workgroups(tiles_x, tiles_y, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, output_size);
        let (sender, receiver) = futures_channel::oneshot::channel();
        encoder.map_buffer_on_submit(&staging, wgpu::MapMode::Read, .., |result| {
            crate::runners::wgpu::complete_readback(sender, result);
        });
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()?;
        receiver.await??;
        let view = staging.slice(..).get_mapped_range()?;
        let features = bytemuck::cast_slice::<u8, TileFeatures>(&view).to_vec();
        drop(view);
        staging.unmap();
        Ok(features)
    }
}
