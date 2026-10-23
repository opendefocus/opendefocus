//! Validates the render request and decides how the frame is rendered: region geometry, channel count, halo, block limit and whether blocks or the astigmatic stream are used.
use super::{
    ADAPTIVE_PREPARE_BYTES_PER_PIXEL,
    blocks::{
        AdaptiveBlockPlan, adaptive_block_pixel_limit, adaptive_gpu_working_buffer_bytes,
        pixels_per_budget, plan_adaptive_blocks,
    },
    region::region_size,
};
use crate::{
    AdaptiveSampleBudget,
    datamodel::{
        IVector4, Settings,
        render::{Quality, RenderSpecs},
    },
    error::{Error, Result},
    runners::WgpuRunner,
};

/// Largest number of channels a caller image may have.
const MAX_IMAGE_CHANNELS: usize = 4;

/// Validated geometry of one render request: the caller image and the two regions it is rendered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FrameGeometry {
    /// Image width in pixels, equal to the full region width.
    pub(super) width: usize,
    /// Image height in pixels, equal to the full region height.
    pub(super) height: usize,
    /// Number of channels of the caller image, 1 to 4.
    pub(super) channels: usize,
    /// Region the caller image covers, in global coordinates.
    pub(super) full: IVector4,
    /// Region that is written, inside `full`.
    pub(super) requested: IVector4,
}

impl FrameGeometry {
    /// Checks the regions against the caller image dimensions `(height, width, channels)`.
    ///
    /// # Errors
    /// Returns `InvalidDimensions` when a region is empty, the image does not match the full region,
    /// the render region leaves the full region, or the channel count is not 1 to 4.
    pub(super) fn new(specs: &RenderSpecs, image_dim: (usize, usize, usize)) -> Result<Self> {
        let (height, width, channels) = image_dim;
        let (full, requested) = (specs.full_region, specs.render_region);
        let full_size = region_size(&full)?;
        region_size(&requested)?;
        let requested_inside_full = requested.x >= full.x
            && requested.y >= full.y
            && requested.z <= full.z
            && requested.w <= full.w;
        if channels == 0
            || channels > MAX_IMAGE_CHANNELS
            || full_size != (width, height)
            || !requested_inside_full
        {
            return Err(Error::InvalidDimensions);
        }
        Ok(Self {
            width,
            height,
            channels,
            full,
            requested,
        })
    }
}

/// Device limits that bound the size of a render operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeviceLimits {
    /// Largest texture edge in pixels.
    pub(super) max_dimension: usize,
    /// Largest number of source pixels one block may hold.
    pub(super) block_pixels: usize,
    /// Largest byte size of one streamed working buffer.
    pub(super) working_buffer_bytes: u64,
}

impl DeviceLimits {
    /// Reads the limits of the device behind `runner`.
    pub(super) fn of(runner: &WgpuRunner) -> Self {
        Self {
            max_dimension: runner.device.limits().max_texture_dimension_2d as usize,
            block_pixels: adaptive_block_pixel_limit(runner),
            working_buffer_bytes: adaptive_gpu_working_buffer_bytes(runner),
        }
    }
}

/// How the requested region is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Strategy {
    /// Globally aligned output blocks, each rendered from its own source context.
    Blocks(AdaptiveBlockPlan),
    /// Exact astigmatic render from streamed source chunks, used when no block plan fits the device.
    Streamed,
}

/// Everything the executors need to know about a render, decided before any large buffer is allocated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RenderPlan {
    /// Validated frame geometry.
    pub(super) frame: FrameGeometry,
    /// Maximum CoC radius in pixels.
    pub(super) max_radius: f32,
    /// Sample budget of the quality setting.
    pub(super) budget: AdaptiveSampleBudget,
    /// Source context in pixels around each block.
    pub(super) halo: i32,
    /// Largest number of source pixels one operation may hold.
    pub(super) block_limit: usize,
    /// Device limits the plan was made for.
    pub(super) limits: DeviceLimits,
    /// Chosen way to render.
    pub(super) strategy: Strategy,
}

/// Returns true when the settings request a non-zero astigmatic warp.
pub(super) fn astigmatism_active(settings: &Settings) -> bool {
    settings.non_uniform.astigmatism.enable && settings.non_uniform.astigmatism.amount > 0.0
}

/// Returns the source context in pixels each block needs around its core.
/// Astigmatism can reach any source pixel, so it needs the whole frame.
///
/// # Errors
/// Returns `AdaptiveSettingsUnsupported` when the halo does not fit the coordinate type.
fn block_halo(settings: &Settings, frame: &FrameGeometry) -> Result<i32> {
    let padding = crate::adaptive_source_padding(settings);
    let padding = if astigmatism_active(settings) {
        padding.max(frame.width.max(frame.height) as u32)
    } else {
        padding
    };
    i32::try_from(padding).map_err(|_| {
        Error::AdaptiveSettingsUnsupported("Adaptive block halo exceeds supported coordinates")
    })
}

