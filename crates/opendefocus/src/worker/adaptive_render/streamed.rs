//! Exact astigmatic render for frames whose source context does not fit one block: the source is prepared as streamed chunks and each chunk's core is gathered from all of them.
use super::{
    ADAPTIVE_PREPARE_BYTES_PER_PIXEL, FOCUS_REPAIR_HALO, RenderContext, SourceFrame,
    blocks::pixels_per_budget,
    inputs::gpu_pixel_count,
    region::{contains, copy_region, grow_clamped, intersection, origin, region_size},
};
use crate::{
    datamodel::{IVector4, render::RenderSpecs, settings_to_convolve_settings},
    error::{Error, Result},
    runners::{
        StreamGather, StreamSource, TileFeatures,
        adaptive::{AdaptiveLayerPixel, resolve_subpixel_focus_with},
        adaptive_streaming::{
            ADAPTIVE_OUTPUT_BYTES_PER_PIXEL, AdaptiveSourceChunk, AdaptiveSourceRegion,
            plan_source_regions_with_core_limit, preparation_halo, prepare_source_chunks,
        },
    },
};
use bytemuck::Zeroable;
use ndarray::{Array3, ArrayViewMut3};

/// Source chunks of a streamed render with the tile classification computed from them.
struct PreparedStream {
    /// Prepared source partitions, each holding its own copy of the data it needs.
    chunks: Vec<AdaptiveSourceChunk>,
    /// Classifier features of the global tile grid.
    features: Vec<TileFeatures>,
    /// Origin and size of the global tile grid.
    tile_layout: (i32, i32, usize, usize),
}

/// Renders the requested region of the frame from streamed source chunks and writes it into `image`.
/// Consumes `source`: the chunks own the data the gathers need, so the full-frame inputs are released
/// before the output accumulator and the GPU gather resources are allocated.
///
/// # Errors
/// Returns an error when the chunks cannot be planned or prepared, or a gather fails.
pub(super) async fn render_streamed(
    context: &RenderContext<'_>,
    image: &mut ArrayViewMut3<'_, f32>,
    source: SourceFrame,
) -> Result<()> {
    let frame = &context.plan.frame;
    let (full, requested) = (frame.full, frame.requested);
    let regions = plan_stream_regions(context)?;
    let (chunks, features, tile_layout) = prepare_source_chunks(
        context.runner,
        &source.rgba,
        &source.coc,
        &full,
        &regions,
        context.plan.max_radius,
    )
    .await?;
    drop(source);
    let stream = PreparedStream {
        chunks,
        features,
        tile_layout,
    };
    let lens = settings_to_convolve_settings(
        context.settings,
        &RenderSpecs {
            full_region: full,
            render_region: requested,
        },
        context.filter_resolution,
        gpu_pixel_count(frame.width, frame.height)?,
    );
    let repair = grow_clamped(&requested, FOCUS_REPAIR_HALO, &full);
    let mut streamed = gather_region(context, &stream, &lens, &repair).await?;
    resolve_subpixel_focus_with(
        frame.width,
        frame.height,
        [(repair.x - full.x) as usize, (repair.y - full.y) as usize],
        |x, y| prepared_pixel_at(&stream.chunks, full.x + x as i32, full.y + y as i32),
        &mut streamed,
    );
    copy_region(
        image,
        origin(&full),
        &streamed.view(),
        origin(&repair),
        &requested,
        frame.channels,
    );
    Ok(())
}

/// Plans the source partitions: the classifier halo ignores the astigmatic warp because the warp is
/// handled by gathering across chunks, and the preparation halo covers the layer propagation reach.
///
/// # Errors
/// Returns an error when the halo does not fit the coordinate type or no partition fits the device.
fn plan_stream_regions(context: &RenderContext<'_>) -> Result<Vec<AdaptiveSourceRegion>> {
    let plan = context.plan;
    let mut unwarped = context.settings.clone();
    unwarped.non_uniform.astigmatism.enable = false;
    let classifier_halo = crate::adaptive_source_padding(&unwarped);
    let preparation_halo = preparation_halo(plan.max_radius)?;
    let halo = i32::try_from(classifier_halo.max(preparation_halo as u32))
        .map_err(|_| Error::SizeLimitExceeded)?;
    let max_source_pixels = pixels_per_budget(
        plan.limits.working_buffer_bytes,
        ADAPTIVE_PREPARE_BYTES_PER_PIXEL,
    )
    .min(plan.block_limit);
    let max_core_pixels = pixels_per_budget(
        plan.limits.working_buffer_bytes,
        ADAPTIVE_OUTPUT_BYTES_PER_PIXEL,
    );
    plan_source_regions_with_core_limit(
        &plan.frame.full,
        halo,
        max_source_pixels,
        max_core_pixels,
        plan.limits.max_dimension,
    )
}

/// Gathers `region` chunk by chunk and returns it as an RGBA array. The GPU frame cache lives only for
/// this call, so its resources are released before the CPU focus repair runs.
///
/// # Errors
/// Returns an error when a gather fails or returns an unexpected size.
async fn gather_region(
    context: &RenderContext<'_>,
    stream: &PreparedStream,
    lens: &crate::convolve_settings::ConvolveSettings,
    region: &IVector4,
) -> Result<Array3<f32>> {
    let plan = context.plan;
    let (width, height) = region_size(region)?;
    let mut gathered = Array3::zeros((height, width, 4));
    let mut cache = context.runner.new_stream_frame_cache(&stream.chunks);
    for (index, chunk) in stream.chunks.iter().enumerate() {
        let Some(copy) = intersection(&chunk.region.core, region) else {
            continue;
        };
        let output = context
            .runner
            .gather_adaptive_streamed_high(
                &StreamSource {
                    chunks: &stream.chunks,
                    features: &stream.features,
                    tile_layout: stream.tile_layout,
                },
                &mut cache,
                index,
                &plan.frame.full,
                &copy,
                &StreamGather {
                    max_radius_px: plan.max_radius,
                    sample_budget: plan.budget,
                    lens,
                    aperture: context.aperture,
                },
            )
            .await?;
        let (copy_width, copy_height) = region_size(&copy)?;
        if output.dim() != (copy_height, copy_width, 4) {
            return Err(Error::InvalidDimensions);
        }
        copy_region(
            &mut gathered.view_mut(),
            origin(region),
            &output.view(),
            origin(&copy),
            &copy,
            4,
        );
    }
    Ok(gathered)
}

/// Returns the prepared layer pixel at global position `(x, y)`, or a zero pixel outside every chunk core.
fn prepared_pixel_at(chunks: &[AdaptiveSourceChunk], x: i32, y: i32) -> AdaptiveLayerPixel {
    chunks
        .iter()
        .filter(|chunk| contains(&chunk.region.core, x, y))
        .find_map(|chunk| {
            let core = chunk.region.core;
            let column = (x - core.x) as usize;
            let row = (y - core.y) as usize;
            chunk
                .prepared
                .pixels
                .get(row * chunk.prepared.width + column)
                .copied()
        })
        .unwrap_or_else(AdaptiveLayerPixel::zeroed)
}
