//! Hardware tests that stream Accurate gathers across an internal source core seam and compare an ROI there with
//! the full-frame gather: a focus ramp and a far occluder whose probe disc spans the seam.
use crate::datamodel::IVector4;
use ndarray::{Array2, Array3};

use super::WgpuRunner;
use super::adaptive_streaming_gpu_tests::assert_features_match;
use crate::convolve_settings::ConvolveSettings;
use crate::runners::adaptive::{CocPolarity, resolve_subpixel_focus_with};
use crate::runners::adaptive_streaming::{
    global_tile_layout, plan_source_regions_with_core_limit, preparation_halo,
    prepare_source_chunks,
};
use crate::runners::{StreamGather, StreamSource};

/// Builds the source colour and CoC fixture whose columns step across the source seam at `seam_x`.
fn seam_fixture(width: usize, height: usize, seam_x: i32) -> (Array3<f32>, Array2<f32>) {
    let source = Array3::from_shape_fn((height, width, 4), |(_, x, channel)| {
        let color = if x as i32 == seam_x {
            [1.0, 0.25, 9.0, 1.0]
        } else if x as i32 == seam_x + 6 {
            [0.5, 12.0, 0.25, 1.0]
        } else if (x as i32) < seam_x {
            [0.25, 0.5, 3.0, 1.0]
        } else {
            [8.0, 1.5, 0.125, 1.0]
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn((height, width), |(_, x)| {
        if x as i32 == seam_x || x as i32 == seam_x + 6 {
            0.0
        } else if (x as i32) < seam_x {
            -8.0
        } else {
            8.0
        }
    });
    (source, coc)
}

/// Builds the Accurate quality astigmatic depth lens used by the focus ramp test over the `full` frame.
fn focus_ramp_lens(
    full: &IVector4,
    width: usize,
    height: usize,
    max_radius: f32,
) -> ConvolveSettings {
    let specs = crate::datamodel::render::RenderSpecs::from_rects(
        glam::IVec4::new(full.x, full.y, full.z, full.w),
        glam::IVec4::new(full.x, full.y, full.z, full.w),
    );
    let mut settings = crate::datamodel::Settings::default();
    settings
        .defocus
        .set_defocus_mode(crate::datamodel::defocus::DefocusMode::Depth);
    settings.defocus.use_direct_math = true;
    settings.defocus.circle_of_confusion.max_size = max_radius;
    settings.render.resolution = crate::datamodel::UVector2 {
        x: width as u32,
        y: height as u32,
    };
    settings.render.center = crate::datamodel::Vector2 {
        x: width as f32 * 0.5,
        y: height as f32 * 0.5,
    };
    settings.render.filter.mode = crate::datamodel::render::FilterMode::Simple as i32;
    settings
        .render
        .set_quality(crate::datamodel::render::Quality::Accurate);
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.astigmatism.amount = 2.0;
    settings.non_uniform.astigmatism.gamma = 1.4;
    crate::datamodel::settings_to_convolve_settings(
        &settings,
        &specs,
        crate::adaptive_filter_resolution(&settings),
        (width * height) as u32,
    )
}

/// Streams every source core overlapping `repair` into a repair-sized buffer and resolves its subpixel focus.
async fn stream_repaired_roi(
    runner: &WgpuRunner,
    source: &StreamSource<'_>,
    full: &IVector4,
    repair: &IVector4,
    gather: &StreamGather<'_>,
    width: usize,
    height: usize,
) -> Array3<f32> {
    let repair_origin = [repair.x as usize, repair.y as usize];
    let mut streamed = Array3::zeros((
        (repair.w - repair.y) as usize,
        (repair.z - repair.x) as usize,
        4,
    ));
    let mut stream_cache = runner.new_stream_frame_cache(source.chunks);
    for (destination_index, chunk) in source.chunks.iter().enumerate() {
        let core = chunk.region.core;
        if core.x >= repair.z || core.z <= repair.x || core.y >= repair.w || core.w <= repair.y {
            continue;
        }
        let copy = IVector4 {
            x: core.x.max(repair.x),
            y: core.y.max(repair.y),
            z: core.z.min(repair.z),
            w: core.w.min(repair.w),
        };
        let output = runner
            .gather_adaptive_streamed_high(
                source,
                &mut stream_cache,
                destination_index,
                full,
                &copy,
                gather,
            )
            .await
            .unwrap();
        for y in copy.y..copy.w {
            for x in copy.x..copy.z {
                for channel in 0..4 {
                    streamed[[(y - repair.y) as usize, (x - repair.x) as usize, channel]] =
                        output[[(y - copy.y) as usize, (x - copy.x) as usize, channel]];
                }
            }
        }
    }
    resolve_subpixel_focus_with(
        width,
        height,
        repair_origin,
        |x, y| {
            source
                .chunks
                .iter()
                .find_map(|chunk| {
                    let core = chunk.region.core;
                    let global_x = x as i32 + full.x;
                    let global_y = y as i32 + full.y;
                    if global_x >= core.x
                        && global_x < core.z
                        && global_y >= core.y
                        && global_y < core.w
                    {
                        let local_x = (global_x - core.x) as usize;
                        let local_y = (global_y - core.y) as usize;
                        chunk
                            .prepared
                            .pixels
                            .get(local_y * chunk.prepared.width + local_x)
                            .copied()
                    } else {
                        None
                    }
                })
                .expect("source cores cover all prepared frame pixels")
        },
        &mut streamed,
    );
    streamed
}

/// Asserts that every requested pixel of the streamed repair buffer matches the full-frame output within tolerance.
fn assert_roi_matches(
    full_output: &Array3<f32>,
    streamed: &Array3<f32>,
    requested: &IVector4,
    repair: &IVector4,
) {
    for y in requested.y..requested.w {
        for x in requested.x..requested.z {
            for channel in 0..4 {
                let expected = full_output[[y as usize, x as usize, channel]];
                let actual = streamed[[(y - repair.y) as usize, (x - repair.x) as usize, channel]];
                let tolerance = 1e-3 * expected.abs().max(1.0);
                assert!(
                    (expected - actual).abs() <= tolerance,
                    "streamed ROI mismatch at ({x}, {y}, {channel}): {expected} != {actual}"
                );
            }
        }
    }
}

/// Builds a far surface (`CoC` -2) that ends two columns before the source seam at `seam_x`, in front of a bright
/// far background (`CoC` -8): its far occlusion integrates a probe disc that spans the seam.
fn far_occluder_fixture(width: usize, height: usize, seam_x: i32) -> (Array3<f32>, Array2<f32>) {
    let surface = |x: usize| (x as i32) < seam_x - 2;
    let source = Array3::from_shape_fn((height, width, 4), |(_, x, channel)| {
        let color = if surface(x) {
            [0.2, 0.3, 0.4, 1.0]
        } else {
            [6.0, 2.0, 1.0, 1.0]
        };
        color[channel]
    });
    let coc = Array2::from_shape_fn(
        (height, width),
        |(_, x)| if surface(x) { -2.0 } else { -8.0 },
    );
    (source, coc)
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a streamed focus ramp matches the full gather across ROI and source seams.
async fn hardware_streamed_focus_ramp_matches_full_gather_across_roi_and_source_seams() {
    assert_streamed_matches_full_gather_at_seam(seam_fixture).await;
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that the streamed far occlusion, whose probe cover accumulates over source cores, matches the
/// full gather's far occlusion pre-pass.
async fn hardware_streamed_far_occlusion_matches_full_gather_across_source_seams() {
    assert_streamed_matches_full_gather_at_seam(far_occluder_fixture).await;
}

/// Renders the scene of `fixture` (source and `CoC` for a frame size and seam column) as one astigmatic Accurate
/// gather and streamed over source cores, and asserts that an ROI across an internal core seam matches.
async fn assert_streamed_matches_full_gather_at_seam(
    fixture: impl Fn(usize, usize, i32) -> (Array3<f32>, Array2<f32>),
) {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (width, height) = (120usize, 100usize);
    let max_radius = 8.0f32;
    let full = IVector4 {
        x: 0,
        y: 0,
        z: width as i32,
        w: height as i32,
    };
    let halo = preparation_halo(max_radius).unwrap();
    let regions = plan_source_regions_with_core_limit(
        &full,
        halo,
        10_000,
        10_000,
        runner.device.limits().max_texture_dimension_2d as usize,
    )
    .unwrap();
    assert!(
        regions.len() > 1,
        "fixture must require streamed source cores"
    );
    let seam_x = regions
        .iter()
        .map(|region| region.core.z)
        .filter(|x| *x > full.x && *x < full.z)
        .min_by_key(|x| (x - width as i32 / 2).abs())
        .expect("planner should create an internal vertical core seam");

    let (source, coc) = fixture(width, height, seam_x);
    let full_prepared = runner
        .prepare_adaptive_layers(&source, &coc, max_radius, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let full_features = runner
        .classify_tiles(&source, &coc, [full.x, full.y], max_radius)
        .await
        .unwrap();
    let (tiles_x, tiles_y) = {
        let (_, _, tiles_x, tiles_y) = global_tile_layout(&full).unwrap();
        (tiles_x, tiles_y)
    };
    let lens = focus_ramp_lens(&full, width, height, max_radius);
    let budget =
        crate::AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate);
    let full_output = runner
        .gather_adaptive_frame_with_lens(
            &source,
            &full_prepared,
            &full_features,
            max_radius,
            budget,
            Some(&lens),
        )
        .await
        .unwrap();

    let (chunks, streamed_features, tile_layout) =
        prepare_source_chunks(&runner, &source, &coc, &full, &regions, max_radius)
            .await
            .unwrap();
    assert_eq!((tile_layout.2, tile_layout.3), (tiles_x, tiles_y));
    assert_features_match(&streamed_features, &full_features);

    let requested = IVector4 {
        x: seam_x - 4,
        y: 32,
        z: seam_x + 3,
        w: 64,
    };
    let repair_halo = 7;
    let repair = IVector4 {
        x: requested.x.saturating_sub(repair_halo).max(full.x),
        y: requested.y.saturating_sub(repair_halo).max(full.y),
        z: requested.z.saturating_add(repair_halo).min(full.z),
        w: requested.w.saturating_add(repair_halo).min(full.w),
    };
    let stream_source = StreamSource {
        chunks: &chunks,
        features: &streamed_features,
        tile_layout,
    };
    let gather = StreamGather {
        max_radius_px: max_radius,
        sample_budget: budget,
        lens: &lens,
        aperture: None,
    };
    let streamed = stream_repaired_roi(
        &runner,
        &stream_source,
        &full,
        &repair,
        &gather,
        width,
        height,
    )
    .await;
    assert_roi_matches(&full_output, &streamed, &requested, &repair);
}
