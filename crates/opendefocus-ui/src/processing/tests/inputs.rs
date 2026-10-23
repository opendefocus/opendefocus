//! Tests for input loading, validation and result-mode support in the UI processor.

use super::super::*;
use opendefocus::datamodel::render::ResultMode;

#[test]
fn test_failed_file_load_preserves_inputs() -> Result<()> {
    let path =
        std::env::temp_dir().join(format!("opendefocus-ui-invalid-{}.png", std::process::id()));
    std::fs::write(&path, b"not an image")?;
    let source = Array3::from_elem((2, 3, 4), 0.5);
    let depth = Array2::from_elem((2, 3), 2.0);
    let mut processor = Processor {
        image: Some(source.clone()),
        depth: Some(depth.clone()),
        filter: Some(source.clone()),
        ..Default::default()
    };
    let result = tokio::runtime::Runtime::new()?.block_on(async {
        for job in [
            Job::Image(path.clone()),
            Job::Depth(path.clone()),
            Job::Filter(path.clone()),
        ] {
            assert!(processor.process(job).await.is_err());
            assert_eq!(processor.image.as_ref(), Some(&source));
            assert_eq!(processor.depth.as_ref(), Some(&depth));
            assert_eq!(processor.filter.as_ref(), Some(&source));
        }
        Ok(())
    });
    std::fs::remove_file(path)?;
    result
}

/// Checks that every quality and the 2D, depth and focal-plane modes validate, and that a NaN focal plane is rejected.
#[test]
fn adaptive_quality_and_result_modes_are_supported() {
    let mut settings = Settings::default();
    settings.render.gui = true;

    for quality in [
        opendefocus::datamodel::render::Quality::Fast,
        opendefocus::datamodel::render::Quality::Accurate,
    ] {
        settings.render.set_quality(quality);
        assert!(opendefocus::validate_adaptive_settings(&settings).is_ok());
    }

    settings.defocus.set_defocus_mode(DefocusMode::Twod);
    assert!(opendefocus::validate_adaptive_settings(&settings).is_ok());
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.render.set_result_mode(ResultMode::FocalPlaneSetup);
    assert!(opendefocus::validate_adaptive_settings(&settings).is_ok());

    settings.defocus.circle_of_confusion.focal_plane = f32::NAN;
    assert!(opendefocus::validate_adaptive_settings(&settings).is_err());
}

/// Checks that the bundled direct-CoC sample is negated on load, so its near foreground is positive.
#[test]
fn bundled_hdr_coc_is_converted_to_positive_foreground() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let sample = crate::samples::SAMPLES.last().unwrap();
        let source = decode(&image::load_from_memory(sample.depth.unwrap())?)?;
        let source_coc = source.slice(s![.., .., 0]);
        let mut processor = Processor::default();
        processor.process(Job::Sample(sample)).await?;
        let loaded = processor.depth.as_ref().unwrap();

        assert!(sample.invert_direct_coc);
        assert_eq!(loaded.dim(), source_coc.dim());
        for ((y, x), value) in source_coc.indexed_iter() {
            assert_eq!(loaded[[y, x]], -*value);
        }
        assert!(loaded.iter().any(|value| *value > 0.0));
        assert!(loaded.iter().any(|value| *value < 0.0));
        Ok(())
    })
}

/// Checks that spatial lens effects pass adaptive settings validation.
#[test]
fn adaptive_validation_allows_spatial_lens_effects() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.non_uniform.catseye.enable = true;
    settings.non_uniform.barndoors.enable = true;
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.axial_aberration.enable = true;
    settings.non_uniform.inverse_foreground = true;

    assert!(opendefocus::validate_adaptive_settings(&settings).is_ok());
}

/// Checks that focus picks return the depth at the picked pixel and reject missing, out-of-range, negative and non-finite depth.
#[test]
fn pick_focus_returns_depth_at_pixel_and_rejects_invalid_picks() -> Result<()> {
    let depth = Array2::from_shape_vec((2, 3), vec![0.5, 1.0, 2.0, -1.0, f32::NAN, 3.0])?;
    let mut processor = Processor {
        depth: Some(depth),
        ..Default::default()
    };
    tokio::runtime::Runtime::new()?.block_on(async {
        match processor.process(Job::PickFocus { x: 2, y: 0 }).await? {
            Output::Focus { x, y, value } => assert_eq!((x, y, value), (2, 0, 2.0)),
            _ => panic!("expected a focus output"),
        }
        assert!(
            processor
                .process(Job::PickFocus { x: 0, y: 1 })
                .await
                .is_err()
        );
        assert!(
            processor
                .process(Job::PickFocus { x: 1, y: 1 })
                .await
                .is_err()
        );
        assert!(
            processor
                .process(Job::PickFocus { x: 3, y: 0 })
                .await
                .is_err()
        );
        assert!(
            processor
                .process(Job::PickFocus { x: 0, y: 2 })
                .await
                .is_err()
        );
        let mut empty = Processor::default();
        assert!(empty.process(Job::PickFocus { x: 0, y: 0 }).await.is_err());
        Ok(())
    })
}

/// Checks that float RGBA pixels are converted to 8-bit display pixels in row order.
#[test]
fn preview_converts_float_rgba_to_display_pixels() -> Result<()> {
    let image = Array3::from_shape_vec((1, 2, 4), vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0])?;
    let preview = preview(&image)?;
    assert_eq!(preview.size, [2, 1]);
    assert_eq!(
        preview.pixels,
        vec![
            eframe::egui::Color32::from_rgb(255, 0, 0),
            eframe::egui::Color32::from_rgb(0, 0, 255),
        ]
    );
    Ok(())
}
