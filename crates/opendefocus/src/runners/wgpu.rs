//! WGPU runner: adapter and device setup, the pipeline and buffer fields, and the uniform parameter structs
//! that mirror the WGSL shader layouts. Each stage's dispatch code lives in a submodule.
/// Adaptive pipeline construction and dispatch on the runner.
#[path = "adaptive_gpu.rs"]
mod adaptive_gpu;
/// Streaming of large frames as disjoint source cores with halos.
mod adaptive_streaming;
/// Tile classifier dispatch.
mod classify;
/// Adaptive gather dispatch and integral-tree subtree ends.
mod gather;
/// Compute pipeline creation for the adaptive shaders.
mod pipelines;
/// Layer preparation dispatch: near and far initialization, jump flooding and composition.
mod prepare;
/// Focal and bokeh preview dispatch.
mod preview;
/// Mip pyramid reduction and packing dispatch.
mod pyramid;

pub(crate) use adaptive_streaming::{StreamGather, StreamSource};
pub(super) use gather::integral_subtree_ends;
#[cfg(test)]
#[path = "adaptive_streaming_gpu_tests.rs"]
/// Hardware tests for streamed gathers.
mod adaptive_streaming_gpu_tests;
#[cfg(test)]
/// Tests for the tile classifier.
mod classifier_tests;
#[cfg(test)]
/// Hardware tests for the adaptive gather.
mod gather_tests;
#[cfg(test)]
/// Hardware tests for lens effects.
mod lens_tests;
#[cfg(test)]
/// Tests that compare GPU output with analytic probes instead of CPU oracles.
mod oracle_free_tests;
#[cfg(test)]
/// Hardware tests for layer preparation.
mod preparation_tests;

