//! Hardware-backed rendering tests for the UI processor: full-frame, stripes, previews and samples.

use super::super::*;

/// Checks that a full-frame GPU render through the processor matches running the adaptive pipeline directly.
#[test]
#[ignore = "requires serialized Metal hardware access"]
fn adaptive_processor_renders_the_full_frame_on_gpu() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let mut processor = Processor::default();
        let sample = crate::samples::SAMPLES.last().unwrap();
        processor.process(Job::Sample(sample)).await?;

        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.use_direct_math = true;
        settings.defocus.circle_of_confusion.max_size = 24.0;
        settings.defocus.camera_max_size = 24.0;
        let dimensions = processor.image.as_ref().unwrap().dim();
        settings.render.center.x = dimensions.1 as f32 * 0.5;
        settings.render.center.y = dimensions.0 as f32 * 0.5;
        settings.non_uniform.catseye.enable = true;
        // Fast renders through a half-size path that the direct gather below does not take.
        settings
            .render
            .set_quality(opendefocus::datamodel::render::Quality::Accurate);
        let Output::Render {
            preview: actual,
            region,
            requested,
            complete,
            elapsed,
            initialization,
            ..
        } = processor
            .process(Job::Render {
                settings: settings.clone(),
                region: None,
            })
            .await?
        else {
            unreachable!()
        };
        let image = processor.image.as_ref().unwrap();
        let depth = processor.depth.as_ref().unwrap();
        let full = IVector4 {
            x: 0,
            y: 0,
            z: image.dim().1 as i32,
            w: image.dim().0 as i32,
        };
        assert!(complete);
        assert_eq!(region, full);
        assert_eq!(requested, full);
        eprintln!(
            "Adaptive UI Processor smoke: {:.2} ms processing, {:.2} ms initialization",
            elapsed.as_secs_f64() * 1000.0,
            initialization.as_secs_f64() * 1000.0
        );

        let coc = opendefocus::adaptive_coc_from_depth(depth, &settings);
        let renderer = &processor.renderer.as_ref().unwrap().1;
        let features = renderer
            .classify_adaptive_tiles(image, &coc, [0, 0], 24.0)
            .await?;
        let prepared = renderer
            .prepare_adaptive_layers(
                image,
                &coc,
                24.0,
                opendefocus::CocPolarity::PositiveForeground,
            )
            .await?;
        let expected = renderer
            .gather_adaptive_frame_with_settings(image, &prepared, &features, &settings)
            .await?;
        assert_eq!(actual.pixels, preview(&expected)?.pixels);
        Ok(())
    })
}

/// Checks that a zero focus depth renders in focal-plane setup, and that filter previews render in a single pass.
#[test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
fn zero_focus_and_filter_preview_are_supported() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let source = Array3::from_shape_fn((40, 32, 4), |(_, _, c)| if c == 3 { 1.0 } else { 0.0 });
        let mut processor = Processor {
            image: Some(source.clone()),
            depth: Some(Array2::zeros((40, 32))),
            filter: Some(source.clone()),
            ..Default::default()
        };
        let Output::Focus { value, .. } = processor.process(Job::PickFocus { x: 0, y: 0 }).await?
        else {
            unreachable!()
        };
        assert_eq!(value, 0.0);
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.circle_of_confusion.focal_plane = value;
        settings
            .render
            .set_result_mode(opendefocus::datamodel::render::ResultMode::FocalPlaneSetup);
        for math in [0, 1] {
            settings.defocus.circle_of_confusion.math = math;
            let mut bands = Vec::new();
            let last = processor
                .process_with_updates(
                    Job::Render {
                        settings: settings.clone(),
                        region: None,
                    },
                    |band| {
                        bands.push(band);
                        Ok(())
                    },
                )
                .await?;
            bands.push(last);
            for band in bands {
                let Output::Render { preview, .. } = band else {
                    unreachable!()
                };
                assert!(
                    preview
                        .pixels
                        .iter()
                        .all(|pixel| pixel.to_array() == [0, 255, 0, 255])
                );
            }
        }
        settings.render.filter.preview = true;
        for mode in [
            opendefocus::datamodel::render::FilterMode::Simple,
            opendefocus::datamodel::render::FilterMode::BokehCreator,
            opendefocus::datamodel::render::FilterMode::Image,
        ] {
            settings.render.filter.set_mode(mode);
            let output = processor
                .process_with_updates(
                    Job::Render {
                        settings: settings.clone(),
                        region: None,
                    },
                    |_| panic!("Filter preview must render in a single pass"),
                )
                .await?;
            let preview = match output {
                Output::Render {
                    preview, complete, ..
                } => {
                    assert!(complete);
                    assert_eq!(preview.size, [32, 40]);
                    preview
                }
                Output::FilterPreview { preview, .. } => {
                    let bounds = settings
                        .render
                        .filter
                        .calculate_filter_box(settings.bokeh.aspect_ratio);
                    assert_eq!(
                        preview.size,
                        [
                            (bounds[2] - bounds[0]) as usize,
                            (bounds[3] - bounds[1]) as usize
                        ]
                    );
                    preview
                }
                _ => unreachable!(),
            };
            if mode == opendefocus::datamodel::render::FilterMode::BokehCreator {
                assert!(preview.pixels.iter().any(|pixel| pixel.r() > 0));
            } else {
                // A stale filter-preview flag cannot override focal-plane
                // setup in Simple or Image mode.
                assert!(
                    preview
                        .pixels
                        .iter()
                        .all(|pixel| { pixel.to_array() == [0, 255, 0, 255] })
                );
            }
        }
        assert_eq!(processor.image.as_ref().unwrap(), &source);
        Ok(())
    })
}

