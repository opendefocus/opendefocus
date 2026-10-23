//! Crate root for the OpenDefocus renderer. Declares the internal modules and re-exports the adaptive
//! API, along with the sample-budget and CoC helpers used when a render is prepared.
#![warn(unused_extern_crates)]
#![doc=include_str!("../README.md")]
#![warn(clippy::print_stdout, clippy::print_stderr)]

/// Process-wide abort flag that long-running renders poll.
pub mod abort;
/// Error type and `Result` alias returned by the public API.
mod error;
/// Embedded WGSL shader sources and the kernel constants that name them.
mod kernel;
/// Public renderer object and its render entry points.
mod renderer;
/// Host-side preparation of filters, image resizing and mip chains.
mod renders;
/// Render backends: the WGPU runner and the adaptive pipeline stages.
mod runners;
#[cfg(test)]
/// Test modules for the crate.
mod tests;
#[cfg(feature = "wgpu")]
/// Settings validation for the adaptive renderer.
mod validation;
/// Background workers that execute adaptive blocks and previews.
mod worker;

#[cfg(feature = "wgpu")]
pub use runners::TileFeatures;
#[cfg(feature = "wgpu")]
pub use runners::adaptive;
#[cfg(feature = "wgpu")]
pub use runners::adaptive::{
    AdaptiveLayerPixel, AdaptiveSourcePyramid, CocPolarity, PreparedLayers,
};

#[cfg(feature = "wgpu")]
pub use validation::validate_adaptive_settings;
/// GPU convolution uniform, flag sets and sample weights.
mod convolve_settings;

/// Exported datastructure containing all settings for configuring the convolution
pub mod datamodel;

use crate::kernel::{
    ACCURATE_DETAIL_OFFSET, ACCURATE_DETAIL_POINTS, ACCURATE_SMOOTH_OFFSET, ACCURATE_SMOOTH_POINTS,
    FAST_DETAIL_OFFSET, FAST_DETAIL_POINTS, FAST_SMOOTH_OFFSET, FAST_SMOOTH_POINTS,
};
#[cfg(feature = "wgpu")]
use crate::runners::WgpuRunner;
use datamodel::render::{FilterMode, Quality};
use ndarray::Array2;

/// Returns whether the settings request the generated bokeh filter preview: the preview flag is set and the filter mode is `BokehCreator`.
pub(crate) fn is_bokeh_filter_preview(settings: &datamodel::Settings) -> bool {
    settings.render.filter.preview
        && FilterMode::try_from(settings.render.filter.mode).ok() == Some(FilterMode::BokehCreator)
}

/// Per-tile sample counts for the adaptive gather.
///
/// Fast selects sampled gathers. The Accurate budget identifies exact
/// source-pixel integration; its nominal counts are diagnostic metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveSampleBudget {
    /// Samples per pixel for smooth tiles (low-detail regions).
    smooth: u32,
    /// Samples per pixel for detail tiles (high-frequency regions).
    detail: u32,
    /// Offset of the smooth sample pattern in the shared gather offset table.
    smooth_offset: u32,
    /// Offset of the detail sample pattern in the shared gather offset table.
    detail_offset: u32,
}

impl AdaptiveSampleBudget {
    /// Select the fixed adaptive gather budget for a renderer quality.
    #[must_use]
    pub const fn for_quality(quality: Quality) -> Self {
        match quality {
            Quality::Fast => Self {
                smooth: FAST_SMOOTH_POINTS,
                detail: FAST_DETAIL_POINTS,
                smooth_offset: FAST_SMOOTH_OFFSET,
                detail_offset: FAST_DETAIL_OFFSET,
            },
            Quality::Accurate => Self {
                smooth: ACCURATE_SMOOTH_POINTS,
                detail: ACCURATE_DETAIL_POINTS,
                smooth_offset: ACCURATE_SMOOTH_OFFSET,
                detail_offset: ACCURATE_DETAIL_OFFSET,
            },
        }
    }

    /// Smooth-tile sample count, also used as the diagnostic report value.
    #[must_use]
    pub const fn smooth_samples(self) -> u32 {
        self.smooth
    }

    /// Detail-tile sample count, also used as the diagnostic report value.
    #[must_use]
    pub const fn detail_samples(self) -> u32 {
        self.detail
    }

