//! Keep the reduced layers on the GPU between pyramid construction and gather.
use super::{Error, PreparedLayers, PyramidParams, Result, WgpuRunner, get_aborted};
use crate::runners::wgpu::dispatch_pass;

/// Bytes of one prepared layer pixel in the reduction buffers: twelve floats.
const LAYER_PIXEL_BYTES: u64 = 48;
/// Bytes of one packed RGBA32F texel.
const PACKED_TEXEL_BYTES: u64 = 16;

/// The four reduced layer textures of a resident pyramid, with its highest mip index.
pub(super) struct GatherTextures {
    /// Textures holding the reduced near and far layers, one per layer group.
    pub textures: [wgpu::Texture; 4],
    /// Highest mip level index, as a float for the gather shader.
    pub max_mip: f32,
}

impl WgpuRunner {
    /// Builds the reduced layer pyramid and keeps it on the GPU for the gather.
    /// # Errors
    /// Returns `AdaptiveCancelled` when an abort is requested while the pyramid is built, or a GPU error when a resource cannot be created.
    pub(super) fn resident_adaptive_pyramid(
        &self,
        prepared: &PreparedLayers,
    ) -> Result<GatherTextures> {
        self.resident_adaptive_pyramid_with_base_only(prepared, false)
    }

    /// Like [`Self::resident_adaptive_pyramid`], but keeps only the base level, so `max_mip` is 0.
    ///
    /// # Errors
    /// Returns `AdaptiveCancelled` when an abort is requested, or a GPU error when a texture cannot be created.
    pub(super) fn resident_adaptive_base_textures(
        &self,
        prepared: &PreparedLayers,
    ) -> Result<GatherTextures> {
        self.resident_adaptive_pyramid_with_base_only(prepared, true)
    }

