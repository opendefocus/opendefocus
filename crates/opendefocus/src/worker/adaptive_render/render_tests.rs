//! Tests and hardware checks for adaptive renders that are cropped, split into blocks or streamed.
use super::*;
use crate::error::Error;
use crate::runners::adaptive_streaming::ADAPTIVE_OUTPUT_BYTES_PER_PIXEL;
use crate::{
    OpenDefocusRenderer,
    datamodel::{
        IVector4, UVector2,
        defocus::DefocusMode,
        render::{FilterMode, Quality},
    },
    runners::adaptive_streaming::{plan_source_regions_with_core_limit, preparation_halo},
};
use blocks::pixels_per_budget;
use ndarray::s;

/// Runs the cropped render comparison with offset tiles and asserts it matches the full frame.
async fn check_cropped_render_matches_full_frame_with_offset_tiles() {
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 { x: 80, y: 64 };
    settings.render.gui = true;
    settings.render.quality = Quality::Accurate as i32;
    settings.defocus.set_defocus_mode(DefocusMode::Twod);
    settings.defocus.circle_of_confusion.size = 4.0;
    let source = Array3::from_shape_fn((64, 80, 4), |(y, x, channel)| {
        if channel == 3 {
            1.0
        } else {
            ((x * 3 + y * 5 + channel * 13) % 97) as f32 / 96.0
        }
    });
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    let mut full = source.clone();
    renderer
        .render(settings.clone(), full.view_mut(), None, None)
        .await
        .unwrap();
    let mut crop = source.slice(s![7..54, 11..70, ..]).to_owned();
    renderer
        .render_stripe(
            RenderSpecs {
                full_region: IVector4 {
                    x: 11,
                    y: 7,
                    z: 70,
                    w: 54,
                },
                render_region: IVector4 {
                    x: 23,
                    y: 19,
                    z: 58,
                    w: 42,
                },
            },
            settings,
            crop.view_mut(),
            None,
            None,
        )
        .await
        .unwrap();
    for y in 19..42 {
        for x in 23..58 {
            for channel in 0..4 {
                assert!(
                    (full[[y, x, channel]] - crop[[y - 7, x - 11, channel]]).abs() < 1e-5,
                    "crop mismatch at ({x},{y},{channel})"
                );
            }
        }
    }
    assert_eq!(
        crop[[0, 0, 0]],
        source[[7, 11, 0]],
        "outside requested region stays untouched"
    );
}

#[tokio::test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock outside sandbox"]
/// Hardware check that a cropped render with offset tiles matches the full frame.
async fn cropped_render_matches_full_frame_with_offset_tiles() {
    check_cropped_render_matches_full_frame_with_offset_tiles().await;
}

#[tokio::test]
/// Verifies 2D and Fast quality renders work without a depth map.
async fn two_d_and_fast_quality_render_without_depth() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Twod);
    settings.defocus.circle_of_confusion.size = 2.0;
    settings.render.gui = true;
    settings.render.quality = Quality::Fast as i32;
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    let mut image = Array3::from_shape_fn((16, 16, 4), |(_, _, c)| [2.0f32, 1.0, 0.5, 0.5][c]);
    renderer
        .render(settings, image.view_mut(), None, None)
        .await
        .unwrap();
    for pixel in image.outer_iter() {
        assert!(pixel.iter().all(|v| v.is_finite()));
    }
    assert!((image[[8, 8, 0]] - 2.0).abs() < 1e-4);
    assert!((image[[8, 8, 3]] - 0.5).abs() < 1e-4);
}

#[tokio::test]
/// Verifies a stale preview flag keeps the image filter and padding requirements.
async fn stale_preview_flag_keeps_image_filter_and_padding_requirements() {
    let mut settings = Settings::default();
    settings.render.filter.preview = true;
    settings.render.filter.mode = FilterMode::Image as i32;
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    assert!(renderer.padding(&settings) > 0);

    let mut image = Array3::<f32>::zeros((4, 4, 4));
    let result = renderer
        .render(settings.clone(), image.view_mut(), None, None)
        .await;
    assert!(matches!(result, Err(Error::NoFilterProvided)));

    settings.render.filter.mode = FilterMode::BokehCreator as i32;
    assert_eq!(renderer.padding(&settings), 0);
}

#[tokio::test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock outside sandbox"]
/// Hardware check that forced adaptive blocks match a single pass.
async fn forced_adaptive_blocks_match_single_pass() {
    check_forced_adaptive_blocks_match_single_pass().await;
}