/// Checks that a generated bokeh preview has the filter-box size and matches a direct render, without a source image or depth.
#[test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
fn generated_preview_uses_host_format_without_source_or_depth() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let mut processor = Processor::default();
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings
            .render
            .filter
            .set_mode(opendefocus::datamodel::render::FilterMode::BokehCreator);
        settings.render.filter.preview = true;
        for (shape, resolution, aspect) in [
            (
                opendefocus::datamodel::bokeh_creator::FilterType::Disc,
                32,
                0.5,
            ),
            (
                opendefocus::datamodel::bokeh_creator::FilterType::Blade,
                64,
                2.0,
            ),
        ] {
            settings.bokeh.set_filter_type(shape);
            settings.render.filter.resolution = resolution;
            settings.bokeh.aspect_ratio = aspect;
            let bounds = settings.render.filter.calculate_filter_box(aspect);
            let expected_size = [
                (bounds[2] - bounds[0]) as usize,
                (bounds[3] - bounds[1]) as usize,
            ];
            let Output::FilterPreview {
                preview: actual, ..
            } = processor
                .process(Job::Render {
                    settings: settings.clone(),
                    region: None,
                })
                .await?
            else {
                unreachable!()
            };
            assert_eq!(actual.size, expected_size);
            let mut expected = Array3::zeros((expected_size[1], expected_size[0], 4));
            let renderer = &processor.renderer.as_ref().unwrap().1;
            renderer
                .render(settings.clone(), expected.view_mut(), None, None)
                .await?;
            assert_eq!(actual.pixels, preview(&expected)?.pixels);
        }
        assert!(processor.image.is_none());
        assert!(processor.depth.is_none());
        Ok(())
    })
}

/// Checks that one UI render request covers the whole frame and matches a direct full-frame render within one unit per channel.
#[test]
fn ui_render_uses_one_adaptive_request_and_matches_full_frame() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let source = Array3::from_shape_fn((140, 32, 4), |(y, x, c)| {
            if c == 3 {
                1.0
            } else {
                ((x / 3 + y / 3 + c) % 2) as f32
            }
        });
        let mut settings = Settings::default();
        settings.defocus.circle_of_confusion.size = 3.0;
        settings
            .render
            .set_quality(opendefocus::datamodel::render::Quality::Accurate);
        // Adapter-dependent coverage runs in GPU validation.
        let Ok(renderer) = OpenDefocusRenderer::new(&mut settings).await else {
            return Ok(());
        };
        let mut full = source.clone();
        renderer
            .render(settings.clone(), full.view_mut(), None, None)
            .await?;
        let expected = preview(&full)?;
        let device = settings.render.device_name.clone().unwrap_or_default();
        let mut processor = Processor {
            image: Some(source.clone()),
            renderer: Some((RendererKey, renderer.clone(), device)),
            ..Default::default()
        };
        let mut updates = 0;
        let output = processor
            .process_with_updates(
                Job::Render {
                    settings: settings.clone(),
                    region: None,
                },
                |_| {
                    updates += 1;
                    Ok(())
                },
            )
            .await?;
        assert_eq!(updates, 0, "A single adaptive request has no partial bands");
        let Output::Render {
            preview: actual,
            region,
            complete,
            ..
        } = output
        else {
            unreachable!()
        };
        assert_eq!(
            region,
            IVector4 {
                x: 0,
                y: 0,
                z: 32,
                w: 140
            }
        );
        assert!(complete);
        assert_eq!(actual.pixels.len(), expected.pixels.len());
        for (pixel, expected) in actual.pixels.iter().zip(&expected.pixels) {
            assert!(
                pixel
                    .to_array()
                    .iter()
                    .zip(expected.to_array())
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "UI render differs from direct full-frame render"
            );
        }
        assert_eq!(processor.image.as_ref().unwrap(), &source);
        for (quality, gpu_height) in [
            (opendefocus::datamodel::render::Quality::Fast, 256),
            (opendefocus::datamodel::render::Quality::Accurate, 64),
        ] {
            settings.render.set_quality(quality);
            assert_eq!(renderer.stripe_height(&settings), gpu_height);
        }
        settings
            .render
            .set_result_mode(opendefocus::datamodel::render::ResultMode::FocalPlaneSetup);
        assert_eq!(renderer.stripe_height(&settings), 32);
        Ok(())
    })
}

