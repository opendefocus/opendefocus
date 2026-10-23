//! Creates the sampler and GPU aperture resources for streamed gathers.
use crate::convolve_settings::ConvolveSettings;
use crate::kernel::ApertureMip;
use crate::{
    error::{Error, Result},
    renders::resize::MipmapBuffer,
    runners::wgpu::WgpuRunner,
};
use wgpu::util::DeviceExt;

use super::StreamAperture;

impl WgpuRunner {
    /// Creates the sampler and GPU aperture resources for a streamed gather. Linear mipmap filtering is used only when an aperture mip chain is supplied.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid, or when a GPU resource cannot be created or read back.
    pub(super) fn make_stream_aperture(
        &self,
        aperture: Option<&MipmapBuffer>,
        lens: &ConvolveSettings,
    ) -> Result<StreamAperture> {
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Adaptive streamed gather sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: if aperture.is_some() {
                wgpu::MipmapFilterMode::Linear
            } else {
                wgpu::MipmapFilterMode::Nearest
            },
            lod_min_clamp: 0.0,
            lod_max_clamp: 16.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        if let Some(aperture) = aperture {
            let mips = aperture.get_images_view();
            if mips.is_empty()
                || mips[0].dim().2 != 4
                || mips[0].dim().1 != lens.filter_resolution.x as usize
                || mips[0].dim().0 != lens.filter_resolution.y as usize
            {
                return Err(Error::InvalidDimensions);
            }
            if mips
                .iter()
                .any(|mip| mip.iter().any(|value| !value.is_finite()))
            {
                return Err(Error::InvalidParameter);
            }
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Adaptive streamed aperture mip chain"),
                size: wgpu::Extent3d {
                    width: mips[0].dim().1 as u32,
                    height: mips[0].dim().0 as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let mut pixels = Vec::with_capacity(mips.len());
            for (level, mip) in mips.iter().enumerate() {
                let values = mip.as_slice().ok_or(Error::NonContiguousArray)?;
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(values),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip.dim().1 as u32 * 16),
                        rows_per_image: Some(mip.dim().0 as u32),
                    },
                    wgpu::Extent3d {
                        width: mip.dim().1 as u32,
                        height: mip.dim().0 as u32,
                        depth_or_array_layers: 1,
                    },
                );
                pixels.push(
                    values
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|pixel| glam::Vec4::new(pixel[0], pixel[1], pixel[2], pixel[3]))
                        .collect::<Vec<_>>(),
                );
            }
            let aperture_mips: Vec<_> = pixels
                .iter()
                .zip(mips.iter())
                .map(|(pixels, mip)| ApertureMip {
                    resolution: glam::UVec2::new(mip.dim().1 as u32, mip.dim().0 as u32),
                    pixels,
                })
                .collect();
            let mut bounds = crate::kernel::build_aperture_bounds(&aperture_mips);
            bounds.extend(crate::kernel::build_aperture_bounds_layout(&aperture_mips));
            let packed_bounds: Vec<f32> = bounds
                .iter()
                .flat_map(|bound| {
                    [
                        bound.min.x,
                        bound.min.y,
                        bound.min.z,
                        bound.min.w,
                        bound.max.x,
                        bound.max.y,
                        bound.max.z,
                        bound.max.w,
                    ]
                })
                .collect();
            let bounds = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Adaptive streamed aperture bounds"),
                    contents: bytemuck::cast_slice(&packed_bounds),
                    usage: wgpu::BufferUsages::STORAGE,
                });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            Ok(StreamAperture {
                sampler,
                view: Some(view),
                _texture: Some(texture),
                bounds,
            })
        } else {
            let bounds = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Adaptive streamed circular bounds"),
                    contents: bytemuck::cast_slice(&[1.0_f32; 8]),
                    usage: wgpu::BufferUsages::STORAGE,
                });
            Ok(StreamAperture {
                sampler,
                view: None,
                _texture: None,
                bounds,
            })
        }
    }
}