/// Runs the forced block comparison and asserts it matches the single pass.
async fn check_forced_adaptive_blocks_match_single_pass() {
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 { x: 80, y: 64 };
    settings.render.gui = true;
    settings.render.quality = Quality::Accurate as i32;
    settings.defocus.set_defocus_mode(DefocusMode::Twod);
    settings.defocus.circle_of_confusion.size = 1.0;
    let source = Array3::from_shape_fn((64, 80, 4), |(y, x, channel)| {
        if channel == 3 {
            0.7
        } else {
            ((x * 3 + y * 5 + channel * 13) % 97) as f32 / 96.0
        }
    });
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    let mut whole = source.clone();
    renderer
        .render(settings.clone(), whole.view_mut(), None, None)
        .await
        .unwrap();
    let mut blocked = source;
    render_with_block_limit(
        &renderer.runner,
        &RenderSpecs::from_rects(
            glam::IVec4::new(0, 0, 80, 64),
            glam::IVec4::new(0, 0, 80, 64),
        ),
        &settings,
        blocked.view_mut(),
        None,
        None,
        4_000,
    )
    .await
    .unwrap();
    for (expected, actual) in whole.iter().zip(blocked.iter()) {
        assert!((expected - actual).abs() < 1e-5, "{expected} != {actual}");
    }
}

#[tokio::test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock outside sandbox"]
/// Hardware check that depth blocks keep focus edges and HDR across boundaries.
async fn depth_blocks_preserve_focus_edges_and_hdr_across_boundaries() {
    check_depth_blocks_preserve_focus_edges_and_hdr_across_boundaries().await;
}

/// Runs the depth block boundary comparison and asserts the focus edges and HDR match.
async fn check_depth_blocks_preserve_focus_edges_and_hdr_across_boundaries() {
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 { x: 96, y: 80 };
    settings.render.gui = true;
    settings.render.quality = Quality::Accurate as i32;
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = true;
    settings.defocus.circle_of_confusion.max_size = 4.0;
    let depth = Array2::from_shape_fn((80, 96), |(y, x)| {
        if (x as i32 - 46).pow(2) + (y as i32 - 39).pow(2) < 19 * 19 {
            0.0
        } else if x < 62 {
            -4.0
        } else {
            3.0
        }
    });
    let source = Array3::from_shape_fn((80, 96, 4), |(y, x, channel)| {
        if channel == 3 {
            0.65
        } else if (x == 32 && y == 32) || (x == 63 && y == 48) {
            8.0
        } else {
            ((x * 3 + y * 5 + channel * 13) % 97) as f32 / 96.0
        }
    });
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    let mut whole = source.clone();
    renderer
        .render(
            settings.clone(),
            whole.view_mut(),
            Some(depth.clone()),
            None,
        )
        .await
        .unwrap();
    let mut blocked = source;
    render_with_block_limit(
        &renderer.runner,
        &RenderSpecs::from_rects(
            glam::IVec4::new(0, 0, 96, 80),
            glam::IVec4::new(0, 0, 96, 80),
        ),
        &settings,
        blocked.view_mut(),
        Some(depth),
        None,
        6_400,
    )
    .await
    .unwrap();
    for (expected, actual) in whole.iter().zip(blocked.iter()) {
        assert!(
            (expected - actual).abs() < 1e-4,
            "block mismatch: {expected} != {actual}"
        );
    }
}

#[tokio::test]
#[ignore = "requires a hardware GPU"]
/// Hardware check that ROI and block renders match the full frame.
async fn hardware_roi_and_block_renders_match_full_frame() {
    check_cropped_render_matches_full_frame_with_offset_tiles().await;
    check_forced_adaptive_blocks_match_single_pass().await;
    check_depth_blocks_preserve_focus_edges_and_hdr_across_boundaries().await;
}

