//! Dispatches layer preparation on the WGPU runner: initialization, jump-flood propagation and composition of the near and far layers.
use super::validate_adaptive_inputs;
use super::{PrepareParams, WgpuRunner};
use crate::abort::get_aborted;
use crate::error::{Error, Result};
use crate::runners::adaptive::{AdaptiveLayerPixel, CocPolarity, PreparedLayers};
use crate::runners::wgpu::dispatch_pass;
use ndarray::{Array2, Array3};

/// Floats per pixel of the prepared layer output: near and far `RGBA`, two `CoC` values, two coverages and the edge distance.
const PREPARED_FLOATS_PER_PIXEL: usize = 13;
/// Floats per pixel of one jump-flood seed buffer: nearest negative-far and exact-focus seeds.
const SEED_FLOATS_PER_PIXEL: u64 = 4;
/// Threads per workgroup edge of the preparation shaders.
const WORKGROUP_EDGE: u32 = 8;
/// Pixels beyond this edge distance keep their value when flat edges are cleared.
const EDGE_DISTANCE_LIMIT: f32 = 8.0;
/// Coverage differences up to this value count as flat.
const FLAT_COVERAGE_GRADIENT: f32 = 1e-3;
/// Pixels around an edge pixel that are searched for a coverage change.
const COVERAGE_NEIGHBORHOOD: usize = 2;
/// Marks a pixel as having no near edge.
const NO_EDGE: f32 = -1.0;

/// The three preparation pipelines.
struct PreparePipelines<'a> {
    /// Seeds the far background search.
    initialize: &'a wgpu::ComputePipeline,
    /// One jump-flood step.
    jump_flood: &'a wgpu::ComputePipeline,
    /// Composes the near and far layers from the final seeds.
    compose: &'a wgpu::ComputePipeline,
}

/// Returns the jump-flood step sizes from the largest power of two above `max_radius_px` down to one.
fn jump_steps(max_radius_px: f32) -> Vec<u32> {
    let max_radius = max_radius_px.ceil() as u32;
    let mut step = 1u32;
    while step <= max_radius && step <= (u32::MAX / 2) {
        step *= 2;
    }
    let mut steps = Vec::new();
    while step > 0 {
        steps.push(step);
        step /= 2;
    }
    steps
}

/// Splits the raw readback into layer pixels and near edge distances.
fn layers_from_readback(values: &[f32]) -> (Vec<AdaptiveLayerPixel>, Vec<f32>) {
    let records = values.as_chunks::<PREPARED_FLOATS_PER_PIXEL>().0;
    let mut pixels = Vec::with_capacity(records.len());
    let mut near_edge_distance = Vec::with_capacity(records.len());
    for &[
        nr,
        ng,
        nb,
        na,
        fr,
        fg,
        fb,
        fa,
        near_coc,
        far_coc,
        near_valid,
        far_valid,
        edge_distance,
    ] in records
    {
        pixels.push(AdaptiveLayerPixel {
            near_rgba: [nr, ng, nb, na],
            far_rgba: [fr, fg, fb, fa],
            near_coc,
            far_coc,
            near_valid,
            far_valid,
        });
        near_edge_distance.push(edge_distance);
    }
    (pixels, near_edge_distance)
}

/// Returns the largest coverage difference between pixel `(x, y)` and its neighbours.
fn coverage_gradient(
    pixels: &[AdaptiveLayerPixel],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
) -> f32 {
    let center = pixels[y * width + x].near_valid;
    let mut gradient = 0.0f32;
    for neighbor_y in
        y.saturating_sub(COVERAGE_NEIGHBORHOOD)..=(y + COVERAGE_NEIGHBORHOOD).min(height - 1)
    {
        for neighbor_x in
            x.saturating_sub(COVERAGE_NEIGHBORHOOD)..=(x + COVERAGE_NEIGHBORHOOD).min(width - 1)
        {
            if (neighbor_x, neighbor_y) != (x, y) {
                gradient = gradient
                    .max((center - pixels[neighbor_y * width + neighbor_x].near_valid).abs());
            }
        }
    }
    gradient
}