    /// Shared implementation of the resident pyramid builders. With `base_only` set, only the base level is built.
    ///
    /// # Errors
    /// Returns `AdaptiveCancelled` when an abort is requested, or a GPU error when a texture cannot be created.
    fn resident_adaptive_pyramid_with_base_only(
        &self,
        prepared: &PreparedLayers,
        base_only: bool,
    ) -> Result<GatherTextures> {
        let reduce = self
            .adaptive_pyramid
            .as_ref()
            .ok_or(Error::AdaptiveClassifierUnavailable)?;
        let pack = self
            .adaptive_pack
            .as_ref()
            .ok_or(Error::AdaptiveClassifierUnavailable)?;
        let width = u32::try_from(prepared.width).map_err(|_| Error::SizeLimitExceeded)?;
        let height = u32::try_from(prepared.height).map_err(|_| Error::SizeLimitExceeded)?;
        if width == 0
            || height == 0
            || prepared.pixels.len() != prepared.width * prepared.height
            || prepared.near_edge_distance.len() != prepared.width * prepared.height
        {
            return Err(Error::InvalidDimensions);
        }
        let limits = self.device.limits();
        let source_size = u64::from(width) * u64::from(height) * LAYER_PIXEL_BYTES;
        if width > limits.max_texture_dimension_2d
            || height > limits.max_texture_dimension_2d
            || source_size > limits.max_storage_buffer_binding_size
        {
            return Err(Error::SizeLimitExceeded);
        }
        let mip_count = if base_only {
            1
        } else {
            width.max(height).ilog2() + 1
        };
        let textures = self.create_layer_textures(width, height, mip_count);
        let mut source = self.storage_buffer(
            "Adaptive resident layer input",
            bytemuck::cast_slice(&prepared.pixels),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Adaptive resident pyramid encoder"),
            });
        let mut level = (width, height);
        for mip in 0..mip_count {
            if get_aborted() {
                return Err(Error::AdaptiveCancelled);
            }
            if mip > 0 {
                let (target, reduced) = self.encode_reduction(&mut encoder, reduce, &source, level);
                source = target;
                level = reduced;
            }
            let edge_distances = if mip == 0 {
                prepared.near_edge_distance.clone()
            } else {
                vec![-1.0; (level.0 * level.1) as usize]
            };
            self.encode_level_packing(
                &mut encoder,
                pack,
                &source,
                &edge_distances,
                level,
                (&textures, mip),
            )?;
        }
        // Subsequent gather submissions on this queue consume these textures in order.
        self.queue.submit([encoder.finish()]);
        Ok(GatherTextures {
            textures,
            max_mip: (mip_count - 1) as f32,
        })
    }

    /// Creates the four RGBA32F layer textures with `mip_count` levels.
    fn create_layer_textures(&self, width: u32, height: u32, mip_count: u32) -> [wgpu::Texture; 4] {
        [
            "Adaptive near color",
            "Adaptive far color",
            "Adaptive near metadata",
            "Adaptive far metadata",
        ]
        .map(|label| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mip_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        })
    }

    /// Records the reduction of `source` (a `level` sized layer buffer) to half size and returns the
    /// reduced buffer with its size.
    fn encode_reduction(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        reduce: &wgpu::ComputePipeline,
        source: &wgpu::Buffer,
        (level_width, level_height): (u32, u32),
    ) -> (wgpu::Buffer, (u32, u32)) {
        let target_width = (level_width / 2).max(1);
        let target_height = (level_height / 2).max(1);
        let target = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive resident reduced layers"),
            size: u64::from(target_width) * u64::from(target_height) * LAYER_PIXEL_BYTES,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let params = self.uniform_buffer(
            "Adaptive resident reduction dimensions",
            bytemuck::bytes_of(&PyramidParams {
                frame: [level_width, level_height, target_width, target_height],
            }),
        );
        let group = self.buffer_bind_group(
            "Adaptive resident reduction bindings",
            reduce,
            &[(0, &params), (1, source), (2, &target)],
        );
        dispatch_pass(
            encoder,
            "Adaptive resident reduction",
            reduce,
            &group,
            [target_width.div_ceil(8), target_height.div_ceil(8)],
        );
        (target, (target_width, target_height))
    }

    /// Records packing of one pyramid level into texture-ready buffers and their copy into mip `mip` of `textures`.
    ///
    /// # Errors
    /// Returns `SizeLimitExceeded` when a packed level exceeds one storage binding.
    fn encode_level_packing(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pack: &wgpu::ComputePipeline,
        source: &wgpu::Buffer,
        edge_distances: &[f32],
        (level_width, level_height): (u32, u32),
        (textures, mip): (&[wgpu::Texture; 4], u32),
    ) -> Result<()> {
        // Buffer-to-texture copies require 256-byte rows, including odd widths.
        let row_width = level_width.div_ceil(16) * 16;
        let packed_size = u64::from(row_width) * u64::from(level_height) * PACKED_TEXEL_BYTES;
        if packed_size > self.device.limits().max_storage_buffer_binding_size {
            return Err(Error::SizeLimitExceeded);
        }
        let packed: [wgpu::Buffer; 4] = std::array::from_fn(|_| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Adaptive resident packed texture level"),
                size: packed_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });
        let edge_distance_buffer = self.storage_buffer(
            "Adaptive resident edge distances",
            bytemuck::cast_slice(edge_distances),
        );
        let params = self.uniform_buffer(
            "Adaptive resident packing dimensions",
            bytemuck::bytes_of(&PyramidParams {
                frame: [level_width, level_height, row_width, 0],
            }),
        );
        let group = self.buffer_bind_group(
            "Adaptive resident packing bindings",
            pack,
            &[
                (0, &params),
                (1, source),
                (2, &packed[0]),
                (3, &packed[1]),
                (4, &packed[2]),
                (5, &packed[3]),
                (6, &edge_distance_buffer),
            ],
        );
        dispatch_pass(
            encoder,
            "Adaptive resident texture packing",
            pack,
            &group,
            [level_width.div_ceil(8), level_height.div_ceil(8)],
        );
        for (buffer, texture) in packed.iter().zip(textures) {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_width * PACKED_TEXEL_BYTES as u32),
                        rows_per_image: Some(level_height),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: level_width,
                    height: level_height,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
/// Hardware tests for resident pyramids.
mod tests {
    use super::*;
    use crate::runners::adaptive::AdaptiveLayerPixel;