#[tokio::test]
#[ignore = "requires a hardware GPU with sufficient full-context storage"]
/// Hardware check that a large astigmatic ROI keeps the full frame context.
async fn hardware_large_astigmatic_roi_retains_full_frame_context() {
    let (width, height) = (3840, 2160);
    let mut settings = crate::datamodel::Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = true;
    settings.defocus.circle_of_confusion.max_size = 6.0;
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.astigmatism.amount = 2.0;
    settings.non_uniform.astigmatism.gamma = 1.4;
    settings.render.resolution = crate::datamodel::UVector2 {
        x: width as u32,
        y: height as u32,
    };
    settings.render.center = crate::datamodel::Vector2 {
        x: width as f32 * 0.5,
        y: height as f32 * 0.5,
    };
    settings
        .render
        .set_quality(crate::datamodel::render::Quality::Accurate);
    settings
        .render
        .set_farm_quality(crate::datamodel::render::Quality::Accurate);
    let renderer = crate::OpenDefocusRenderer::new(&mut settings)
        .await
        .unwrap();
    assert!(
        adaptive_block_pixel_limit(&renderer.runner) >= width * height,
        "GPU cannot retain this full source context"
    );
    let reference = [0.25_f32, 0.5, 2.0, 1.0];
    let mut image = Array3::from_shape_fn((height, width, 4), |(_, _, c)| reference[c]);
    let depth = Array2::from_elem((height, width), -4.0);
    let requested = glam::IVec4::new(2784, 832, 2801, 849);
    render(
        &renderer.runner,
        &RenderSpecs::from_rects(
            glam::IVec4::new(0, 0, width as i32, height as i32),
            requested,
        ),
        &settings,
        image.view_mut(),
        Some(depth),
        None,
    )
    .await
    .unwrap();
    for y in 832..849 {
        for x in 2784..2801 {
            for c in 0..4 {
                assert!(
                    (image[[y, x, c]] - reference[c]).abs() < 1e-5,
                    "large spatial ROI ({x},{y},{c}) = {}",
                    image[[y, x, c]]
                );
            }
        }
    }
    assert_eq!(image[[0, 0, 2]], reference[2]);
}

#[tokio::test]
#[ignore = "requires a hardware GPU"]
/// Hardware check that a forced astigmatic stream matches the single-pass ROI.
async fn hardware_forced_astigmatic_stream_matches_single_pass_roi() {
    let (width, height) = (120usize, 100usize);
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = true;
    settings.defocus.circle_of_confusion.max_size = 8.0;
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.astigmatism.amount = 2.0;
    settings.non_uniform.astigmatism.gamma = 1.4;
    settings.render.resolution = UVector2 {
        x: width as u32,
        y: height as u32,
    };
    settings.render.center = crate::datamodel::Vector2 {
        x: width as f32 * 0.5,
        y: height as f32 * 0.5,
    };
    settings.render.filter.mode = FilterMode::Simple as i32;
    settings.render.set_quality(Quality::Accurate);
    settings.render.set_farm_quality(Quality::Accurate);
    let runner = WgpuRunner::new_adaptive().await.unwrap();

    let mut unwarped = settings.clone();
    unwarped.non_uniform.astigmatism.enable = false;
    let halo = i32::try_from(
        crate::adaptive_source_padding(&unwarped).max(preparation_halo(8.0).unwrap() as u32),
    )
    .unwrap();
    let regions = plan_source_regions_with_core_limit(
        &IVector4 {
            x: 0,
            y: 0,
            z: width as i32,
            w: height as i32,
        },
        halo,
        10_000,
        pixels_per_budget(
            runner.device.limits().max_storage_buffer_binding_size,
            ADAPTIVE_OUTPUT_BYTES_PER_PIXEL,
        ),
        runner.device.limits().max_texture_dimension_2d as usize,
    )
    .unwrap();
    let seam_x = regions
        .iter()
        .map(|region| region.core.z)
        .filter(|x| *x > 0 && *x < width as i32)
        .min_by_key(|x| (x - width as i32 / 2).abs())
        .expect("forced stream fixture must have an internal source seam");

    let image = Array3::from_shape_fn((height, width, 4), |(y, x, channel)| {
        let alpha = 0.2 + ((x * 7 + y * 11) % 75) as f32 / 100.0;
        let rgb = [
            ((x * 3 + y * 5) % 23) as f32 * 0.75,
            ((x * 13 + y * 2) % 31) as f32 * 0.5,
            ((x * 5 + y * 17) % 19) as f32 * 1.25,
        ];
        if channel == 3 { alpha } else { rgb[channel] }
    });
    let coc = Array2::from_shape_fn((height, width), |(y, x)| {
        let dx = x as i32 - seam_x;
        if y == height / 2 && (-4..=4).contains(&dx) {
            dx as f32 * 2.0
        } else if x < width / 3 {
            -8.0
        } else if x > 2 * width / 3 {
            8.0
        } else {
            0.0
        }
    });
    let specs = RenderSpecs::from_rects(
        glam::IVec4::new(0, 0, width as i32, height as i32),
        glam::IVec4::new(seam_x - 5, 42, seam_x + 6, 58),
    );
    let mut reference = image.clone();
    render_with_block_limit(
        &runner,
        &specs,
        &settings,
        reference.view_mut(),
        Some(coc.clone()),
        None,
        usize::MAX,
    )
    .await
    .unwrap();
    let mut streamed = image.clone();
    render_with_block_limit(
        &runner,
        &specs,
        &settings,
        streamed.view_mut(),
        Some(coc),
        None,
        10_000,
    )
    .await
    .unwrap();

    for y in specs.render_region.y..specs.render_region.w {
        for x in specs.render_region.x..specs.render_region.z {
            for channel in 0..4 {
                let expected = reference[[y as usize, x as usize, channel]];
                let actual = streamed[[y as usize, x as usize, channel]];
                let tolerance = 2e-3 * expected.abs().max(1.0);
                assert!(
                    (expected - actual).abs() <= tolerance,
                    "streamed ROI mismatch at ({x}, {y}, {channel}): {expected} != {actual}"
                );
            }
        }
    }
    assert_eq!(streamed[[0, 0, 0]], image[[0, 0, 0]]);
}