    /// Packs the budget as the four `u32` values the GPU kernel reads, in the order `[smooth, detail, smooth_offset, detail_offset]`.
    pub(crate) const fn kernel_params(self) -> [u32; 4] {
        [
            self.smooth,
            self.detail,
            self.smooth_offset,
            self.detail_offset,
        ]
    }
}

#[cfg(feature = "wgpu")]
/// Returns the filter box size in pixels as `(width, height)` for the current filter and bokeh aspect ratio.
fn adaptive_filter_resolution(settings: &datamodel::Settings) -> glam::UVec2 {
    let bounds = settings
        .render
        .filter
        .calculate_filter_box(settings.bokeh.aspect_ratio);
    glam::UVec2::new(bounds[2] - bounds[0], bounds[3] - bounds[1])
}

/// Largest accepted effective defocus radius in pixels.
const MAX_EFFECTIVE_RADIUS: f32 = 16384.0;

#[derive(Debug, Clone)]
/// `OpenDefocus` rendering instance that stores the device configuration
pub struct OpenDefocusRenderer {
    #[cfg(feature = "wgpu")]
    /// WGPU runner that performs the adaptive GPU rendering for this renderer.
    runner: WgpuRunner,
}

// Astigmatism can fold remote aperture coordinates back into support. A radius
// halo cannot conservatively bound that warp; preserve the complete frame.
/// Returns the pristine source context in pixels needed around a render region: twice the aperture padding plus 16, widened for axial aberration.
/// With astigmatism it is at least the larger resolution side, because astigmatism can fold distant aperture coordinates back into support.
pub(crate) fn adaptive_source_padding(settings: &datamodel::Settings) -> u32 {
    let mut aperture_padding = settings.defocus.get_padding();
    if settings.non_uniform.axial_aberration.enable {
        // Large axial amounts can reverse and enlarge a color channel's CoC.
        let scale = (1.0 - settings.non_uniform.axial_aberration.amount.abs() * 0.25)
            .abs()
            .max(1.0);
        aperture_padding = (aperture_padding as f64 * scale as f64).ceil() as u32;
    }
    let padding = aperture_padding.saturating_mul(2).saturating_add(16);
    if settings.non_uniform.astigmatism.enable && settings.non_uniform.astigmatism.amount > 0.0 {
        padding.max(
            settings
                .render
                .resolution
                .x
                .max(settings.render.resolution.y),
        )
    } else {
        padding
    }
}

/// Prepare the `CoC` input used by the adaptive path with the same depth
/// conversion and 2D constant as the established renderer. Camera-derived
/// values use the legacy-compatible sign: positive is foreground and negative
/// is background. The gather applies its configured maximum reach separately.
/// In 2D mode this returns the synthetic 1x1 constant-CoC plane; the public
/// rendering entry point should pass it through layer preparation unchanged.
#[must_use]
pub fn adaptive_coc_from_depth(depth: &Array2<f32>, settings: &datamodel::Settings) -> Array2<f32> {
    if settings.defocus.defocus_mode() == datamodel::defocus::DefocusMode::Twod {
        return Array2::from_elem((1, 1), settings.defocus.get_size());
    }

    if settings.defocus.use_direct_math {
        return depth.clone();
    }

    let mut coc_settings = settings.defocus.circle_of_confusion;
    coc_settings.max_size = settings.defocus.get_max_size();
    coc_settings.size = settings.defocus.get_size();
    if settings.defocus.use_camera_focal
        && let Some(camera_data) = coc_settings.camera_data.as_mut()
    {
        camera_data.resolution = datamodel::circle_of_confusion::Resolution {
            width: settings.render.resolution.x,
            height: settings.render.resolution.y,
        };
    }
    coc_settings.math = match coc_settings.math() {
        datamodel::circle_of_confusion::Math::OneDividedByZ => {
            datamodel::circle_of_confusion::Math::OneDividedByZ.into()
        }
        datamodel::circle_of_confusion::Math::Real => {
            datamodel::circle_of_confusion::Math::Real.into()
        }
    };
    let calculator = datamodel::circle_of_confusion::Calculator::new(coc_settings);
    depth.mapv(|value| -datamodel::circle_of_confusion::calculate(&calculator, value))
}