    /// Asserts that every texel of one mapped readback level matches its packed reference, applying the near edge distance at mip 0.
    fn assert_readback_matches(
        values: &[f32],
        row_floats: usize,
        (level_width, level_height): (usize, usize),
        source: &[f32],
        prepared: &PreparedLayers,
        (mip, texture_index): (usize, usize),
    ) {
        let (width, height) = (prepared.width, prepared.height);
        for (y, x) in (0..level_height).flat_map(|y| (0..level_width).map(move |x| (y, x))) {
            let edge_distance = if mip == 0 {
                prepared.near_edge_distance[y * width + x]
            } else {
                -1.0
            };
            let reference = crate::kernel::adaptive_pack_pixel_with_edge_distance(
                source,
                y * level_width + x,
                edge_distance,
            )[texture_index];
            for channel in 0..4 {
                let actual = values[y * row_floats + x * 4 + channel];
                assert!(
                    (actual - reference[channel]).abs() < 1e-5,
                    "{width}x{height} mip{mip} texture{texture_index} at({x},{y}) channel{channel}: {actual} != {}",
                    reference[channel]
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires a hardware GPU; run with adaptive Metal checks"]
    /// Verifies resident textures match the diagnostic pyramid at every mip level.
    async fn resident_textures_match_diagnostic_pyramid_at_every_mip() {
        let runner = WgpuRunner::new_adaptive().await.expect("hardware GPU");
        for (width, height) in [(1, 7), (7, 3), (17, 9), (32, 16)] {
            let prepared = PreparedLayers {
                width,
                height,
                near_edge_distance: vec![-1.0; width * height],
                pixels: (0..width * height)
                    .map(|i| {
                        let valid = (i % 4) as f32 / 3.0;
                        AdaptiveLayerPixel {
                            near_rgba: [i as f32 * valid, valid * 2.0, valid * 0.2, valid],
                            far_rgba: [0.4, i as f32 * 0.1, 3.0, 1.0],
                            near_coc: (i % 8 + 1) as f32,
                            far_coc: -((i % 9 + 1) as f32),
                            near_valid: valid,
                            far_valid: 1.0,
                        }
                    })
                    .collect(),
            };
            let expected = runner.build_adaptive_pyramid(&prepared).await.unwrap();
            let resident = runner.resident_adaptive_pyramid(&prepared).unwrap();
            assert_eq!(resident.max_mip as usize + 1, expected.levels.len());
            let base_only = runner.resident_adaptive_base_textures(&prepared).unwrap();
            assert_eq!(base_only.max_mip, 0.0);
            let mut encoder = runner
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            let mut readbacks = Vec::new();
            for (mip, level) in expected.levels.iter().enumerate() {
                let row_bytes = (level.width as u32 * 16).div_ceil(256) * 256;
                for (texture_index, texture) in resident.textures.iter().enumerate() {
                    let staging = runner.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Resident pyramid test readback"),
                        size: u64::from(row_bytes) * level.height as u64,
                        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                        mapped_at_creation: false,
                    });
                    encoder.copy_texture_to_buffer(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: mip as u32,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyBufferInfo {
                            buffer: &staging,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(row_bytes),
                                rows_per_image: Some(level.height as u32),
                            },
                        },
                        wgpu::Extent3d {
                            width: level.width as u32,
                            height: level.height as u32,
                            depth_or_array_layers: 1,
                        },
                    );
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    encoder.map_buffer_on_submit(
                        &staging,
                        wgpu::MapMode::Read,
                        ..,
                        move |result| crate::runners::wgpu::complete_readback(sender, result),
                    );
                    readbacks.push((
                        staging,
                        receiver,
                        mip,
                        texture_index,
                        row_bytes as usize / 4,
                    ));
                }
            }
            runner.queue.submit([encoder.finish()]);
            runner
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            for (staging, receiver, mip, texture_index, row_floats) in readbacks {
                receiver.await.unwrap().unwrap();
                let bytes = staging.slice(..).get_mapped_range().unwrap();
                let values: &[f32] = bytemuck::cast_slice(&bytes);
                let level = &expected.levels[mip];
                let source: &[f32] = bytemuck::cast_slice(&level.pixels);
                assert_readback_matches(
                    values,
                    row_floats,
                    (level.width, level.height),
                    source,
                    &prepared,
                    (mip, texture_index),
                );
                drop(bytes);
                staging.unmap();
            }
        }
    }
}
