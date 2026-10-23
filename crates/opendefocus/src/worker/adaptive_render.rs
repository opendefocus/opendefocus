//! Renders a frame through the adaptive pipeline, splitting it into blocks when the frame exceeds the per-operation budget.
#[cfg(test)]
mod block_tests;
/// Block-wise executor: cores with halo, rendered one after the other.
mod blocked;
/// Block planning helpers for the adaptive renderer.
mod blocks;
/// Half-resolution path of the Fast quality.
mod half;
/// Preparation of the CoC, aperture and RGBA inputs.
mod inputs;
/// Request validation and the choice between blocks and the astigmatic stream.
mod plan;
/// Rectangle maths and window copies.
mod region;
#[cfg(test)]
/// Tests for the adaptive render path.
mod render_tests;
mod scatter;
/// Streamed executor for astigmatic frames that exceed the block limits.
mod streamed;

use crate::datamodel::{Settings, render::RenderSpecs};
use crate::{
    error::Result, is_bokeh_filter_preview, renders::resize::MipmapBuffer, runners::WgpuRunner,
    validate_adaptive_settings, worker::adaptive_preview::render_adaptive_preview,
};
use blocked::render_blocks;
use blocks::adaptive_block_pixel_limit;
use glam::UVec2;
use inputs::{
    aperture_resolution, bound_coc, build_aperture, coc_from_depth, rgba_from_image,
    validated_max_radius,
};
use ndarray::{Array2, Array3, ArrayViewMut3};
use plan::{DeviceLimits, FrameGeometry, RenderPlan, Strategy, plan_render};
use streamed::render_streamed;

/// Bytes of GPU storage the preparation stage needs per source pixel: its largest per-pixel binding, 13 floats.
const ADAPTIVE_PREPARE_BYTES_PER_PIXEL: u64 = 13 * 4;
/// Pixel count of the CPU reference frame used by block-split tests.
#[cfg(test)]
const CPU_REFERENCE_FRAME_PIXELS: usize = 2_400_000;
/// Edge length in pixels of a classifier and gather tile. Block cores align to this grid.
const ADAPTIVE_TILE_SIZE: i32 = 16;
/// Halo in pixels around a region whose focus is repaired. Focus repair writes up to three pixels from a
/// crossing and reads its endpoints four pixels away, so those neighbours are gathered too.
const FOCUS_REPAIR_HALO: i32 = 7;

/// Read-only state shared by the block and streamed executors.
struct RenderContext<'a> {
    /// Runner that owns the device.
    runner: &'a WgpuRunner,
    /// Validated render settings.
    settings: &'a Settings,
    /// Decisions made before rendering.
    plan: &'a RenderPlan,
    /// Aperture texture with mips, or `None` for the simple disc.
    aperture: Option<&'a MipmapBuffer>,
    /// Aperture resolution passed to the lens settings.
    filter_resolution: UVec2,
}

/// Full-frame source data an executor renders from.
struct SourceFrame {
    /// Source image as RGBA.
    rgba: Array3<f32>,
    /// CoC clamped to the maximum radius.
    coc: Array2<f32>,
}

/// Render a frame through the shared adaptive pipeline. `image` covers
/// `render_specs.full_region`; only `render_specs.render_region` is written.
///
/// # Errors
/// Returns an error when the render inputs are invalid, a block cannot be planned, or GPU work fails.
pub(crate) async fn render(
    runner: &WgpuRunner,
    render_specs: &RenderSpecs,
    settings: &Settings,
    image: ArrayViewMut3<'_, f32>,
    depth: Option<Array2<f32>>,
    filter: Option<Array3<f32>>,
) -> Result<()> {
    render_with_block_limit(
        runner,
        render_specs,
        settings,
        image,
        depth,
        filter,
        adaptive_block_pixel_limit(runner),
    )
    .await
}

/// Renders with an explicit per-block pixel limit. [`render`] passes the device limit, so smaller limits split the frame into more blocks.
/// # Errors
/// Returns an error when the render inputs are invalid or GPU work fails.
async fn render_with_block_limit(
    runner: &WgpuRunner,
    render_specs: &RenderSpecs,
    settings: &Settings,
    mut image: ArrayViewMut3<'_, f32>,
    depth: Option<Array2<f32>>,
    filter: Option<Array3<f32>>,
    max_block_pixels: usize,
) -> Result<()> {
    if is_bokeh_filter_preview(settings) {
        render_adaptive_preview(runner, settings, image.view_mut(), None).await?;
        return Ok(());
    }
    validate_adaptive_settings(settings)?;
    let frame = FrameGeometry::new(render_specs, image.dim())?;
    let rgba = rgba_from_image(&image.view())?;
    let coc = coc_from_depth(depth, settings, &frame)?;
    if render_adaptive_preview(runner, settings, image.view_mut(), Some(coc.view())).await? {
        return Ok(());
    }
    let source = SourceFrame { rgba, coc };
    if half::applies(settings, &frame, &source.coc) {
        return half::render_half(
            runner,
            settings,
            frame,
            &mut image,
            source,
            filter,
            max_block_pixels,
        )
        .await;
    }
    render_planned(
        runner,
        settings,
        frame,
        &mut image,
        source,
        filter,
        max_block_pixels,
    )
    .await
}

/// Plans and renders `source` at its own resolution, writing the requested region of `frame` into `image`.
///
/// # Errors
/// Returns an error when the settings are invalid, a block cannot be planned, or GPU work fails.
async fn render_planned(
    runner: &WgpuRunner,
    settings: &Settings,
    frame: FrameGeometry,
    image: &mut ArrayViewMut3<'_, f32>,
    source: SourceFrame,
    filter: Option<Array3<f32>>,
    max_block_pixels: usize,
) -> Result<()> {
    let max_radius = validated_max_radius(settings)?;
    let source = SourceFrame {
        coc: bound_coc(source.coc, max_radius),
        ..source
    };
    let limits = DeviceLimits::of(runner);
    let plan = plan_render(frame, settings, max_radius, limits, max_block_pixels)?;
    let aperture = build_aperture(filter, settings)?;
    let context = RenderContext {
        runner,
        settings,
        plan: &plan,
        aperture: aperture.as_ref(),
        filter_resolution: aperture_resolution(aperture.as_ref(), settings),
    };
    match plan.strategy {
        Strategy::Streamed => render_streamed(&context, image, source).await,
        Strategy::Blocks(blocks) => render_blocks(&context, blocks, image, &source).await,
    }
}
