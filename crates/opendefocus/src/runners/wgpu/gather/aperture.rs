//! Uploads the aperture mip chain and its integral bounds for the adaptive gather.
use super::super::WgpuRunner;
use crate::convolve_settings::ConvolveSettings;
use crate::error::{Error, Result};
use crate::renders::resize::MipmapBuffer;
use ndarray::ArrayView3;
use wgpu::util::DeviceExt;

/// Bytes of one RGBA32F aperture texel.
const APERTURE_TEXEL_BYTES: u32 = 16;
/// Channels of an aperture mip.
const APERTURE_CHANNELS: usize = 4;

/// Checks that the mips form a halving chain of finite RGBA images whose base matches the lens filter resolution
/// and fits the texture limit.
///
/// # Errors
/// Returns `InvalidDimensions` when the chain is empty, mismatched or contains non-finite values.
fn validate_aperture_mips(
    mips: &[ArrayView3<'_, f32>],
    lens: &ConvolveSettings,
    max_texture_dimension: usize,
) -> Result<()> {
    let Some(base) = mips.first() else {
        return Err(Error::InvalidDimensions);
    };
    let (base_height, base_width, _) = base.dim();
    if base_height > max_texture_dimension
        || base_width > max_texture_dimension
        || base_width != lens.filter_resolution.x as usize
        || base_height != lens.filter_resolution.y as usize
        || mips.iter().any(|mip| {
            mip.dim().2 != APERTURE_CHANNELS || mip.iter().any(|value| !value.is_finite())
        })
    {
        return Err(Error::InvalidDimensions);
    }
    let (mut expected_width, mut expected_height) = (base_width, base_height);
    for mip in &mips[1..] {
        expected_width = (expected_width / 2).max(1);
        expected_height = (expected_height / 2).max(1);
        if mip.dim() != (expected_height, expected_width, APERTURE_CHANNELS) {
            return Err(Error::InvalidDimensions);
        }
    }
    Ok(())
}

impl WgpuRunner {
    /// Uploads every mip of `aperture` into one RGBA32F texture.
    ///
    /// # Errors
    /// Returns `MissingLensSettings` without a lens, `InvalidDimensions` for a malformed mip chain,
    /// and `NonContiguousArray` when a mip cannot be uploaded directly.
    pub(super) fn upload_aperture_texture(
        &self,
        aperture: &MipmapBuffer,
        lens: Option<&ConvolveSettings>,
    ) -> Result<wgpu::Texture> {
        let lens_settings = lens.ok_or(Error::MissingLensSettings)?;
        let mip_views = aperture.get_images_view();
        validate_aperture_mips(
            &mip_views,
            lens_settings,
            self.device.limits().max_texture_dimension_2d as usize,
        )?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Adaptive aperture/filter mip chain"),
            size: wgpu::Extent3d {
                width: mip_views[0].dim().1 as u32,
                height: mip_views[0].dim().0 as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_views.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip_level, mip) in mip_views.iter().enumerate() {
            let mip_width = mip.dim().1 as u32;
            let mip_height = mip.dim().0 as u32;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip_level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(mip.as_slice().ok_or(Error::NonContiguousArray)?),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip_width * APERTURE_TEXEL_BYTES),
                    rows_per_image: Some(mip_height),
                },
                wgpu::Extent3d {
                    width: mip_width,
                    height: mip_height,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(texture)
    }

    /// Builds the min/max bounds of the aperture mips that the integral traversal uses to skip empty regions,
    /// and uploads them as a storage buffer.
    ///
    /// # Errors
    /// Returns `NonContiguousArray` when a mip cannot be read directly, and `SizeLimitExceeded` when the
    /// bounds exceed `max_storage` bytes.
    pub(super) fn upload_aperture_bounds(
        &self,
        aperture: &MipmapBuffer,
        max_storage: u64,
    ) -> Result<wgpu::Buffer> {
        let mip_views = aperture.get_images_view();
        let aperture_pixels = mip_views
            .iter()
            .map(|mip| {
                mip.as_slice()
                    .ok_or(Error::NonContiguousArray)
                    .map(|values| {
                        values
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|pixel| glam::Vec4::new(pixel[0], pixel[1], pixel[2], pixel[3]))
                            .collect::<Vec<_>>()
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        let aperture_mips: Vec<_> = aperture_pixels
            .iter()
            .zip(&mip_views)
            .map(|(pixels, mip)| crate::kernel::ApertureMip {
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
        let bounds_bytes = (packed_bounds.len() as u64) * 4;
        if bounds_bytes == 0 || bounds_bytes > max_storage {
            return Err(Error::SizeLimitExceeded);
        }
        Ok(self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Adaptive aperture integral bounds"),
                contents: bytemuck::cast_slice(&packed_bounds),
                usage: wgpu::BufferUsages::STORAGE,
            }))
    }
}
