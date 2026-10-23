//! Dispatches the focal and bokeh preview pass on the WGPU runner.
use super::{PreviewParams, WgpuRunner};
use crate::error::{Error, Result};
use crate::runners::wgpu::{buffer_entry, dispatch_pass};
use ndarray::{ArrayView2, ArrayViewMut3};

/// Largest preview chunk uploaded at once: 64 MiB.
const MAX_CHUNK_BYTES: u64 = 64 * 1024 * 1024;
/// Floats per RGBA pixel.
const RGBA_CHANNELS: usize = 4;
/// Threads per workgroup edge of the preview shader.
const WORKGROUP_EDGE: u32 = 8;

/// Builds the uniform parameters of the preview from the bokeh settings, for a `width` by `height` image.
fn preview_params(
    settings: &crate::datamodel::Settings,
    (width, height): (u32, u32),
    is_bokeh: bool,
) -> PreviewParams {
    let bokeh = &settings.bokeh;
    PreviewParams {
        dimensions: [
            width,
            height,
            u32::from(!is_bokeh),
            u32::from(!settings.defocus.show_image),
        ],
        chunk: [0, 0, width, 0],
        shape: [
            bokeh.radius,
            bokeh.aspect_ratio,
            bokeh.angle,
            bokeh.curvature,
        ],
        ring: [
            bokeh.ring_size,
            bokeh.outer_blur,
            bokeh.inner_blur,
            bokeh.ring_color,
        ],
        color: [
            bokeh.inner_color,
            bokeh.blades as f32,
            if bokeh.filter_type() == bokeh_creator::FilterType::Disc {
                1.0
            } else {
                0.0
            },
            0.0,
        ],
    }
}

/// Reads rows `rows` of `image` as RGBA, filling missing channels with zero and alpha with one,
/// together with the matching depth values.
fn chunk_inputs(
    image: &ArrayViewMut3<'_, f32>,
    depth: &ArrayView2<'_, f32>,
    rows: std::ops::Range<usize>,
) -> (Vec<f32>, Vec<f32>) {
    let (_, width, channels) = image.dim();
    let mut source = Vec::with_capacity(width * rows.len() * RGBA_CHANNELS);
    let mut depth_values = Vec::with_capacity(width * rows.len());
    for y in rows {
        for x in 0..width {
            for channel in 0..RGBA_CHANNELS {
                source.push(if channel < channels {
                    image[[y, x, channel]]
                } else if channel == 3 {
                    1.0
                } else {
                    0.0
                });
            }
            depth_values.push(depth[[y, x]]);
        }
    }
    (source, depth_values)
}

/// Copies the rendered RGBA chunk starting at row `y_start` into the channels `image` has.
fn write_chunk(image: &mut ArrayViewMut3<'_, f32>, rendered: &[f32], y_start: usize) {
    let (height, width, channels) = image.dim();
    let rows = rendered.len() / (width * RGBA_CHANNELS);
    for y in y_start..(y_start + rows).min(height) {
        for x in 0..width {
            let pixel = ((y - y_start) * width + x) * RGBA_CHANNELS;
            for channel in 0..channels {
                image[[y, x, channel]] = rendered[pixel + channel];
            }
        }
    }
}

