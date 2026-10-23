//! Hardware tests for streamed Accurate gathers: cropped halos, source seams, opacity proofs and one-pixel cores.
use crate::datamodel::IVector4;
use ndarray::{Array2, Array3, s};

use super::{TileFeatures, WgpuRunner};
use crate::convolve_settings::ConvolveSettings;
use crate::runners::adaptive::CocPolarity;
use crate::runners::{StreamGather, StreamSource};
use crate::runners::{
    adaptive::resolve_subpixel_focus_with,
    adaptive_streaming::{
        crop_prepared_layers, crop_rgba, global_tile_layout, plan_source_regions,
        plan_source_regions_with_core_limit, preparation_halo, prepare_source_chunks,
    },
};

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that preparing a cropped halo matches the full-frame preparation.
async fn hardware_cropped_halo_preparation_matches_full_frame() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (width, height) = (120usize, 100usize);
    let max_radius = 8.0f32;
    let halo = preparation_halo(max_radius).unwrap();
    let source = Array3::from_shape_fn((height, width, 4), |(y, x, channel)| {
        let alpha = 0.35 + (y % 4) as f32 * 0.1;
        let background = [0.5 + y as f32 * 0.01, 0.25, 1.5];
        let foreground = [12.0 + y as f32 * 0.1, 5.0, 2.0];
        let color = match x {
            0..=47 => background,
            48 => std::array::from_fn(|c| background[c] * 0.6 + foreground[c] * 0.4),
            _ => foreground,
        };
        if channel == 3 {
            alpha
        } else {
            color[channel] * alpha
        }
    });
    let coc = Array2::from_shape_fn((height, width), |(y, x)| {
        if x == 48 {
            1.0
        } else if (65..85).contains(&x) && (54..58).contains(&y) {
            -6.0
        } else if x < 48 {
            -4.0
        } else {
            8.0
        }
    });
    let full = IVector4 {
        x: -37,
        y: -21,
        z: -37 + width as i32,
        w: -21 + height as i32,
    };
    let full_prepared = runner
        .prepare_adaptive_layers(&source, &coc, max_radius, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let regions = plan_source_regions(&full, halo, 84 * 84, 96).unwrap();
    assert!(regions.len() > 1, "fixture must require source partitions");

    let boundary = regions
        .iter()
        .find(|region| {
            region.core.x <= full.x + 48
                && full.x + 48 < region.core.z
                && region.core.y <= full.y + 40
                && full.y + 40 < region.core.w
        })
        .expect("a core should contain the fractional-coverage boundary");
    let checked = [regions[0], *boundary, *regions.last().unwrap()];
    let edge_index = 40 * width + 48;
    let full_edge_coverage = full_prepared.pixels[edge_index].near_valid;
    assert!(
        (full_edge_coverage - 0.4).abs() < 1e-3,
        "fixture should exercise fractional foreground coverage, got {full_edge_coverage}"
    );

    for region in checked {
        let context_source = crop_rgba(&source, &full, &region.context).unwrap();
        let context_coc = coc
            .slice(s![
                (region.context.y - full.y) as usize..(region.context.w - full.y) as usize,
                (region.context.x - full.x) as usize..(region.context.z - full.x) as usize,
            ])
            .to_owned();
        let context_prepared = runner
            .prepare_adaptive_layers(
                &context_source,
                &context_coc,
                max_radius,
                CocPolarity::PositiveForeground,
            )
            .await
            .unwrap();
        let cropped =
            crop_prepared_layers(&context_prepared, &region.context, &region.core).unwrap();
        let expected = crop_prepared_layers(&full_prepared, &full, &region.core).unwrap();
        assert_eq!(
            (cropped.width, cropped.height),
            (expected.width, expected.height)
        );
        assert_eq!(cropped.pixels.len(), expected.pixels.len());
        for (index, (actual, expected)) in cropped.pixels.iter().zip(&expected.pixels).enumerate() {
            assert_eq!(actual.near_rgba, expected.near_rgba, "near RGBA at {index}");
            assert_eq!(actual.far_rgba, expected.far_rgba, "far RGBA at {index}");
            assert_eq!(actual.near_coc, expected.near_coc, "near CoC at {index}");
            assert_eq!(actual.far_coc, expected.far_coc, "far CoC at {index}");
            assert_eq!(
                actual.near_valid, expected.near_valid,
                "near validity at {index}"
            );
            assert_eq!(
                actual.far_valid, expected.far_valid,
                "far validity at {index}"
            );
        }
        assert_eq!(
            cropped.near_edge_distance, expected.near_edge_distance,
            "near edge distance for core {:?}",
            region.core
        );
    }
}

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

