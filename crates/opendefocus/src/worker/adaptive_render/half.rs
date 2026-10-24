//! Half-resolution path of the Fast quality: the frame is rendered at half size and upsampled, and the source detail
//! the half size lost is added back to pixels that are in focus and not under a blurred foreground.
use super::{
    SourceFrame, inputs::validated_max_radius, plan::FrameGeometry, region::origin, render_planned,
    scatter,
};
use crate::datamodel::{
    IVector4, Settings, UVector2, Vector2,
    render::{Quality, RenderSpecs},
};
use crate::{
    error::Result,
    runners::{GATHER_OUTPUT_CHANNELS, WgpuRunner},
};
use ndarray::{Array2, Array3, ArrayViewMut2, ArrayViewMut3, Axis, s};

/// Smallest side in pixels of the frame buffer that is worth rendering at half size. The frame, not the requested region, decides so that every stripe or tile of one image takes the same path.
const MIN_SIDE: i32 = 64;
/// CoC magnitude in pixels below which a pixel is considered in focus.
const FOCUS_BELOW: f32 = 0.5;
/// CoC magnitude in pixels above which the half-resolution result replaces the source.
const BLUR_ABOVE: f32 = 2.0;
/// Relative change of the half-resolution render below which a pixel keeps its source detail.
const CHANGE_BELOW: f32 = 0.03;
/// Relative change of the half-resolution render above which the render replaces the source.
const CHANGE_ABOVE: f32 = 0.2;
/// Channel of the half-size render that holds the front-layer transmittance of the sampled gather.
const FRONT_TRANSMITTANCE: usize = 4;

/// Returns whether the frame should be rendered at half size: Fast quality, a large enough frame, and some pixel that is out of focus.
pub(super) fn applies(settings: &Settings, frame: &FrameGeometry, coc: &Array2<f32>) -> bool {
    settings.render.get_quality() == Quality::Fast
        && frame.width >= MIN_SIDE as usize
        && frame.height >= MIN_SIDE as usize
        && coc.iter().any(|value| value.abs() > FOCUS_BELOW)
}

/// Returns `x` scaled to `0..=1` between the two edges, with a smooth ramp.
pub(super) fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Returns the settings for a render at half size: the lens centre, resolution and blur size are halved.
fn half_settings(settings: &Settings) -> Settings {
    let mut half = settings.clone();
    half.defocus.proxy_scale = Some(settings.defocus.proxy_scale.unwrap_or(1.0) * 0.5);
    half.render.resolution = UVector2 {
        x: settings.render.resolution.x.div_ceil(2),
        y: settings.render.resolution.y.div_ceil(2),
    };
    half.render.center = Vector2 {
        x: settings.render.center.x * 0.5,
        y: settings.render.center.y * 0.5,
    };
    half
}

/// Returns `region` grown by one pixel on every side and clipped to `bounds`.
fn grow_by_one(region: &IVector4, bounds: &IVector4) -> IVector4 {
    IVector4 {
        x: (region.x - 1).max(bounds.x),
        y: (region.y - 1).max(bounds.y),
        z: (region.z + 1).min(bounds.z),
        w: (region.w + 1).min(bounds.w),
    }
}

/// Averages each 2x2 block of global pixels into one half-size pixel, spreading the rows over the available threads. Blocks that leave the frame repeat its edge pixels.
/// The CoC is averaged the same way and halved, as it is measured in pixels.
fn downsample(
    source: &SourceFrame,
    full: &IVector4,
    half_full: &IVector4,
) -> (Array3<f32>, Array2<f32>) {
    let half_height = (half_full.w - half_full.y) as usize;
    let half_width = (half_full.z - half_full.x) as usize;
    let mut rgba = Array3::zeros((half_height, half_width, 4));
    let mut coc = Array2::zeros((half_height, half_width));
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let rows = half_height.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let chunks = rgba
            .axis_chunks_iter_mut(Axis(0), rows)
            .zip(coc.axis_chunks_iter_mut(Axis(0), rows));
        for (index, (rgba_rows, coc_rows)) in chunks.enumerate() {
            scope.spawn(move || {
                downsample_rows(source, full, half_full, index * rows, rgba_rows, coc_rows);
            });
        }
    });
    (rgba, coc)
}