/// Clears the near edge distance of pixels whose neighbourhood has no coverage change, because
/// such pixels have no edge to treat specially.
fn clear_flat_edges(
    pixels: &[AdaptiveLayerPixel],
    near_edge_distance: &mut [f32],
    width: usize,
    height: usize,
) {
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let distance = near_edge_distance[index];
            if !(distance < 0.0 || distance >= EDGE_DISTANCE_LIMIT)
                && coverage_gradient(pixels, width, height, x, y) <= FLAT_COVERAGE_GRADIENT
            {
                near_edge_distance[index] = NO_EDGE;
            }
        }
    }
}

impl WgpuRunner {
    /// Split premultiplied source pixels into near/far layers and extend valid
    /// far background under occluders and exactly-focused background beneath
    /// defocused near pixels using a bounded jump-flood search. Focused color
    /// is never copied into the far mip support of focused pixels themselves.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid, or when a GPU resource cannot be created or read back.
    pub async fn prepare_adaptive_layers(
        &self,
        rgba: &Array3<f32>,
        coc: &Array2<f32>,
        max_radius_px: f32,
        polarity: CocPolarity,
    ) -> Result<PreparedLayers> {
        let pipelines = self.prepare_pipelines()?;
        let (height, width, channels) = rgba.dim();
        if channels != 4 || coc.dim() != (height, width) || width == 0 || height == 0 {
            return Err(Error::InvalidDimensions);
        }
        validate_adaptive_inputs(Some(rgba), coc, max_radius_px)?;
        if get_aborted() {
            return Err(Error::AdaptiveCancelled);
        }
        let width = u32::try_from(width).map_err(|_| Error::SizeLimitExceeded)?;
        let height = u32::try_from(height).map_err(|_| Error::SizeLimitExceeded)?;
        let pixels = u64::from(width) * u64::from(height);
        let output_size = pixels * PREPARED_FLOATS_PER_PIXEL as u64 * 4;
        let seed_size = pixels * SEED_FLOATS_PER_PIXEL * 4;
        let max_storage = self.device.limits().max_storage_buffer_binding_size;
        if output_size > max_storage || seed_size > max_storage {
            return Err(Error::SizeLimitExceeded);
        }
        let inverted_coc =
            (polarity == CocPolarity::NegativeForeground).then(|| coc.mapv(|value| -value));
        let canonical_coc = inverted_coc.as_ref().unwrap_or(coc);
        let staging = self
            .run_preparation(
                &pipelines,
                rgba,
                canonical_coc,
                (width, height),
                max_radius_px,
                (output_size, seed_size),
            )
            .await?;
        let view = staging.slice(..).get_mapped_range()?;
        let (layer_pixels, mut near_edge_distance) =
            layers_from_readback(bytemuck::cast_slice(&view));
        drop(view);
        staging.unmap();
        let (width, height) = (width as usize, height as usize);
        clear_flat_edges(&layer_pixels, &mut near_edge_distance, width, height);
        Ok(PreparedLayers {
            width,
            height,
            pixels: layer_pixels,
            near_edge_distance,
        })
    }

