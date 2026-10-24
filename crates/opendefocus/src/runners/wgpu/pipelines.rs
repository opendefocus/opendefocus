//! Compute pipeline creation for the adaptive shaders. Each builder loads one embedded WGSL source and returns its pipeline.
use super::{GatherPipelines, WgpuRunner};
use crate::kernel::adaptive_gather_sample_offsets;
use wgpu::util::DeviceExt;
impl WgpuRunner {
    /// Creates the focal and bokeh preview compute pipeline.
    pub(super) fn create_adaptive_preview(device: &wgpu::Device) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WGPU preview rendering"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_PREVIEW_WGSL.into()),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Adaptive preview renderer"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// Uploads the adaptive gather sample offset table to a GPU buffer.
    pub(super) fn create_adaptive_sample_offsets(device: &wgpu::Device) -> wgpu::Buffer {
        let offsets = adaptive_gather_sample_offsets();
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Adaptive circular gather sample pattern"),
            contents: bytemuck::cast_slice(&offsets),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Creates the pipelines of one gather variant: the sampled gather, or the exact Accurate gather with its far
    /// occlusion pass. The fused specialization applies only to the plain-circle case.
    pub(super) fn create_gather_pipelines(
        device: &wgpu::Device,
        high: bool,
        plain_circle: bool,
        use_fused_capped_circle: bool,
    ) -> GatherPipelines {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native adaptive convolution"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_CONVOLUTION_WGSL.into()),
        });
        let layout = Self::create_adaptive_convolution_layout(device);
        let create = |entry_point| {
            Self::create_adaptive_convolution(
                device,
                &shader,
                &layout,
                entry_point,
                plain_circle,
                use_fused_capped_circle,
            )
        };
        if high {
            GatherPipelines {
                gather: create("adaptive_convolution_high"),
                far_occlusion: Some(create("adaptive_far_occlusion")),
            }
        } else {
            GatherPipelines {
                gather: create("adaptive_convolution_sampled"),
                far_occlusion: None,
            }
        }
    }

    /// Creates the explicit pipeline layout shared by every adaptive convolution entry point.
    fn create_adaptive_convolution_layout(device: &wgpu::Device) -> wgpu::PipelineLayout {
        // Keep one resource contract for circular and spatial aperture kernels.
        let entries: Vec<_> = (0..17)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: match binding {
                    0 | 11 => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    1 => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    3..=7 | 14 => wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    _ => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: binding != 2,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                count: None,
            })
            .collect();
        let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Native adaptive convolution bindings"),
            entries: &entries,
        });
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Native adaptive convolution layout"),
            bind_group_layouts: &[Some(&bindings)],
            immediate_size: 0,
        })
    }

    /// Creates the adaptive convolution pipeline for `entry_point` of `shader` with the plain-circle and fused
    /// specializations.
    fn create_adaptive_convolution(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        entry_point: &str,
        plain_circle: bool,
        use_fused_capped_circle: bool,
    ) -> wgpu::ComputePipeline {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Native adaptive convolution"),
            layout: Some(layout),
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[
                    ("PLAIN_CIRCLE", if plain_circle { 1.0 } else { 0.0 }),
                    (
                        "FUSED_CAPPED_CIRCLE",
                        if use_fused_capped_circle { 1.0 } else { 0.0 },
                    ),
                ],
                ..Default::default()
            },
            cache: None,
        })
    }

    /// Creates the streamed Accurate gather pipeline, which binds 18 storage resources.
    pub(super) fn create_adaptive_streaming_convolution(
        device: &wgpu::Device,
    ) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native adaptive streaming convolution"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_CONVOLUTION_WGSL.into()),
        });
        let entries: Vec<_> = (0..18)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: match binding {
                    0 | 11 | 17 => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    1 => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    3..=7 | 14 => wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    _ => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: binding != 2,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                count: None,
            })
            .collect();
        let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Native adaptive streaming convolution bindings"),
            entries: &entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Native adaptive streaming convolution layout"),
            bind_group_layouts: &[Some(&bindings)],
            immediate_size: 0,
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Native adaptive streaming convolution"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("adaptive_convolution_stream_high"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("PLAIN_CIRCLE", 0.0)],
                ..Default::default()
            },
            cache: None,
        })
    }

    /// Creates the pipeline that compacts streamed output into 16 bytes per pixel.
    pub(super) fn create_adaptive_stream_compact(device: &wgpu::Device) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Adaptive streamed output compaction"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_CONVOLUTION_WGSL.into()),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Adaptive streamed output compaction"),
            layout: None,
            module: &shader,
            entry_point: Some("adaptive_stream_compact"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// Creates the layer pyramid reduction pipeline (entry point `reduce_layers`).
    pub(super) fn create_adaptive_pyramid(device: &wgpu::Device) -> wgpu::ComputePipeline {
        Self::create_native_layer_pipeline(device, "reduce_layers")
    }

    /// Creates the layer packing pipeline (entry point `pack_layers`).
    pub(super) fn create_adaptive_pack(device: &wgpu::Device) -> wgpu::ComputePipeline {
        Self::create_native_layer_pipeline(device, "pack_layers")
    }

    /// Creates a compute pipeline from the pyramid shader for the given entry point.
    fn create_native_layer_pipeline(device: &wgpu::Device, entry: &str) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native adaptive layer pyramid"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_PYRAMID_WGSL.into()),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: None,
            module: &shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// Creates the three layer preparation pipelines: initialization, jump-flood step and composition.
    pub(super) fn create_adaptive_prepare_pipelines(
        device: &wgpu::Device,
    ) -> (
        wgpu::ComputePipeline,
        wgpu::ComputePipeline,
        wgpu::ComputePipeline,
    ) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native adaptive layer preparation"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_PREPARE_WGSL.into()),
        });
        let make = |label, entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: None,
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        (
            make("Adaptive layer seed initialization", "initialize"),
            make("Adaptive bounded background jump flood", "jump_flood"),
            make("Adaptive near/far layer preparation", "compose_layers"),
        )
    }

    /// Creates the tile classifier compute pipeline.
    pub(super) fn create_adaptive_classifier(device: &wgpu::Device) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native adaptive tile classifier"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_CLASSIFIER_WGSL.into()),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Adaptive tile feature classifier"),
            layout: None,
            module: &shader,
            entry_point: Some("classify"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// Creates the jump-flood propagation compute pipeline.
    pub(super) fn create_adaptive_propagation(device: &wgpu::Device) -> wgpu::ComputePipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Native adaptive tile classifier"),
            source: wgpu::ShaderSource::Wgsl(crate::kernel::ADAPTIVE_CLASSIFIER_WGSL.into()),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Adaptive blur reach propagation"),
            layout: None,
            module: &shader,
            entry_point: Some("propagate"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }
}