/// Fills the half-size rows starting at row `first` of `rgba` and `coc`; see [`downsample`].
fn downsample_rows(
    source: &SourceFrame,
    full: &IVector4,
    half_full: &IVector4,
    first: usize,
    mut rgba: ArrayViewMut3<'_, f32>,
    mut coc: ArrayViewMut2<'_, f32>,
) {
    let (height, width, _) = source.rgba.dim();
    let half_width = (half_full.z - half_full.x) as usize;
    let clamp = |value: i32, size: usize| value.clamp(0, size as i32 - 1) as usize;
    for row in 0..rgba.dim().0 {
        let top = 2 * (half_full.y + (first + row) as i32) - full.y;
        for x in 0..half_width {
            let left = 2 * (half_full.x + x as i32) - full.x;
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (sy, sx) = (clamp(top + dy, height), clamp(left + dx, width));
                for channel in 0..4 {
                    rgba[[row, x, channel]] += 0.25 * source.rgba[[sy, sx, channel]];
                }
                coc[[row, x]] += 0.125 * source.coc[[sy, sx]];
            }
        }
    }
}

/// A bilinear tap along one axis: the two half-size indices and the weight of the second.
struct Tap {
    /// Index of the first neighbour.
    first: usize,
    /// Index of the second neighbour.
    second: usize,
    /// Weight of the second neighbour; the first gets the rest.
    weight: f32,
}

/// Returns the taps for global full-size coordinate `global` in a half-size axis that starts at `half_start` and has `size` pixels.
fn tap(global: i32, half_start: i32, size: usize) -> Tap {
    let position = global as f32 * 0.5 - 0.25 - half_start as f32;
    let floor = position.floor();
    let last = size as i32 - 1;
    Tap {
        first: (floor as i32).clamp(0, last) as usize,
        second: (floor as i32 + 1).clamp(0, last) as usize,
        weight: position - floor,
    }
}

/// Everything the composite of one pixel reads.
struct Inputs<'a> {
    /// Full-resolution source.
    source: &'a SourceFrame,
    /// Half-resolution source, the result of [`downsample`].
    half_source: &'a Array3<f32>,
    /// Half-resolution render: RGBA and the front-layer transmittance in channel [`FRONT_TRANSMITTANCE`].
    half_render: &'a Array3<f32>,
    /// Region of the full-size frame in global coordinates.
    full: IVector4,
    /// Region of the half-size frame in global coordinates.
    half_full: IVector4,
}

/// Bilinear sample of a half-size array.
fn bilinear(array: &Array3<f32>, (ty, tx): (&Tap, &Tap), channel: usize) -> f32 {
    let row = |y: usize| {
        array[[y, tx.first, channel]] * (1.0 - tx.weight)
            + array[[y, tx.second, channel]] * tx.weight
    };
    row(ty.first) * (1.0 - ty.weight) + row(ty.second) * ty.weight
}

/// Blends the upsampled half-size render with the full-resolution source for one pixel by adding the source detail
/// the half size lost, `source - half source`, to the render with the part of the source the render keeps.
///
/// That part is zero for a blurred pixel and for one the render changed (the change is measured against the 2x2
/// averaged source), and at most the front-layer transmittance: a blurred foreground covers the source, and over a
/// textured surface its colour can match the 2x2 average, so the change alone would let the full-resolution detail
/// through it. A focused pixel under nothing that the render did not change keeps its source.
fn blend_pixel(inputs: &Inputs<'_>, (gx, gy): (i32, i32), out: &mut [f32]) {
    let (half_height, half_width, _) = inputs.half_render.dim();
    let taps = (
        &tap(gy, inputs.half_full.y, half_height),
        &tap(gx, inputs.half_full.x, half_width),
    );
    let (y, x) = ((gy - inputs.full.y) as usize, (gx - inputs.full.x) as usize);
    let by_focus = smoothstep(FOCUS_BELOW, BLUR_ABOVE, inputs.source.coc[[y, x]].abs());
    let mut render = [0.0; 4];
    for (channel, value) in render.iter_mut().enumerate() {
        *value = bilinear(inputs.half_render, taps, channel);
    }
    out[..4].copy_from_slice(&render);
    // Blurred pixels, most of a defocused frame, take the render without the detail terms.
    if by_focus >= 1.0 {
        return;
    }
    let covered = 1.0 - bilinear(inputs.half_render, taps, FRONT_TRANSMITTANCE).clamp(0.0, 1.0);
    let mut half_source = [0.0; 4];
    let mut change = 0.0_f32;
    let mut brightness = 0.0_f32;
    for channel in 0..4 {
        half_source[channel] = bilinear(inputs.half_source, taps, channel);
        change = change.max((render[channel] - half_source[channel]).abs());
        if channel < 3 {
            brightness = brightness.max(half_source[channel].abs());
        }
    }
    let by_change = smoothstep(CHANGE_BELOW, CHANGE_ABOVE, change / (0.02 + brightness));
    let kept = 1.0 - by_focus.max(by_change).max(covered);
    for (channel, value) in out.iter_mut().enumerate().take(4) {
        *value += kept * (inputs.source.rgba[[y, x, channel]] - half_source[channel]);
    }
}