    /// Returns the three preparation pipelines.
    ///
    /// # Errors
    /// Returns `AdaptiveClassifierUnavailable` when the runner was built without them.
    fn prepare_pipelines(&self) -> Result<PreparePipelines<'_>> {
        Ok(PreparePipelines {
            initialize: self
                .adaptive_prepare_init
                .as_ref()
                .ok_or(Error::AdaptiveClassifierUnavailable)?,
            jump_flood: self
                .adaptive_prepare_jump
                .as_ref()
                .ok_or(Error::AdaptiveClassifierUnavailable)?,
            compose: self
                .adaptive_prepare_compose
                .as_ref()
                .ok_or(Error::AdaptiveClassifierUnavailable)?,
        })
    }

    /// Creates the uniform buffer of one preparation stage.
    fn prepare_params(
        &self,
        (width, height): (u32, u32),
        jump_step: u32,
        max_radius: f32,
    ) -> wgpu::Buffer {
        self.uniform_buffer(
            "Adaptive layer stage parameters",
            bytemuck::bytes_of(&PrepareParams {
                width,
                height,
                jump_step,
                foreground_positive: 1,
                max_radius,
                padding: [0; 3],
            }),
        )
    }

    /// Runs initialization, the jump-flood passes and composition, and returns the mapped readback buffer.
    /// All other GPU buffers are released when this returns, before the caller builds the CPU arrays.
    ///
    /// # Errors
    /// Returns an error when a resource cannot be created or the readback fails.
    async fn run_preparation(
        &self,
        pipelines: &PreparePipelines<'_>,
        rgba: &Array3<f32>,
        canonical_coc: &Array2<f32>,
        size: (u32, u32),
        max_radius_px: f32,
        (output_size, seed_size): (u64, u64),
    ) -> Result<wgpu::Buffer> {
        let input_color = self.storage_buffer(
            "Adaptive layer source color",
            bytemuck::cast_slice(rgba.as_slice().ok_or(Error::NonContiguousArray)?),
        );
        let input_coc = self.storage_buffer(
            "Adaptive layer source CoC",
            bytemuck::cast_slice(canonical_coc.as_slice().ok_or(Error::NonContiguousArray)?),
        );
        let seed = |label| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: seed_size,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        let (seed_a, seed_b) = (seed("Adaptive far seed A"), seed("Adaptive far seed B"));
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive prepared near and far layers"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive prepared layer readback"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let steps = jump_steps(max_radius_px);
        let init_params = self.prepare_params(size, 0, max_radius_px);
        let init_group = self.buffer_bind_group(
            "Adaptive seed initialization bindings",
            pipelines.initialize,
            &[(0, &init_params), (2, &input_coc), (4, &seed_a)],
        );
        let compose_params = self.prepare_params(size, 0, max_radius_px);
        let final_seeds = if steps.len().is_multiple_of(2) {
            &seed_a
        } else {
            &seed_b
        };
        let compose_group = self.buffer_bind_group(
            "Adaptive layer composition bindings",
            pipelines.compose,
            &[
                (0, &compose_params),
                (1, &input_color),
                (2, &input_coc),
                (3, final_seeds),
                (5, &output),
            ],
        );
        let jump_groups: Vec<_> = steps
            .iter()
            .enumerate()
            .map(|(index, &jump_step)| {
                let (seeds_in, seeds_out) = if index % 2 == 0 {
                    (&seed_a, &seed_b)
                } else {
                    (&seed_b, &seed_a)
                };
                let params = self.prepare_params(size, jump_step, max_radius_px);
                let group = self.buffer_bind_group(
                    "Adaptive jump flood bindings",
                    pipelines.jump_flood,
                    &[(0, &params), (3, seeds_in), (4, seeds_out)],
                );
                (params, group)
            })
            .collect();

        let workgroups = [
            size.0.div_ceil(WORKGROUP_EDGE),
            size.1.div_ceil(WORKGROUP_EDGE),
        ];
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive near/far preparation encoder"),
            });
        dispatch_pass(
            &mut encoder,
            "Adaptive far seed initialization",
            pipelines.initialize,
            &init_group,
            workgroups,
        );
        for (_, group) in &jump_groups {
            dispatch_pass(
                &mut encoder,
                "Adaptive bounded background extension",
                pipelines.jump_flood,
                group,
                workgroups,
            );
        }
        dispatch_pass(
            &mut encoder,
            "Adaptive near/far layer output",
            pipelines.compose,
            &compose_group,
            workgroups,
        );
        encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, output_size);
        let (sender, receiver) = futures_channel::oneshot::channel();
        encoder.map_buffer_on_submit(&staging, wgpu::MapMode::Read, .., |result| {
            crate::runners::wgpu::complete_readback(sender, result);
        });
        self.queue.submit([encoder.finish()]);
        self.wait_for_gpu()?;
        receiver.await??;
        Ok(staging)
    }
}