/// Asserts that streamed classifier features have the same count and values as the full-frame features.
fn assert_features_match(streamed: &[TileFeatures], expected: &[TileFeatures]) {
    assert_eq!(streamed.len(), expected.len());
    for (streamed, expected) in streamed.iter().zip(expected) {
        assert_eq!(streamed.values, expected.values);
    }
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

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a streamed focus ramp matches the full gather across ROI and source seams.
async fn hardware_streamed_focus_ramp_matches_full_gather_across_roi_and_source_seams() {
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

    let (source, coc) = seam_fixture(width, height, seam_x);
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

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
#[allow(
    clippy::float_cmp,
    reason = "opaque alpha is written exactly as 1.0 by the shader"
)]
/// Hardware check that the opacity proof covers transparent neighbouring cores.
async fn hardware_streamed_opacity_proof_covers_transparent_neighboring_cores() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (width, height) = (32usize, 16usize);
    let max_radius = 4.0f32;
    let full = IVector4 {
        x: 0,
        y: 0,
        z: width as i32,
        w: height as i32,
    };
    let regions = plan_source_regions_with_core_limit(
        &full,
        preparation_halo(max_radius).unwrap(),
        width * height,
        16 * 16,
        runner.device.limits().max_texture_dimension_2d as usize,
    )
    .unwrap();
    assert_eq!(regions.len(), 2);

    let source = Array3::from_shape_fn((height, width, 4), |(_, x, channel)| {
        if x < 16 {
            [1.0, 0.5, 0.25, 1.0][channel]
        } else {
            0.0
        }
    });
    let coc = Array2::from_elem((height, width), max_radius);
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, max_radius, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let features = runner
        .classify_tiles(&source, &coc, [full.x, full.y], max_radius)
        .await
        .unwrap();
    let specs = crate::datamodel::render::RenderSpecs::from_rects(
        glam::IVec4::new(full.x, full.y, full.z, full.w),
        glam::IVec4::new(full.x, full.y, full.z, full.w),
    );
    let mut settings = crate::datamodel::Settings::default();
    settings.render.resolution = crate::datamodel::UVector2 {
        x: width as u32,
        y: height as u32,
    };
    settings.render.center = crate::datamodel::Vector2 {
        x: width as f32 * 0.5,
        y: height as f32 * 0.5,
    };
    settings.render.filter.mode = crate::datamodel::render::FilterMode::Simple as i32;
    let lens = crate::datamodel::settings_to_convolve_settings(
        &settings,
        &specs,
        crate::adaptive_filter_resolution(&settings),
        (width * height) as u32,
    );
    let budget =
        crate::AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate);
    let expected = runner
        .gather_adaptive_frame_with_lens(
            &source,
            &prepared,
            &features,
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
    assert!(
        chunks[0]
            .source
            .index_axis(ndarray::Axis(2), 3)
            .iter()
            .all(|alpha| *alpha == 1.0)
    );
    assert!(
        chunks[1]
            .source
            .index_axis(ndarray::Axis(2), 3)
            .iter()
            .all(|alpha| *alpha == 0.0)
    );

    let mut streamed = Array3::zeros((height, width, 4));
    let mut cache = runner.new_stream_frame_cache(&chunks);
    for (destination_index, chunk) in chunks.iter().enumerate() {
        let core = chunk.region.core;
        let output = runner
            .gather_adaptive_streamed_high(
                &StreamSource {
                    chunks: &chunks,
                    features: &streamed_features,
                    tile_layout,
                },
                &mut cache,
                destination_index,
                &full,
                &core,
                &StreamGather {
                    max_radius_px: max_radius,
                    sample_budget: budget,
                    lens: &lens,
                    aperture: None,
                },
            )
            .await
            .unwrap();
        for y in 0..output.dim().0 {
            for x in 0..output.dim().1 {
                for channel in 0..4 {
                    streamed[[core.y as usize + y, core.x as usize + x, channel]] =
                        output[[y, x, channel]];
                }
            }
        }
    }
    for (index, (actual, expected)) in streamed.iter().zip(expected.iter()).enumerate() {
        assert!(
            (actual - expected).abs() < 1.0e-4,
            "mixed-alpha streamed output differs at element {index}: {actual} != {expected}"
        );
    }
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that a one-pixel core matches the full custom aperture gather.
async fn hardware_streamed_one_pixel_core_matches_full_custom_aperture_gather() {
    check_streamed_one_pixel_core_matches_full_custom_aperture_gather(15).await;
    check_streamed_one_pixel_core_matches_full_custom_aperture_gather(-17).await;
}

/// Builds the source colour and CoC fixture of `width` by `height` pixels for the one-pixel core test.
fn one_pixel_fixture(width: usize, height: usize) -> (Array3<f32>, Array2<f32>) {
    let source = Array3::from_shape_fn((height, width, 4), |(y, x, channel)| {
        let alpha = 0.23 + ((3 * x + 5 * y) % 7) as f32 * 0.1;
        let color = [
            1.5 + x as f32 * 0.2,
            0.2 + y as f32 * 0.3,
            4.0 + (x + y) as f32 * 0.15,
        ];
        if channel == 3 {
            alpha
        } else {
            color[channel] * alpha
        }
    });
    let coc = Array2::from_shape_fn((height, width), |(y, x)| {
        if x == 0 && y == 0 {
            1.5
        } else if (x + 2 * y) % 7 < 3 {
            -3.25
        } else {
            5.25
        }
    });
    (source, coc)
}

/// Builds the aperture mip chain at `filter_resolution`: a disc with horizontal and vertical bar channels.
fn one_pixel_aperture(filter_resolution: glam::UVec2) -> crate::renders::resize::MipmapBuffer {
    crate::renders::resize::MipmapBuffer::from_vec(vec![Array3::from_shape_fn(
        (
            filter_resolution.y as usize,
            filter_resolution.x as usize,
            4,
        ),
        |(y, x, channel)| {
            let dx = (x as f32 + 0.5) / filter_resolution.x as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / filter_resolution.y as f32 * 2.0 - 1.0;
            let radius = (dx * dx + dy * dy).sqrt();
            let disc = (1.0 - radius).clamp(0.0, 1.0);
            match channel {
                0 => disc * disc,
                1 => (1.0 - dx.abs()).clamp(0.0, 1.0) * disc,
                2 => (1.0 - dy.abs()).clamp(0.0, 1.0) * disc,
                _ => disc,
            }
        },
    )])
    .unwrap()
}

/// Builds the image-filtered astigmatic depth lens used by the one-pixel core test over the `full` frame.
fn one_pixel_lens(
    full: &IVector4,
    width: usize,
    height: usize,
    max_radius: f32,
    filter_resolution: glam::UVec2,
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
    settings.render.filter.mode = crate::datamodel::render::FilterMode::Image as i32;
    settings.render.filter.resolution = 8;
    settings
        .render
        .set_quality(crate::datamodel::render::Quality::Accurate);
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.astigmatism.amount = 2.0;
    settings.non_uniform.astigmatism.gamma = 1.4;
    crate::datamodel::settings_to_convolve_settings(
        &settings,
        &specs,
        filter_resolution,
        (width * height) as u32,
    )
}

/// Runs the one-pixel core comparison and asserts that the results match.
async fn check_streamed_one_pixel_core_matches_full_custom_aperture_gather(origin: i32) {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let (width, height) = (17usize, 17usize);
    let max_radius = 6.0f32;
    let full = IVector4 {
        x: origin,
        y: origin,
        z: origin + width as i32,
        w: origin + height as i32,
    };
    let regions = plan_source_regions_with_core_limit(
        &full,
        preparation_halo(max_radius).unwrap(),
        width * height,
        16 * 16,
        runner.device.limits().max_texture_dimension_2d as usize,
    )
    .unwrap();
    assert_eq!(
        regions.len(),
        4,
        "fixture should force a four-core partition"
    );
    assert_eq!(
        regions[0].core,
        IVector4 {
            x: full.x,
            y: full.y,
            z: full.x + 1,
            w: full.y + 1,
        },
        "first source core should be exactly one pixel"
    );

    let (source, coc) = one_pixel_fixture(width, height);
    let filter_resolution = {
        let mut settings = crate::datamodel::Settings::default();
        settings.render.filter.mode = crate::datamodel::render::FilterMode::Image as i32;
        settings.render.filter.resolution = 8;
        crate::adaptive_filter_resolution(&settings)
    };
    let aperture = one_pixel_aperture(filter_resolution);

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
    let lens = one_pixel_lens(&full, width, height, max_radius, filter_resolution);
    let budget =
        crate::AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate);
    let full_output = runner
        .gather_adaptive_frame_with_aperture(
            &source,
            &full_prepared,
            &full_features,
            max_radius,
            budget,
            &lens,
            &aperture,
        )
        .await
        .unwrap();

    let (chunks, streamed_features, tile_layout) =
        prepare_source_chunks(&runner, &source, &coc, &full, &regions, max_radius)
            .await
            .unwrap();
    assert_eq!((tile_layout.2, tile_layout.3), (tiles_x, tiles_y));
    assert_features_match(&streamed_features, &full_features);

    let mut streamed = Array3::zeros((height, width, 4));
    let mut stream_cache = runner.new_stream_frame_cache_with_budget(&chunks, 0);
    for (destination_index, chunk) in chunks.iter().enumerate() {
        let core = chunk.region.core;
        let output = runner
            .gather_adaptive_streamed_high(
                &StreamSource {
                    chunks: &chunks,
                    features: &streamed_features,
                    tile_layout,
                },
                &mut stream_cache,
                destination_index,
                &full,
                &core,
                &StreamGather {
                    max_radius_px: max_radius,
                    sample_budget: budget,
                    lens: &lens,
                    aperture: Some(&aperture),
                },
            )
            .await
            .unwrap();
        for local_y in 0..chunk.prepared.height {
            for local_x in 0..chunk.prepared.width {
                for channel in 0..4 {
                    streamed[[
                        (core.y - full.y) as usize + local_y,
                        (core.x - full.x) as usize + local_x,
                        channel,
                    ]] = output[[local_y, local_x, channel]];
                }
            }
        }
    }
    for y in 0..height {
        for x in 0..width {
            for channel in 0..4 {
                let expected = full_output[[y, x, channel]];
                let actual = streamed[[y, x, channel]];
                let tolerance = 1e-3 * expected.abs().max(1.0);
                assert!(
                    (expected - actual).abs() <= tolerance,
                    "one-pixel-core mismatch at ({x}, {y}, {channel}): {expected} != {actual}"
                );
            }
        }
    }
}