/// Builds a small 2D or depth-mode settings value with the given circle of confusion size.
fn small_settings(mode: DefocusMode, size: f32) -> Settings {
    let mut settings = Settings::default();
    settings.render.resolution = UVector2 { x: 16, y: 16 };
    settings.render.gui = true;
    settings.render.quality = Quality::Accurate as i32;
    settings.defocus.set_defocus_mode(mode);
    settings.defocus.circle_of_confusion.size = size;
    settings
}

/// Full-frame render specs for a `width` by `height` image.
fn full_specs(width: i32, height: i32) -> RenderSpecs {
    RenderSpecs::from_rects(
        glam::IVec4::new(0, 0, width, height),
        glam::IVec4::new(0, 0, width, height),
    )
}

#[tokio::test]
#[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
/// Verifies on hardware that the adaptive render rejects images with a non-RGBA channel count, depth maps of the wrong size,
/// a zero block limit, and a halo too large for the GPU coordinates, each with its specific error.
async fn adaptive_render_rejects_invalid_inputs_with_specific_errors() {
    let mut settings = small_settings(DefocusMode::Depth, 2.0);
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    let specs = full_specs(16, 16);
    let depth = Array2::from_elem((16, 16), 0.5_f32);

    let mut five_channels = Array3::<f32>::zeros((16, 16, 5));
    let result = render_with_block_limit(
        &renderer.runner,
        &specs,
        &settings,
        five_channels.view_mut(),
        Some(depth.clone()),
        None,
        usize::MAX,
    )
    .await;
    assert!(matches!(result, Err(Error::InvalidDimensions)));

    let mut image = Array3::from_shape_fn((16, 16, 4), |(_, _, c)| if c == 3 { 1.0 } else { 0.5 });
    let wrong_depth = Array2::from_elem((16, 15), 0.5_f32);
    let result = render_with_block_limit(
        &renderer.runner,
        &specs,
        &settings,
        image.view_mut(),
        Some(wrong_depth),
        None,
        usize::MAX,
    )
    .await;
    assert!(matches!(result, Err(Error::InvalidDimensions)));

    let result = render_with_block_limit(
        &renderer.runner,
        &specs,
        &settings,
        image.view_mut(),
        Some(depth.clone()),
        None,
        0,
    )
    .await;
    assert!(matches!(result, Err(Error::SizeLimitExceeded)));

    let result = render_with_block_limit(
        &renderer.runner,
        &specs,
        &settings,
        image.view_mut(),
        None,
        None,
        usize::MAX,
    )
    .await;
    assert!(matches!(result, Err(Error::DepthNotFound)),);

    let mut huge = small_settings(DefocusMode::Twod, 3.0e9);
    let huge_renderer = OpenDefocusRenderer::new(&mut huge).await.unwrap();
    let result = render_with_block_limit(
        &huge_renderer.runner,
        &specs,
        &huge,
        image.view_mut(),
        None,
        None,
        usize::MAX,
    )
    .await;
    assert!(matches!(result, Err(Error::AdaptiveSettingsUnsupported(_))));
}

#[tokio::test]
#[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
/// Verifies on hardware that a depth map supplied in 2D mode is ignored, so the result equals the render without depth.
async fn two_d_render_ignores_a_supplied_depth_map() {
    let mut settings = small_settings(DefocusMode::Twod, 2.0);
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    let source = Array3::from_shape_fn((16, 16, 4), |(y, x, c)| ((x + y * 3 + c) % 7) as f32 / 6.0);
    let mut without = source.clone();
    renderer
        .render(settings.clone(), without.view_mut(), None, None)
        .await
        .unwrap();
    let mut with = source;
    renderer
        .render(
            settings,
            with.view_mut(),
            Some(Array2::from_elem((16, 16), 0.25_f32)),
            None,
        )
        .await
        .unwrap();
    assert_eq!(with, without);
}