/// Blends the rows of `requested` that `chunk` covers, starting at row `first_row` of `requested`.
fn blend_rows(
    inputs: &Inputs<'_>,
    requested: &IVector4,
    first_row: usize,
    mut chunk: ArrayViewMut3<'_, f32>,
) {
    let (rows, width, _) = chunk.dim();
    for local_y in 0..rows {
        let gy = requested.y + (first_row + local_y) as i32;
        for local_x in 0..width {
            let mut pixel = [0.0; 4];
            blend_pixel(inputs, (requested.x + local_x as i32, gy), &mut pixel);
            for (channel, value) in pixel.into_iter().enumerate() {
                chunk[[local_y, local_x, channel]] = value;
            }
        }
    }
}

/// Blends every pixel of `requested` into a new RGBA array, spreading the rows over the available threads.
fn composite(inputs: &Inputs<'_>, requested: &IVector4) -> Array3<f32> {
    let height = (requested.w - requested.y) as usize;
    let width = (requested.z - requested.x) as usize;
    let mut output = Array3::zeros((height, width, 4));
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let rows = height.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (index, chunk) in output.axis_chunks_iter_mut(Axis(0), rows).enumerate() {
            scope.spawn(move || blend_rows(inputs, requested, index * rows, chunk));
        }
    });
    output
}

/// Renders the frame at half size through the shared pipeline, upsamples the result and writes the requested region into `image`.
///
/// # Errors
/// Returns an error when the half-size render fails.
pub(super) async fn render_half(
    runner: &WgpuRunner,
    settings: &Settings,
    frame: FrameGeometry,
    image: &mut ArrayViewMut3<'_, f32>,
    source: SourceFrame,
    filter: Option<Array3<f32>>,
    max_block_pixels: usize,
) -> Result<()> {
    let specs = RenderSpecs {
        full_region: frame.full,
        render_region: frame.requested,
    }
    .scale(0.5);
    let half_full = specs.full_region;
    let half_requested = grow_by_one(&specs.render_region, &half_full);
    let (half_rgba, half_coc) = downsample(&source, &frame.full, &half_full);
    let half_dim = half_rgba.dim();
    let half_frame = FrameGeometry::new(
        &RenderSpecs {
            full_region: half_full,
            render_region: half_requested,
        },
        half_dim,
    )?;
    let mut half_frame = half_frame;
    // The half render keeps the gather's transmittance channel for the composite.
    half_frame.channels = GATHER_OUTPUT_CHANNELS;
    let half_settings = half_settings(settings);
    let mut half_render = Array3::zeros((half_dim.0, half_dim.1, GATHER_OUTPUT_CHANNELS));
    let mut gather_rgba = half_rgba.clone();
    let highlights = if scatter::plain_disc(&half_settings) {
        scatter::extract(
            &mut gather_rgba,
            &half_coc,
            validated_max_radius(&half_settings)?,
        )
    } else {
        Vec::new()
    };
    render_planned(
        runner,
        &half_settings,
        half_frame,
        &mut half_render.view_mut(),
        SourceFrame {
            rgba: gather_rgba,
            coc: half_coc.clone(),
        },
        filter,
        max_block_pixels,
    )
    .await?;
    scatter::splat(&mut half_render, &half_coc, &highlights);
    let blended = composite(
        &Inputs {
            source: &source,
            half_source: &half_rgba,
            half_render: &half_render,
            full: frame.full,
            half_full,
        },
        &frame.requested,
    );
    let [full_x, full_y] = origin(&frame.full);
    let (rows, columns) = (
        (frame.requested.y - full_y) as usize..(frame.requested.w - full_y) as usize,
        (frame.requested.x - full_x) as usize..(frame.requested.z - full_x) as usize,
    );
    image
        .slice_mut(s![rows, columns, ..frame.channels])
        .assign(&blended.slice(s![.., .., ..frame.channels]));
    Ok(())
}

