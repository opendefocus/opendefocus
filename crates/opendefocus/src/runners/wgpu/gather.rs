//! Adaptive gather dispatch on the WGPU runner: frame-level entry points, the shared implementation, and the integral subtree end computation.
/// Aperture texture and bounds upload.
mod aperture;
/// Bind group, pipeline selection and the gather pass.
mod dispatch;
/// Small shared GPU resources.
mod resources;
/// Input validation and limit checks.
mod validation;

use super::adaptive_gpu;
use super::disc_shift;
use super::{GatherParams, TileFeatures, WgpuRunner};
use crate::AdaptiveSampleBudget;
use crate::convolve_settings::{ConvolveSettings, GlobalFlags};
use crate::error::{Error, Result};
use crate::renders::resize::MipmapBuffer;
use crate::runners::adaptive::{
    PreparedLayers, adaptive_gather_tile_class, adaptive_layer_search_radii,
};
use dispatch::GatherBindings;
pub(super) use dispatch::GatherPipelines;
use ndarray::Array3;
use resources::{native_lens, source_is_opaque};
use validation::{capped_pixels_dominate, integral_trees};
impl WgpuRunner {
    /// Render caller-provided layers with the default Fast circular sample budget.
    /// Use the settings-aware entry point to supply custom aperture and lens settings.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid, or when a GPU resource cannot be created or read back.
    pub async fn gather_adaptive_frame(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
    ) -> Result<Array3<f32>> {
        self.gather_adaptive_frame_with_budget(
            source,
            prepared,
            features,
            max_radius_px,
            AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Fast),
        )
        .await
    }

    /// Gathers a frame with the given sample budget and no lens. Delegates to [`Self::gather_adaptive_frame_with_lens`].
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU work fails.
    pub async fn gather_adaptive_frame_with_budget(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
        sample_budget: AdaptiveSampleBudget,
    ) -> Result<Array3<f32>> {
        self.gather_adaptive_frame_with_lens(
            source,
            prepared,
            features,
            max_radius_px,
            sample_budget,
            None,
        )
        .await
    }

    /// Gathers a frame with an optional lens (`lens`), which carries the aperture and lens settings.
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU work fails.
    pub async fn gather_adaptive_frame_with_lens(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
        sample_budget: AdaptiveSampleBudget,
        lens: Option<&ConvolveSettings>,
    ) -> Result<Array3<f32>> {
        self.gather_adaptive_frame_impl(
            source,
            prepared,
            features,
            max_radius_px,
            sample_budget,
            lens,
            None,
            true,
        )
        .await
    }

    /// Gathers with an existing filter and aperture mip chain. Accurate quality
    /// uses aperture-aware integral traversal; lower qualities sample the filter.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid, or when a GPU resource cannot be created or read back.
    pub async fn gather_adaptive_frame_with_aperture(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
        sample_budget: AdaptiveSampleBudget,
        lens: &ConvolveSettings,
        aperture: &MipmapBuffer,
    ) -> Result<Array3<f32>> {
        self.gather_adaptive_frame_impl(
            source,
            prepared,
            features,
            max_radius_px,
            sample_budget,
            Some(lens),
            Some(aperture),
            true,
        )
        .await
    }

    /// Shared implementation of the adaptive gather. `fused_capped_circle` selects the fused plain-circle Accurate specialization when it applies.
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU work fails.
    pub(super) async fn gather_adaptive_frame_impl(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
        sample_budget: AdaptiveSampleBudget,
        lens: Option<&ConvolveSettings>,
        aperture: Option<&MipmapBuffer>,
        fused_capped_circle: bool,
    ) -> Result<Array3<f32>> {
        let use_integrals = sample_budget
            == AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate);
        let sample_offsets = self
            .adaptive_sample_offsets
            .as_ref()
            .ok_or(Error::AdaptiveClassifierUnavailable)?;
        let geometry = self.gather_geometry(source, prepared, features, max_radius_px, lens)?;
        let (width, height) = (geometry.width, geometry.height);
        let (near_nodes, far_nodes) =
            integral_trees(prepared, use_integrals, geometry.max_storage)?;
        // Both trees have the same preorder spatial topology; binding 16 stores
        // the exclusive subtree end for each preorder node.
        let subtree_ends = integral_subtree_ends(&near_nodes)?;
        let plain_circle = aperture.is_none()
            && !GlobalFlags::from_bits_retain(lens.map_or(0, |lens| lens.flags))
                .intersects(GlobalFlags::USE_NON_UNIFORM | GlobalFlags::AXIAL_ABERRATION_ENABLE);
        let use_fused_capped_circle = use_integrals
            && plain_circle
            && fused_capped_circle
            && capped_pixels_dominate(prepared, max_radius_px);

        let corrected_source = prepared.decontaminated_source(source);
        let gather_source = corrected_source.as_ref().unwrap_or(source);
        let source_data = gather_source.as_slice().ok_or(Error::NonContiguousArray)?;
        let adaptive_gpu::GatherTextures {
            textures:
                [
                    near_color_texture,
                    far_color_texture,
                    near_metadata_texture,
                    far_metadata_texture,
                ],
            max_mip,
        } = if use_integrals {
            self.resident_adaptive_base_textures(prepared)?
        } else {
            self.resident_adaptive_pyramid(prepared)?
        };
        let source_texture =
            self.upload_source_texture(width, height, bytemuck::cast_slice(source_data));
        let aperture_texture = aperture
            .map(|aperture| self.upload_aperture_texture(aperture, lens))
            .transpose()?;
        let aperture_bounds = match aperture {
            Some(aperture) if use_integrals => {
                self.upload_aperture_bounds(aperture, geometry.max_storage)?
            }
            _ => self.storage_buffer(
                "Circular aperture unused bounds",
                bytemuck::cast_slice(&[1.0_f32; 8]),
            ),
        };
        let aperture_max_mip = aperture.map_or(0.0, |aperture| {
            aperture.get_images_view().len().saturating_sub(1) as f32
        });

        let view_of =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        let source_view = view_of(&source_texture);
        let aperture_view = aperture_texture.as_ref().map(view_of);
        let (near_color_view, far_color_view) =
            (view_of(&near_color_texture), view_of(&far_color_texture));
        let (near_metadata_view, far_metadata_view) = (
            view_of(&near_metadata_texture),
            view_of(&far_metadata_texture),
        );
        let sampler = self.gather_sampler(aperture.is_some(), max_mip.max(aperture_max_mip));
        let classes: Vec<u32> = features.iter().map(adaptive_gather_tile_class).collect();
        let classes_buffer = self.storage_buffer(
            "Adaptive gather tile classes",
            bytemuck::cast_slice(&classes),
        );
        let search_radii =
            adaptive_layer_search_radii(prepared, max_radius_px, geometry.tile_origin);
        let search_radii_buffer = self.storage_buffer(
            "Adaptive gather packed near/far search radii",
            bytemuck::cast_slice(&search_radii),
        );
        let near_nodes_buffer = self.storage_buffer(
            "Adaptive near integral hierarchy",
            bytemuck::cast_slice(&near_nodes),
        );
        let far_nodes_buffer = self.storage_buffer(
            "Adaptive far integral hierarchy",
            bytemuck::cast_slice(&far_nodes),
        );
        let subtree_buffer = self.storage_buffer(
            "Adaptive integral subtree ends",
            bytemuck::cast_slice(&subtree_ends),
        );
        let disc_shift = disc_shift(lens);
        let params = self.uniform_buffer(
            "Adaptive gather parameters",
            bytemuck::bytes_of(&GatherParams {
                frame: [width, height, geometry.tiles_x, geometry.tiles_y],
                tile_origin: [geometry.tile_origin.x, geometry.tile_origin.y],
                source_opaque: u32::from(source_is_opaque(source)),
                _tile_padding: 0,
                samples: sample_budget.kernel_params(),
                radius: [max_radius_px, max_mip, disc_shift.x, disc_shift.y],
            }),
        );
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adaptive gathered output"),
            size: geometry.output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let lens_buffer = self.uniform_buffer(
            "Adaptive lens parameters",
            bytemuck::bytes_of(&native_lens(
                lens,
                width,
                height,
                max_radius_px,
                aperture.is_some(),
            )),
        );
        let bindings = GatherBindings {
            params: &params,
            sampler: &sampler,
            output: &output,
            source_view: &source_view,
            near_color_view: &near_color_view,
            far_color_view: &far_color_view,
            near_metadata_view: &near_metadata_view,
            far_metadata_view: &far_metadata_view,
            classes: &classes_buffer,
            sample_offsets,
            search_radii: &search_radii_buffer,
            lens: &lens_buffer,
            integral_nodes: [&near_nodes_buffer, &far_nodes_buffer],
            aperture_view: aperture_view.as_ref().unwrap_or(&source_view),
            aperture_bounds: &aperture_bounds,
            subtree_ends: &subtree_buffer,
        };
        let pipelines = self.gather_pipeline(use_integrals, plain_circle, use_fused_capped_circle);
        let gathered = self
            .run_gather(pipelines, &bindings, (width, height), geometry.output_size)
            .await?;
        let mut output = gathered.slice(ndarray::s![.., .., ..4]).to_owned();
        prepared.resolve_subpixel_focus(&mut output);
        Ok(output)
    }
}

/// Returns, for each packed node, the index just past its preorder subtree. A single empty root returns `[1]`.
/// # Errors
/// Returns `SizeLimitExceeded` when a node index does not fit in `u32`, and `InvalidDimensions` when a child index is out of range.
pub(crate) fn integral_subtree_ends(nodes: &[[u32; 16]]) -> Result<Vec<u32>> {
    if nodes == [[0u32; 16]] {
        return Ok(vec![1]);
    }
    let mut ends = vec![0u32; nodes.len()];
    for index in (0..nodes.len()).rev() {
        let mut end = u32::try_from(index + 1).map_err(|_| Error::SizeLimitExceeded)?;
        for &child in &nodes[index][8..12] {
            if child != u32::MAX && child & (1 << 31) == 0 {
                if child as usize <= index || child as usize >= nodes.len() {
                    return Err(Error::InvalidDimensions);
                }
                end = end.max(ends[child as usize]);
            }
        }
        ends[index] = end;
    }
    Ok(ends)
}