/// Chooses how to render `frame` on a device with `limits`, with at most `max_block_pixels` source pixels per block.
///
/// # Errors
/// Returns `SizeLimitExceeded` when no block fits and the astigmatic stream does not apply,
/// or any error of the block planner.
pub(super) fn plan_render(
    frame: FrameGeometry,
    settings: &Settings,
    max_radius: f32,
    limits: DeviceLimits,
    max_block_pixels: usize,
) -> Result<RenderPlan> {
    let budget = AdaptiveSampleBudget::for_quality(settings.render.get_quality());
    let exact_accurate_gpu = budget == AdaptiveSampleBudget::for_quality(Quality::Accurate);
    let preparation_limit = if exact_accurate_gpu {
        pixels_per_budget(
            limits.working_buffer_bytes,
            ADAPTIVE_PREPARE_BYTES_PER_PIXEL,
        )
    } else {
        usize::MAX
    };
    let block_limit = max_block_pixels
        .min(limits.block_pixels)
        .min(preparation_limit);
    if block_limit == 0 {
        return Err(Error::SizeLimitExceeded);
    }
    let halo = block_halo(settings, &frame)?;
    let strategy = match plan_adaptive_blocks(
        &frame.full,
        &frame.requested,
        halo,
        block_limit,
        limits.max_dimension,
    ) {
        Ok(blocks) => Strategy::Blocks(blocks),
        Err(_) if exact_accurate_gpu && astigmatism_active(settings) => Strategy::Streamed,
        Err(error) => return Err(error),
    };
    Ok(RenderPlan {
        frame,
        max_radius,
        budget,
        halo,
        block_limit,
        limits,
        strategy,
    })
}

#[cfg(test)]
/// Tests for request validation and strategy selection, which need no GPU.
mod tests {
    use super::*;
    use crate::datamodel::defocus::DefocusMode;

    /// Render specs with the given full and render rectangles.
    fn specs(full: [i32; 4], render: [i32; 4]) -> RenderSpecs {
        RenderSpecs::from_rects(glam::IVec4::from(full), glam::IVec4::from(render))
    }

    /// Limits of a generous device.
    fn roomy_limits() -> DeviceLimits {
        DeviceLimits {
            max_dimension: 16_384,
            block_pixels: usize::MAX,
            working_buffer_bytes: 512 * 1024 * 1024,
        }
    }

    /// Settings of the exact Accurate budget with an astigmatic warp.
    fn astigmatic_high() -> Settings {
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.render.set_quality(Quality::Accurate);
        settings.render.set_farm_quality(Quality::Accurate);
        settings.non_uniform.astigmatism.enable = true;
        settings.non_uniform.astigmatism.amount = 2.0;
        settings
    }

    #[test]
    /// Verifies the geometry accepts a matching image and a render region inside the full region.
    fn geometry_accepts_matching_regions() {
        let geometry = FrameGeometry::new(&specs([0, 0, 8, 6], [2, 1, 5, 4]), (6, 8, 3)).unwrap();
        assert_eq!(
            (geometry.width, geometry.height, geometry.channels),
            (8, 6, 3)
        );
    }

    #[test]
    /// Verifies every geometry violation is reported as invalid dimensions.
    fn geometry_rejects_invalid_requests() {
        let full = [0, 0, 8, 6];
        let cases = [
            (specs([0, 0, 0, 6], full), (6, 8, 4)),
            (specs(full, [3, 3, 3, 5]), (6, 8, 4)),
            (specs(full, full), (6, 7, 4)),
            (specs(full, full), (5, 8, 4)),
            (specs(full, full), (6, 8, 0)),
            (specs(full, full), (6, 8, 5)),
            (specs(full, [-1, 0, 8, 6]), (6, 8, 4)),
            (specs(full, [0, 0, 9, 6]), (6, 8, 4)),
            (specs(full, [0, 0, 8, 7]), (6, 8, 4)),
            (specs(full, [0, -1, 8, 6]), (6, 8, 4)),
        ];
        for (case, dim) in cases {
            assert!(
                matches!(
                    FrameGeometry::new(&case, dim),
                    Err(Error::InvalidDimensions)
                ),
                "{case:?} {dim:?}"
            );
        }
    }

    #[test]
    /// Verifies a frame that fits the limits renders in one block pass.
    fn small_frames_plan_a_single_block() {
        let frame =
            FrameGeometry::new(&specs([0, 0, 64, 64], [0, 0, 64, 64]), (64, 64, 4)).unwrap();
        let plan =
            plan_render(frame, &Settings::default(), 8.0, roomy_limits(), usize::MAX).unwrap();
        assert!(matches!(plan.strategy, Strategy::Blocks(blocks) if blocks.single_pass));
    }

    #[test]
    /// Verifies a zero block limit is rejected before any planning.
    fn zero_block_limit_is_a_size_error() {
        let frame =
            FrameGeometry::new(&specs([0, 0, 64, 64], [0, 0, 64, 64]), (64, 64, 4)).unwrap();
        let result = plan_render(frame, &Settings::default(), 8.0, roomy_limits(), 0);
        assert!(matches!(result, Err(Error::SizeLimitExceeded)));
    }

    #[test]
    /// Verifies an astigmatic exact render that cannot fit any block falls back to the stream,
    /// while a plain render with the same limit reports the size error.
    fn impossible_blocks_stream_only_for_astigmatic_high() {
        let frame =
            FrameGeometry::new(&specs([0, 0, 120, 100], [40, 40, 60, 60]), (100, 120, 4)).unwrap();
        let streamed = plan_render(frame, &astigmatic_high(), 8.0, roomy_limits(), 16).unwrap();
        assert_eq!(streamed.strategy, Strategy::Streamed);

        let mut plain = astigmatic_high();
        plain.non_uniform.astigmatism.enable = false;
        let result = plan_render(frame, &plain, 8.0, roomy_limits(), 16);
        assert!(matches!(result, Err(Error::SizeLimitExceeded)));
    }
}
