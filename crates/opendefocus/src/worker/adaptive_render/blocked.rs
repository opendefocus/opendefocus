//! Block-wise render: the requested region is cut into globally aligned cores, and each core is rendered from its own padded source context.
use super::{
    FOCUS_REPAIR_HALO, RenderContext, SourceFrame,
    blocks::{AdaptiveBlockPlan, align_down},
    inputs::gpu_pixel_count,
    region::{copy_region, grow_clamped, origin, region_size, window},
};
use crate::{
    datamodel::{IVector4, render::RenderSpecs, settings_to_convolve_settings},
    error::{Error, Result},
    runners::{GATHER_OUTPUT_CHANNELS, adaptive::CocPolarity},
};
use ndarray::{Array3, ArrayViewMut3, s};

/// Returns the `(start, end)` spans along one axis that cover `start..end` in steps of `size`.
/// Spans are aligned to multiples of `size`, so the first span may begin before `start`.
fn core_spans(start: i32, end: i32, size: i32) -> Vec<(i32, i32)> {
    let mut spans = Vec::new();
    let mut span_start = align_down(start, size);
    while span_start < end {
        let span_end = span_start.saturating_add(size).min(end);
        spans.push((span_start, span_end));
        span_start = span_end;
    }
    spans
}

/// Renders the requested region of the frame block by block and writes it into `image`.
///
/// # Errors
/// Returns an error when a block fails to classify, prepare or gather.
pub(super) async fn render_blocks(
    context: &RenderContext<'_>,
    blocks: AdaptiveBlockPlan,
    image: &mut ArrayViewMut3<'_, f32>,
    source: &SourceFrame,
) -> Result<()> {
    let frame = &context.plan.frame;
    let (full, requested) = (frame.full, frame.requested);
    for (core_y, core_y_end) in core_spans(requested.y, requested.w, blocks.core_height) {
        for (core_x, core_x_end) in core_spans(requested.x, requested.z, blocks.core_width) {
            let core = IVector4 {
                x: core_x,
                y: core_y,
                z: core_x_end,
                w: core_y_end,
            };
            let block = if blocks.single_pass {
                full
            } else {
                grow_clamped(&core, context.plan.halo, &full)
            };
            let render_region = IVector4 {
                x: core_x.max(requested.x),
                y: core_y.max(requested.y),
                z: core_x_end.min(requested.z),
                w: core_y_end.min(requested.w),
            };
            let output = render_block(context, source, &block, &render_region).await?;
            copy_region(
                image,
                origin(&full),
                &output.view(),
                origin(&block),
                &render_region,
                frame.channels,
            );
        }
    }
    Ok(())
}

/// Renders `block`, a core plus its halo, and returns the result for the whole block: RGBA and the front-layer
/// transmittance (see [`GATHER_OUTPUT_CHANNELS`]). `render_region` is the part of the block that is kept.
///
/// # Errors
/// Returns an error when classification, preparation or the gather fails, or the gather returns an unexpected size.
async fn render_block(
    context: &RenderContext<'_>,
    source: &SourceFrame,
    block: &IVector4,
    render_region: &IVector4,
) -> Result<Array3<f32>> {
    let plan = context.plan;
    let (rows, columns) = window(block, origin(&plan.frame.full));
    let block_rgba = source
        .rgba
        .slice(s![rows.clone(), columns.clone(), ..])
        .to_owned();
    let block_coc = source.coc.slice(s![rows, columns]).to_owned();
    let features = context
        .runner
        .classify_tiles(&block_rgba, &block_coc, [block.x, block.y], plan.max_radius)
        .await?;
    let prepared = context
        .runner
        .prepare_adaptive_layers(
            &block_rgba,
            &block_coc,
            plan.max_radius,
            CocPolarity::PositiveForeground,
        )
        .await?;
    let (block_width, block_height) = region_size(block)?;
    // The block's absolute bounds are the gather origin, while the lens centre stays at its screen-space position.
    let block_specs = RenderSpecs {
        full_region: *block,
        render_region: *render_region,
    };
    let mut lens = settings_to_convolve_settings(
        context.settings,
        &block_specs,
        context.filter_resolution,
        gpu_pixel_count(block_width, block_height)?,
    );
    let process = grow_clamped(render_region, FOCUS_REPAIR_HALO, block);
    lens.process_region = glam::IVec4::new(process.x, process.y, process.z, process.w);
    let output = context
        .runner
        .gather_adaptive_frame_with_transmittance(
            &block_rgba,
            &prepared,
            &features,
            plan.max_radius,
            plan.budget,
            &lens,
            context.aperture,
        )
        .await?;
    if output.dim() != (block_height, block_width, GATHER_OUTPUT_CHANNELS) {
        return Err(Error::InvalidDimensions);
    }
    Ok(output)
}

#[cfg(test)]
/// Tests for the core span iteration.
mod tests {
    use super::*;

    #[test]
    /// Verifies spans are aligned to the grid, clipped to the end and contiguous.
    fn spans_align_to_the_grid_and_clip_to_the_end() {
        assert_eq!(core_spans(5, 40, 16), vec![(0, 16), (16, 32), (32, 40)]);
        assert_eq!(core_spans(16, 32, 16), vec![(16, 32)]);
        assert_eq!(core_spans(-3, 3, 16), vec![(-16, 0), (0, 3)]);
    }

    #[test]
    /// Verifies the unlimited core size yields one span from the start to the end.
    fn unlimited_core_gives_one_span() {
        assert_eq!(core_spans(7, 90, i32::MAX), vec![(7, 90)]);
    }
}