/// Checks that padded stripes match the full render for inverse-depth focus values on hardware.
#[test]
#[ignore = "requires a hardware GPU"]
fn hardware_padded_stripes_match_full_render_for_inverse_z_focus_values() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let (height, width) = (300usize, 48usize);
        let source = Array3::from_shape_fn((height, width, 4), |(y, x, c)| {
            if c == 3 {
                1.0
            } else {
                ((x * 7 + y * 11 + c * 17) % 101) as f32 / 100.0
            }
        });

        for (focal_plane, depth_center, depth_delta, astigmatism) in [
            (0.0, 0.0001, 0.0000002, false),
            (10.0, 10.0, 0.02, false),
            (10.0, 10.0, 0.02, true),
        ] {
            let depth = Array2::from_shape_fn((height, width), |(y, x)| {
                depth_center + ((x + y) % 5) as f32 * depth_delta
            });
            let mut settings = Settings::default();
            settings.defocus.set_defocus_mode(DefocusMode::Depth);
            settings.defocus.use_direct_math = false;
            settings.defocus.circle_of_confusion.math =
                opendefocus::datamodel::circle_of_confusion::Math::OneDividedByZ.into();
            settings.defocus.circle_of_confusion.focal_plane = focal_plane;
            settings.non_uniform.astigmatism.enable = astigmatism;
            settings.non_uniform.astigmatism.amount = 2.0;
            settings.render.center.x = width as f32 * 0.5;
            settings.render.center.y = height as f32 * 0.5;
            settings.defocus.circle_of_confusion.size = 8.0;
            settings.defocus.circle_of_confusion.max_size = 64.0;
            settings.defocus.camera_max_size = 64.0;
            settings
                .render
                .set_quality(opendefocus::datamodel::render::Quality::Accurate);
            settings.render.gui = true;
            settings.render.resolution = UVector2 {
                x: width as u32,
                y: height as u32,
            };
            let coc = opendefocus::adaptive_coc_from_depth(&depth, &settings);
            let max_coc = coc.iter().map(|value| value.abs()).fold(0.0f32, f32::max);
            assert!(
                max_coc < 16.0,
                "fixture CoC should stay below configured radius: {max_coc}"
            );
            assert!(settings.defocus.get_max_size() >= 64.0);

            let renderer = OpenDefocusRenderer::new(&mut settings)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            let mut full = source.clone();
            renderer
                .render(settings.clone(), full.view_mut(), Some(depth.clone()), None)
                .await?;
            let mut striped = source.clone();
            let padding = renderer.padding(&settings) as i32;
            let full_rect = IVector4 {
                x: 0,
                y: 0,
                z: width as i32,
                w: height as i32,
            };
            for y in (0..height as i32).step_by(64) {
                let end = (y + 64).min(height as i32);
                let input = IVector4 {
                    x: 0,
                    y: y.saturating_sub(padding).max(0),
                    z: width as i32,
                    w: end.saturating_add(padding).min(height as i32),
                };
                let mut stripe = source
                    .slice(s![input.y..input.w, input.x..input.z, ..])
                    .to_owned();
                let stripe_depth = depth
                    .slice(s![input.y..input.w, input.x..input.z])
                    .to_owned();
                renderer
                    .render_stripe(
                        RenderSpecs {
                            full_region: input,
                            render_region: IVector4 {
                                y,
                                w: end,
                                ..full_rect
                            },
                        },
                        settings.clone(),
                        stripe.view_mut(),
                        Some(stripe_depth),
                        None,
                    )
                    .await?;
                striped
                    .slice_mut(s![y..end, .., ..])
                    .assign(&stripe.slice(s![y - input.y..end - input.y, .., ..]));
            }
            for (index, (a, b)) in full.iter().zip(striped.iter()).enumerate() {
                assert!(
                    (a - b).abs() <= 1e-5,
                    "stripe mismatch at {index}: {a} vs {b}"
                );
            }
        }
        Ok(())
    })
}