#[cfg(test)]
/// Tests for the half-resolution helpers and for the Fast render that uses them.
mod tests {
    use super::*;
    use crate::{OpenDefocusRenderer, datamodel::defocus::DefocusMode};

    /// Builds a region from its corners.
    fn region(x: i32, y: i32, z: i32, w: i32) -> IVector4 {
        IVector4 { x, y, z, w }
    }

    /// Builds a source whose colour and CoC are constant.
    fn flat_source(height: usize, width: usize, color: f32, coc: f32) -> SourceFrame {
        SourceFrame {
            rgba: Array3::from_elem((height, width, 4), color),
            coc: Array2::from_elem((height, width), coc),
        }
    }

    #[test]
    /// Verifies the ramp is zero below, one above and smooth between its edges.
    fn smoothstep_ramps_between_its_edges() {
        assert_eq!(smoothstep(1.0, 3.0, 0.0), 0.0);
        assert_eq!(smoothstep(1.0, 3.0, 4.0), 1.0);
        assert_eq!(smoothstep(1.0, 3.0, 2.0), 0.5);
    }

    #[test]
    /// Verifies blocks are averaged on the global 2x2 grid, edges repeat, and the CoC is halved.
    fn downsample_averages_global_blocks_and_halves_the_coc() {
        let mut source = flat_source(4, 4, 0.0, 8.0);
        source.rgba[[1, 1, 0]] = 4.0;
        // The frame starts at global (1, 1), so the first half pixel covers global (0..2) and repeats the frame edge.
        let (rgba, coc) = downsample(&source, &region(1, 1, 5, 5), &region(0, 0, 3, 3));
        assert_eq!(rgba.dim(), (3, 3, 4));
        assert_eq!(rgba[[0, 0, 0]], 0.0, "edge repeats pixel (0, 0)");
        assert_eq!(rgba[[1, 0, 0]], 0.0, "block covers global 2..4 in x and y");
        assert_eq!(
            rgba[[1, 1, 0]],
            1.0,
            "block holds source (1, 1) once, and (0, 0), (0, 1), (1, 0)"
        );
        assert_eq!(coc, Array2::from_elem((3, 3), 4.0));
    }

    /// Returns a half-size render of `rgba` with every pixel's front-layer transmittance set to `transmittance`.
    fn with_transmittance(rgba: &Array3<f32>, transmittance: f32) -> Array3<f32> {
        let (height, width, _) = rgba.dim();
        Array3::from_shape_fn(
            (height, width, GATHER_OUTPUT_CHANNELS),
            |(y, x, channel)| {
                if channel == FRONT_TRANSMITTANCE {
                    transmittance
                } else {
                    rgba[[y, x, channel]]
                }
            },
        )
    }

    #[test]
    /// Verifies a focused pixel that the render shows unchanged keeps its full-resolution source, detail included.
    fn unchanged_focused_pixels_keep_the_source() {
        let mut source = flat_source(8, 8, 0.25, 0.0);
        source.rgba[[3, 3, 0]] = 0.9;
        let (half_source, _) = downsample(&source, &region(0, 0, 8, 8), &region(0, 0, 4, 4));
        let blended = composite(
            &Inputs {
                source: &source,
                half_source: &half_source,
                half_render: &with_transmittance(&half_source, 1.0),
                full: region(0, 0, 8, 8),
                half_full: region(0, 0, 4, 4),
            },
            &region(0, 0, 8, 8),
        );
        for (blended, source) in blended.iter().zip(source.rgba.iter()) {
            assert!((blended - source).abs() < 1e-6, "{blended} != {source}");
        }
    }