use crate::abort::get_aborted;
use crate::error::{Error, Result};
use crate::runners::adaptive::PreparedLayers;
use ndarray::Array3;
use wgpu::util::DeviceExt;
use wgpu::{ExperimentalFeatures, Limits};

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Per-tile features from the classifier: eleven floats per 16x16 tile, documented on `values`.
pub struct TileFeatures {
    /// min/max signed `CoC`, min/max alpha, min/max luminance, max local contrast,
    /// peak luminance, reason bits, class (0 sharp, 1 smooth, 2 detail), max adjacent `CoC` step.
    pub values: [f32; 11],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Uniform parameters of the classifier pass.
struct ClassifierParams {
    /// Frame width in pixels.
    width: u32,
    /// Frame height in pixels.
    height: u32,
    /// Number of tile columns.
    tiles_x: u32,
    /// Number of tile rows.
    tiles_y: u32,
    /// Global x origin of the frame, which keeps tiles aligned for cropped frames.
    origin_x: i32,
    /// Global y origin of the frame, which keeps tiles aligned for cropped frames.
    origin_y: i32,
    /// Largest neighbour search reach in tiles.
    max_search_tiles: u32,
    /// Explicit padding that keeps the uniform layout aligned.
    padding: u32,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Uniform parameters of the layer preparation pass.
struct PrepareParams {
    /// Frame width in pixels.
    width: u32,
    /// Frame height in pixels.
    height: u32,
    /// Jump-flood step distance in pixels for the current pass.
    jump_step: u32,
    /// 1 when the input CoC is positive in front of focus, 0 when it is negative.
    foreground_positive: u32,
    /// Largest search radius in pixels.
    max_radius: f32,
    /// Explicit padding that keeps the uniform layout aligned.
    padding: [u32; 3],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Uniform parameters of one pyramid reduction step.
struct PyramidParams {
    /// Frame dimensions for the reduction step, packed into four words as the pyramid shader reads them.
    frame: [u32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Uniform parameters of one gather dispatch.
struct GatherParams {
    /// Frame dimensions and origin, packed into four words as the gather shader reads them.
    frame: [u32; 4],
    /// Origin of the dispatched tile block in the frame.
    tile_origin: [u32; 2],
    /// 1 when the source alpha is opaque everywhere, which enables opaque coverage restore.
    source_opaque: u32,
    /// Explicit padding that keeps the uniform layout aligned.
    _tile_padding: u32,
    /// Sample counts for the smooth and detail tiers, followed by their offsets in the sample table.
    samples: [u32; 4],
    /// Gather radii in pixels, in the order the gather shader reads them.
    radius: [f32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
/// Uniform parameters of one preview dispatch.
struct PreviewParams {
    /// Preview image dimensions, packed as the preview shader reads them.
    dimensions: [u32; 4],
    /// Chunk parameters for chunked preview dispatch.
    chunk: [u32; 4],
    /// Lens shape parameters of the preview.
    shape: [f32; 4],
    /// Ring parameters of the preview lens.
    ring: [f32; 4],
    /// Colour parameters of the preview.
    color: [f32; 4],
}

#[derive(Debug, Clone)]
/// Owns the WGPU device and queue and the adaptive pipelines. The core stages are built by `new_adaptive`; the convolution and preview pipelines are built on first use.
pub struct WgpuRunner {
    /// The WGPU device the renderer allocates resources on.
    pub device: wgpu::Device,
    /// Command queue used for uploads and submissions.
    queue: wgpu::Queue,
    /// Tile classifier pipeline.
    adaptive_classifier: Option<wgpu::ComputePipeline>,
    /// Jump-flood propagation pipeline.
    adaptive_propagation: Option<wgpu::ComputePipeline>,
    /// Layer initialization pipeline of the prepare stage.
    adaptive_prepare_init: Option<wgpu::ComputePipeline>,
    /// Jump-flood step pipeline of the prepare stage.
    adaptive_prepare_jump: Option<wgpu::ComputePipeline>,
    /// Composition pipeline of the prepare stage.
    adaptive_prepare_compose: Option<wgpu::ComputePipeline>,
    /// Mip pyramid reduction pipeline.
    adaptive_pyramid: Option<wgpu::ComputePipeline>,
    /// Packing pipeline that writes the gather layout.
    adaptive_pack: Option<wgpu::ComputePipeline>,
    /// Sampled gather pipeline for Fast, built on first use.
    adaptive_convolution_sampled: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Exact Accurate gather pipeline, built on first use.
    adaptive_convolution_high: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Exact plain-circle Accurate specialization, built on first use.
    adaptive_convolution_circle: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Fused plain-circle Accurate specialization that walks the near tree once, built on first use.
    adaptive_convolution_circle_fused: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Accurate gather for streamed source cores, built on first use.
    adaptive_convolution_stream_high: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Compaction pipeline that packs finalized RGBA into 16 bytes per pixel, built on first use.
    adaptive_stream_compact: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Focal and bokeh preview pipeline, built on first use.
    adaptive_preview: std::sync::OnceLock<wgpu::ComputePipeline>,
    /// Sample offset table buffer, created by `new_adaptive`.
    adaptive_sample_offsets: Option<wgpu::Buffer>,
    /// Name of the graphics backend, such as Metal.
    backend_name: String,
    /// Name of the selected adapter as reported by WGPU.
    adapter_name: String,
}

/// Rejects non-finite or out-of-range prepared `CoC` (and non-finite source
/// pixels when `rgba` is given) before any GPU work is recorded.
///
/// # Errors
/// Returns `AdaptiveCocOutOfRange` when the CoC magnitude exceeds `max_radius_px`, or when any CoC or source value is not finite.
pub(crate) fn validate_adaptive_inputs(
    rgba: Option<&Array3<f32>>,
    coc: &ndarray::Array2<f32>,
    max_radius_px: f32,
) -> Result<()> {
    if !max_radius_px.is_finite()
        || max_radius_px < 0.0
        || coc
            .iter()
            .any(|value| !value.is_finite() || value.abs() > max_radius_px + f32::EPSILON)
        || rgba.is_some_and(|rgba| rgba.iter().any(|value| !value.is_finite()))
    {
        return Err(Error::AdaptiveCocOutOfRange);
    }
    Ok(())
}
/// Returns the display name of a WGPU backend as shown in device descriptions.
fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Dx12 => "DirectX 12",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::BrowserWebGpu => "WebGPU",
        wgpu::Backend::Noop => "Unknown",
    }
}

/// Hands a mapped-buffer result to the reader waiting on `sender`. The reader is gone when the render
/// future was dropped before the map finished, and then nobody is left to inform.
pub(crate) fn complete_readback<T>(sender: futures_channel::oneshot::Sender<T>, result: T) {
    sender.send(result).ok();
}

/// Returns the sub-texel shift applied to the bokeh shape when the lens inverts the foreground shape.
pub(crate) fn disc_shift(lens: Option<&crate::convolve_settings::ConvolveSettings>) -> glam::Vec2 {
    lens.filter(|lens| {
        crate::convolve_settings::GlobalFlags::from_bits_retain(lens.flags)
            .contains(crate::convolve_settings::GlobalFlags::INVERSE_FOREGROUND_BOKEH_SHAPE)
    })
    .map_or(glam::Vec2::ZERO, |lens| {
        2.0 / lens.filter_resolution.as_vec2()
    })
}

/// Binds the whole `buffer` at `binding`.
pub(crate) fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

/// Binds `view` at `binding`.
pub(crate) fn view_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

/// Records one compute pass that runs `pipeline` with `bind_group` over `workgroups` groups.
pub(crate) fn dispatch_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &'static str,
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    workgroups: [u32; 2],
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some(label),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.dispatch_workgroups(workgroups[0], workgroups[1], 1);
}

impl WgpuRunner {
    /// Creates a read-only storage buffer holding `contents`.
    pub(crate) fn storage_buffer(&self, label: &'static str, contents: &[u8]) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE,
            })
    }

    /// Creates a uniform buffer holding `contents`.
    pub(crate) fn uniform_buffer(&self, label: &'static str, contents: &[u8]) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::UNIFORM,
            })
    }

    /// Creates a bind group for `pipeline` that binds each of `buffers` whole at its binding index.
    pub(crate) fn buffer_bind_group(
        &self,
        label: &'static str,
        pipeline: &wgpu::ComputePipeline,
        buffers: &[(u32, &wgpu::Buffer)],
    ) -> wgpu::BindGroup {
        let entries: Vec<_> = buffers
            .iter()
            .map(|(binding, buffer)| buffer_entry(*binding, buffer))
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }

    /// Blocks until all submitted GPU work has finished.
    ///
    /// # Errors
    /// Returns an error when the device cannot be polled.
    pub(crate) fn wait_for_gpu(&self) -> Result<()> {
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok(())
    }

    /// Creates a runner on the first suitable WGPU adapter. On macOS only the Metal backend is used; elsewhere all backends are tried.
    ///
    /// The high-performance adapter is tried first, then the fallback adapter. The device requests `FLOAT32_FILTERABLE`, a 16384 texture dimension limit and a buffer limit of 512 MiB or the adapter maximum, whichever is smaller.
    ///
    /// # Errors
    /// Returns `NoAdapter` when no adapter is found, or a wgpu error when the device cannot be created.
    pub async fn new() -> Result<Self> {
        #[cfg(target_os = "macos")]
        let backends = wgpu::Backends::METAL;

        #[cfg(not(target_os = "macos"))]
        let backends = wgpu::Backends::all();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await
        {
            Ok(adapter) => adapter,
            Err(_) => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: true,
                    compatible_surface: None,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|_| Error::NoAdapter)?,
        };

        let info = adapter.get_info();
        let backend_name = backend_label(info.backend).to_string();
        let adapter_name = info.name.clone();

        // Raising a limit does not allocate a buffer. Retain enough capability
        // for complete 4K spatial context while bounding individual allocations.
        let supported_limits = adapter.limits();
        let adaptive_buffer_limit = (512_u64 * 1024 * 1024)
            .min(supported_limits.max_buffer_size)
            .min(supported_limits.max_storage_buffer_binding_size);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("GPU Device"),
                experimental_features: ExperimentalFeatures::disabled(),
                required_limits: Limits {
                    max_storage_buffer_binding_size: adaptive_buffer_limit,
                    max_buffer_size: adaptive_buffer_limit,
                    max_texture_dimension_2d: supported_limits.max_texture_dimension_2d.min(16_384),
                    ..Default::default()
                },
                required_features: wgpu::Features::FLOAT32_FILTERABLE,
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::default(),
            })
            .await?;

        Ok(Self {
            device,
            queue,
            adaptive_classifier: None,
            adaptive_propagation: None,
            adaptive_prepare_init: None,
            adaptive_prepare_jump: None,
            adaptive_prepare_compose: None,
            adaptive_pyramid: None,
            adaptive_pack: None,
            adaptive_convolution_sampled: std::sync::OnceLock::new(),
            adaptive_convolution_high: std::sync::OnceLock::new(),
            adaptive_convolution_circle: std::sync::OnceLock::new(),
            adaptive_convolution_circle_fused: std::sync::OnceLock::new(),
            adaptive_convolution_stream_high: std::sync::OnceLock::new(),
            adaptive_stream_compact: std::sync::OnceLock::new(),
            adaptive_preview: std::sync::OnceLock::new(),
            adaptive_sample_offsets: None,
            backend_name,
            adapter_name,
        })
    }

    /// Creates a runner and builds the classifier, prepare, pyramid, pack and sample offset pipelines.
    /// # Errors
    /// Returns the errors from [`Self::new`].
    pub async fn new_adaptive() -> Result<Self> {
        let mut runner = Self::new().await?;
        runner.adaptive_classifier = Some(Self::create_adaptive_classifier(&runner.device));
        runner.adaptive_propagation = Some(Self::create_adaptive_propagation(&runner.device));
        let (initialize, jump_flood, compose_layers) =
            Self::create_adaptive_prepare_pipelines(&runner.device);
        runner.adaptive_prepare_init = Some(initialize);
        runner.adaptive_prepare_jump = Some(jump_flood);
        runner.adaptive_prepare_compose = Some(compose_layers);
        runner.adaptive_pyramid = Some(Self::create_adaptive_pyramid(&runner.device));
        runner.adaptive_pack = Some(Self::create_adaptive_pack(&runner.device));
        runner.adaptive_sample_offsets = Some(Self::create_adaptive_sample_offsets(&runner.device));
        Ok(runner)
    }

    /// Returns the adapter and backend description, such as `Name - Metal`.
    pub(crate) fn device_description(&self) -> String {
        format!("{} - {}", self.adapter_name, self.backend_name)
    }
}

#[cfg(test)]
/// Tests for the WGPU backend display names.
mod tests {
    use super::backend_label;

    #[test]
    /// Verifies every WGPU backend maps to its readable device-description name.
    fn backend_label_names_every_backend() {
        assert_eq!(backend_label(wgpu::Backend::Vulkan), "Vulkan");
        assert_eq!(backend_label(wgpu::Backend::Metal), "Metal");
        assert_eq!(backend_label(wgpu::Backend::Dx12), "DirectX 12");
        assert_eq!(backend_label(wgpu::Backend::Gl), "OpenGL");
        assert_eq!(backend_label(wgpu::Backend::BrowserWebGpu), "WebGPU");
        assert_eq!(backend_label(wgpu::Backend::Noop), "Unknown");
    }
}