/// Checks that rendering only a viewport region matches the same pixels from the full frame.
#[test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
fn viewport_stripe_matches_full_frame() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let source = Array3::from_shape_fn((32, 48, 4), |(y, x, c)| {
            if c == 3 {
                1.0
            } else {
                ((x / 3 + y / 3 + c) % 2) as f32
            }
        });
        let mut processor = Processor {
            image: Some(source.clone()),
            depth: Some(Array2::from_elem((32, 48), -2.0)),
            filter: Some(source.clone()),
            ..Default::default()
        };
        let mut settings = Settings::default();
        settings.defocus.circle_of_confusion.size = 3.0;
        settings.defocus.circle_of_confusion.max_size = 3.0;
        settings.render.center.x = 24.0;
        settings.render.center.y = 16.0;
        settings.non_uniform.catseye.enable = true;
        settings
            .render
            .filter
            .set_mode(opendefocus::datamodel::render::FilterMode::Image);
        let region = IVector4 {
            x: 20,
            y: 12,
            z: 28,
            w: 20,
        };
        for () in [()] {
            for mode in [DefocusMode::Twod, DefocusMode::Depth] {
                settings.defocus.set_defocus_mode(mode);
                settings.defocus.use_direct_math = true;
                let Output::Render { preview: full, .. } = processor
                    .process(Job::Render {
                        settings: settings.clone(),
                        region: None,
                    })
                    .await?
                else {
                    unreachable!()
                };
                let Output::Render {
                    preview: part,
                    region: actual,
                    ..
                } = processor
                    .process(Job::Render {
                        settings: settings.clone(),
                        region: Some(region),
                    })
                    .await?
                else {
                    unreachable!()
                };
                assert_eq!(actual, region);
                assert_eq!(part.size, [8, 8]);
                for y in 0..8 {
                    for x in 0..8 {
                        let expected = full.pixels[(y + 12) * 48 + x + 20].to_array();
                        let actual = part.pixels[y * 8 + x].to_array();
                        assert!(
                            expected.iter().zip(actual).all(|(a, b)| a.abs_diff(b) <= 1),
                            "Stripe mismatch at {x},{y}, mode={mode:?}: {actual:?} != {expected:?}"
                        );
                    }
                }
                assert_eq!(processor.image.as_ref().unwrap(), &source);
            }
        }
        assert!(
            processor
                .process(Job::Render {
                    settings,
                    region: Some(IVector4 { x: -1, ..region }),
                })
                .await
                .is_err()
        );
        Ok(())
    })
}

/// Checks that every bundled sample loads with a depth map that matches its image and is signed.
#[test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
fn bundled_samples_load_with_matching_depth() -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let mut processor = Processor::default();
        for sample in crate::samples::SAMPLES.iter().rev() {
            let Output::Sample {
                preview, maximum, ..
            } = processor.process(Job::Sample(sample)).await?
            else {
                unreachable!()
            };
            let image = processor.image.as_ref().unwrap();
            assert_eq!(preview.size, [image.dim().1, image.dim().0]);
            assert!(processor.filter.is_none());
            assert_eq!(processor.depth.is_some(), sample.depth.is_some());
            if let Some(depth) = &processor.depth {
                validate_depth(image, depth)?;
                let min = depth.iter().copied().fold(f32::INFINITY, f32::min);
                let max = depth.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                println!(
                    "{}: depth range {min}..{max}, maximum radius {maximum}",
                    sample.name
                );
                if sample.lens.is_some() {
                    // Real depth values are positive and not bounded by the blur radius.
                    assert!(min > 0.0);
                    continue;
                }
                assert!(
                    min < 0.0 && max > 0.0,
                    "Expected signed circle-of-confusion fixture"
                );
                assert!(depth.iter().all(|v| v.abs() <= maximum));
                let mut settings = Settings::default();
                settings.defocus.set_defocus_mode(DefocusMode::Depth);
                settings.defocus.use_direct_math = true;
                settings.defocus.circle_of_confusion.max_size = maximum;
                settings
                    .render
                    .set_result_mode(opendefocus::datamodel::render::ResultMode::FocalPlaneSetup);
                assert!(matches!(
                    processor
                        .process(Job::Render {
                            region: None,
                            settings,
                        })
                        .await?,
                    Output::Render { .. }
                ));
            }
        }
        Ok(())
    })
}