impl WgpuRunner {
    /// Renders the focal or bokeh preview into `image`. `depth` is used only for focal plane previews.
    /// # Errors
    /// Returns `InvalidDimensions` when the image is empty or has more than four channels, and a GPU error when the pass fails.
    pub(crate) async fn render_preview(
        &self,
        settings: &crate::datamodel::Settings,
        mut image: ArrayViewMut3<'_, f32>,
        depth: Option<ArrayView2<'_, f32>>,
    ) -> Result<()> {
        let (height, width, channels) = image.dim();
        if width == 0 || height == 0 || channels == 0 || channels > RGBA_CHANNELS {
            return Err(Error::InvalidDimensions);
        }
        let width_u32 = u32::try_from(width).map_err(|_| Error::SizeLimitExceeded)?;
        let height_u32 = u32::try_from(height).map_err(|_| Error::SizeLimitExceeded)?;
        let is_bokeh = crate::is_bokeh_filter_preview(settings);
        if !is_bokeh
            && depth
                .as_ref()
                .is_none_or(|depth| depth.dim() != (height, width))
        {
            return Err(Error::InvalidDimensions);
        }
        let mut params = preview_params(settings, (width_u32, height_u32), is_bokeh);
        let params_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Preview parameters"),
            size: std::mem::size_of::<PreviewParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let max_chunk_bytes = self
            .device
            .limits()
            .max_storage_buffer_binding_size
            .min(MAX_CHUNK_BYTES);
        let bytes_per_row =
            u64::from(width_u32) * (RGBA_CHANNELS * std::mem::size_of::<f32>()) as u64;
        if bytes_per_row > max_chunk_bytes {
            return Err(Error::SizeLimitExceeded);
        }
        let rows_per_chunk = (max_chunk_bytes / bytes_per_row)
            .max(1)
            .min(u64::from(height_u32)) as usize;
        for y_start in (0..height).step_by(rows_per_chunk) {
            let chunk_height = rows_per_chunk.min(height - y_start);
            let chunk_height_u32 =
                u32::try_from(chunk_height).map_err(|_| Error::SizeLimitExceeded)?;
            params.chunk = [0, y_start as u32, width_u32, chunk_height_u32];
            let (source, depth_values) = if is_bokeh {
                (vec![0.0; RGBA_CHANNELS], vec![0.0])
            } else {
                let depth = depth.as_ref().ok_or(Error::DepthNotFound)?;
                chunk_inputs(&image, depth, y_start..y_start + chunk_height)
            };
            self.queue
                .write_buffer(&params_buffer, 0, bytemuck::bytes_of(&params));
            let rendered = self
                .run_preview_chunk(
                    &source,
                    &depth_values,
                    &params_buffer,
                    (width_u32, chunk_height_u32),
                )
                .await?;
            write_chunk(&mut image, &rendered, y_start);
        }
        Ok(())
    }

    /// Runs the preview shader over one chunk and returns its RGBA pixels.
    ///
    /// # Errors
    /// Returns an error when the GPU work or the readback fails.
    async fn run_preview_chunk(
        &self,
        source: &[f32],
        depth_values: &[f32],
        params_buffer: &wgpu::Buffer,
        (width, chunk_height): (u32, u32),
    ) -> Result<Vec<f32>> {
        let pipeline = self
            .adaptive_preview
            .get_or_init(|| Self::create_adaptive_preview(&self.device));
        let output_size = u64::from(width)
            * u64::from(chunk_height)
            * (RGBA_CHANNELS * std::mem::size_of::<f32>()) as u64;
        let source_buffer =
            self.storage_buffer("Preview source pixels", bytemuck::cast_slice(source));
        let depth_buffer =
            self.storage_buffer("Preview depth values", bytemuck::cast_slice(depth_values));
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Preview output pixels"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Preview readback"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Adaptive preview bindings"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                buffer_entry(0, &source_buffer),
                buffer_entry(1, &depth_buffer),
                buffer_entry(2, &output_buffer),
                buffer_entry(3, params_buffer),
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render adaptive preview"),
            });
        dispatch_pass(
            &mut encoder,
            "Adaptive preview compute pass",
            pipeline,
            &bind_group,
            [
                width.div_ceil(WORKGROUP_EDGE),
                chunk_height.div_ceil(WORKGROUP_EDGE),
            ],
        );
        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging, 0, output_size);
        let (sender, receiver) = futures_channel::oneshot::channel();
        encoder.map_buffer_on_submit(&staging, wgpu::MapMode::Read, .., |result| {
            crate::runners::wgpu::complete_readback(sender, result);
        });
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()?;
        receiver.await??;
        let view = staging.slice(..).get_mapped_range()?;
        let rendered = bytemuck::cast_slice::<u8, f32>(&view).to_vec();
        drop(view);
        staging.unmap();
        Ok(rendered)
    }
}
