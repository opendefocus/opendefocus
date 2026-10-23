//! Small GPU resources shared by the adaptive gather: storage buffers, the source texture, the sampler and the lens uniform.
use super::super::WgpuRunner;
use crate::convolve_settings::{ConvolveSettings, GlobalFlags};

/// Bytes of one RGBA32F texel.
const RGBA32F_TEXEL_BYTES: u32 = 16;

/// Builds the lens settings the shader reads: the caller's lens, or a neutral one covering the frame.
/// Without an aperture texture the shader gathers a simple disc.
pub(super) fn native_lens(
    lens: Option<&ConvolveSettings>,
    width: u32,
    height: u32,
    max_radius_px: f32,
    use_aperture: bool,
) -> ConvolveSettings {
    let mut native = lens.copied().unwrap_or_else(|| ConvolveSettings {
        full_region: glam::IVec4::new(0, 0, width as i32, height as i32),
        process_region: glam::IVec4::new(0, 0, width as i32, height as i32),
        resolution: glam::UVec2::new(width, height),
        pixel_aspect: 1.0,
        pixel_aspect_normalizer: 1.0,
        filter_aspect_ratio: 1.0,
        filter_aspect_ratio_normalizer: 1.0,
        max_size: max_radius_px,
        ..Default::default()
    });
    if !use_aperture {
        native.flags |= GlobalFlags::SIMPLE_DISC.bits();
    }
    native
}

/// Returns true when every source pixel is opaque, which enables the opaque coverage restore.
pub(super) fn source_is_opaque(source: &ndarray::Array3<f32>) -> bool {
    source
        .index_axis(ndarray::Axis(2), 3)
        .iter()
        .all(|alpha| *alpha >= 1.0 - 1.0e-6)
}

impl WgpuRunner {
    /// Uploads `data`, tightly packed RGBA32F rows, as a `width` by `height` texture.
    pub(super) fn upload_source_texture(
        &self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> wgpu::Texture {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Adaptive gather source texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * RGBA32F_TEXEL_BYTES),
                rows_per_image: Some(height),
            },
            size,
        );
        texture
    }

    /// Creates the clamped linear sampler of the gather. Aperture lookups filter between mips; layer lookups do not.
    pub(super) fn gather_sampler(&self, use_aperture: bool, max_lod: f32) -> wgpu::Sampler {
        self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Adaptive gather linear sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: if use_aperture {
                wgpu::MipmapFilterMode::Linear
            } else {
                wgpu::MipmapFilterMode::Nearest
            },
            lod_min_clamp: 0.0,
            lod_max_clamp: max_lod,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        })
    }
}