    #[test]
    /// Verifies a blurred pixel takes the upsampled half-resolution render.
    fn blurred_pixels_take_the_render() {
        let source = flat_source(8, 8, 0.25, 10.0);
        let half_source = Array3::from_elem((4, 4, 4), 0.25);
        let blended = composite(
            &Inputs {
                source: &source,
                half_source: &half_source,
                half_render: &with_transmittance(&Array3::from_elem((4, 4, 4), 0.75), 0.0),
                full: region(0, 0, 8, 8),
                half_full: region(0, 0, 4, 4),
            },
            &region(2, 2, 6, 6),
        );
        assert!(blended.iter().all(|value| (value - 0.75).abs() < 1e-6));
    }

    #[test]
    /// A focused pixel under an opaque blurred foreground takes the render, even where the render equals the 2x2
    /// average of the source and so looks unchanged: the full-resolution detail must not show through the foreground.
    fn focused_pixels_under_a_foreground_take_the_render() {
        let source = SourceFrame {
            rgba: Array3::from_shape_fn((8, 8, 4), |(y, _, _)| if y % 2 == 0 { 0.6 } else { 0.2 }),
            coc: Array2::zeros((8, 8)),
        };
        let (half_source, _) = downsample(&source, &region(0, 0, 8, 8), &region(0, 0, 4, 4));
        let blended = composite(
            &Inputs {
                source: &source,
                half_source: &half_source,
                half_render: &with_transmittance(&half_source, 0.0),
                full: region(0, 0, 8, 8),
                half_full: region(0, 0, 4, 4),
            },
            &region(0, 0, 8, 8),
        );
        assert!(blended.iter().all(|value| (value - 0.4).abs() < 1e-6));
    }

    /// Renders `source` with 2D defocus of `size` pixels at `quality`, in full or as the stripe `rows` of the frame.
    async fn render(
        source: &Array3<f32>,
        quality: Quality,
        rows: Option<(i32, i32)>,
    ) -> Array3<f32> {
        let (height, width, _) = source.dim();
        let mut settings = Settings::default();
        settings.render.resolution = UVector2 {
            x: width as u32,
            y: height as u32,
        };
        settings.render.gui = true;
        settings.render.set_quality(quality);
        settings.defocus.set_defocus_mode(DefocusMode::Twod);
        settings.defocus.circle_of_confusion.size = 10.0;
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        let mut image = source.clone();
        let full = region(0, 0, width as i32, height as i32);
        match rows {
            None => renderer
                .render(settings, image.view_mut(), None, None)
                .await
                .unwrap(),
            Some((top, bottom)) => renderer
                .render_stripe(
                    RenderSpecs {
                        full_region: full,
                        render_region: region(0, top, width as i32, bottom),
                    },
                    settings,
                    image.view_mut(),
                    None,
                    None,
                )
                .await
                .unwrap(),
        }
        image
    }

    #[tokio::test]
    #[ignore = "requires a hardware GPU"]
    /// Verifies Fast stays close to Accurate, and that stripes join to the full Fast frame.
    async fn fast_renders_close_to_accurate_and_stripes_join() {
        let source = Array3::from_shape_fn((128, 192, 4), |(y, x, channel)| {
            if channel == 3 {
                1.0
            } else if (x / 24 + y / 24) % 2 == 0 {
                0.9 - 0.2 * channel as f32
            } else {
                0.05
            }
        });
        let fast = render(&source, Quality::Fast, None).await;
        let accurate = render(&source, Quality::Accurate, None).await;
        let mean = (&fast - &accurate).mapv(f32::abs).mean().unwrap();
        assert!(mean < 0.02, "Fast drifts from Accurate by {mean}");
        let stripe = render(&source, Quality::Fast, Some((32, 96))).await;
        for y in 32..96 {
            for x in 0..192 {
                for channel in 0..4 {
                    assert_eq!(stripe[[y, x, channel]], fast[[y, x, channel]], "({x},{y})");
                }
            }
        }
    }
}