/// Checks that the original image is evaluated repeatably, and that depth and focus picks are validated.
#[test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
fn evaluates_original_image_and_validates_depth() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let source = Array3::from_shape_fn(
            (8, 8, 4),
            |(_, x, c)| if c == 3 { 1.0 } else { x as f32 / 8.0 },
        );
        assert!(validate_depth(&source, &Array2::zeros((7, 8))).is_err());
        assert!(validate_depth(&source, &Array2::from_elem((8, 8), f32::NAN)).is_err());
        let mut processor = Processor {
            image: Some(source.clone()),
            ..Default::default()
        };
        let mut settings = Settings::default();
        settings.defocus.circle_of_confusion.size = 2.0;
        let first = processor
            .process(Job::Render {
                region: None,
                settings: settings.clone(),
            })
            .await?;
        let second = processor
            .process(Job::Render {
                region: None,
                settings: settings.clone(),
            })
            .await?;
        match (first, second) {
            (Output::Render { preview: a, .. }, Output::Render { preview: b, .. }) => {
                assert_eq!(a.size, [8, 8]);
                assert_eq!(a.pixels, b.pixels);
            }
            _ => unreachable!(),
        }
        assert_eq!(processor.image.as_ref().unwrap(), &source);
        let depth_path =
            std::env::temp_dir().join(format!("opendefocus-ui-depth-{}.exr", std::process::id()));
        Rgba32FImage::from_fn(8, 8, |_, _| image::Rgba([2.0, 5.0, 9.0, 1.0])).save(&depth_path)?;
        let loaded = processor.process(Job::Depth(depth_path.clone())).await;
        std::fs::remove_file(depth_path)?;
        assert!(matches!(loaded?, Output::Depth(_)));
        assert!(
            processor
                .depth
                .as_ref()
                .unwrap()
                .iter()
                .all(|&value| value.to_bits() == 2.0_f32.to_bits())
        );
        processor.depth.as_mut().unwrap()[[1, 3]] = 0.25;
        assert!(matches!(
            processor.process(Job::PickFocus { x: 3, y: 1 }).await?,
            Output::Focus { value: 0.25, .. }
        ));
        assert!(
            processor
                .process(Job::PickFocus { x: 8, y: 1 })
                .await
                .is_err()
        );
        processor.depth.as_mut().unwrap()[[1, 3]] = 0.0;
        assert!(matches!(
            processor.process(Job::PickFocus { x: 3, y: 1 }).await?,
            Output::Focus { value: 0.0, .. }
        ));
        processor.depth.as_mut().unwrap()[[1, 3]] = -1.0;
        assert!(
            processor
                .process(Job::PickFocus { x: 3, y: 1 })
                .await
                .is_err()
        );
        processor.process(Job::ClearDepth).await?;
        assert!(
            processor
                .process(Job::PickFocus { x: 3, y: 1 })
                .await
                .is_err()
        );
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        assert!(
            processor
                .process(Job::Render {
                    region: None,
                    settings: settings.clone(),
                })
                .await
                .is_err()
        );
        processor.depth = Some(Array2::from_elem((8, 8), 1.0));
        settings.defocus.use_direct_math = true;
        assert!(matches!(
            processor
                .process(Job::Render {
                    region: None,
                    settings: settings.clone(),
                })
                .await?,
            Output::Render { .. }
        ));
        settings.defocus.set_defocus_mode(DefocusMode::Twod);
        let gpu_result = processor
            .process(Job::Render {
                region: None,
                settings,
            })
            .await?;
        if let Output::Render {
            preview, device, ..
        } = gpu_result
        {
            assert_eq!(preview.size, [8, 8]);
            println!("WGPU device: {device}");
        } else {
            unreachable!();
        }
        Ok(())
    })
}
