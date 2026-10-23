//! Layout and interaction tests for the viewer: depth-load state, zoom and pan, focus
//! picking, the stripe marker and the status bar.

use super::*;

/// Checks that a sample or depth load switches to depth mode with 1/Z math, whatever the previous settings.
#[test]
fn every_depth_load_resets_direct_math_to_one_over_z() {
    let ctx = egui::Context::default();
    let (jobs, _input) = mpsc::channel();
    let (output, results) = mpsc::channel();
    let mut viewer = Viewer::from_channels(jobs, results);
    assert!(!viewer.settings.defocus.use_direct_math);
    assert_eq!(viewer.settings.defocus.circle_of_confusion.math, 1);
    for result in [
        Output::Sample {
            sample: &samples::SAMPLES[2],
            preview: egui::ColorImage::new([2, 2], vec![egui::Color32::BLACK; 4]),
            maximum: 10.0,
        },
        Output::Depth("new-depth.exr".into()),
    ] {
        viewer.settings.defocus.use_direct_math = true;
        viewer.settings.defocus.circle_of_confusion.math = 0;
        output.send(Ok(result)).unwrap();
        viewer.receive(&ctx);
        assert_eq!(viewer.settings.defocus.defocus_mode(), DefocusMode::Depth);
        assert!(!viewer.settings.defocus.use_direct_math);
        assert_eq!(viewer.settings.defocus.circle_of_confusion.math, 1);
    }
}

/// Checks that wheel zoom keeps the pixel under the cursor fixed, and that middle-drag pans into newly exposed image area.
#[test]
fn wheel_zoom_keeps_cursor_pixel_and_middle_drag_exposes_new_region() {
    fn frame(ctx: &egui::Context, view: &mut ImageView, events: Vec<egui::Event>) -> egui::Rect {
        let mut rect = egui::Rect::ZERO;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(256.0, 128.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        rect = view.image_rect(ui, egui::vec2(512.0, 256.0));
                    });
            },
        );
        output.textures_delta.clear();
        rect
    }
    let ctx = egui::Context::default();
    let mut view = ImageView {
        zoom: Some(1.0),
        ..Default::default()
    };
    let cursor = egui::pos2(80.0, 40.0);
    let before = frame(&ctx, &mut view, vec![egui::Event::PointerMoved(cursor)]);
    let after = frame(
        &ctx,
        &mut view,
        vec![egui::Event::MouseWheel {
            phase: egui::TouchPhase::Move,
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 100.0),
            modifiers: egui::Modifiers::NONE,
            source: egui::MouseWheelSource::Wheel,
        }],
    );
    assert!(after.width() > before.width());
    let before_uv = (cursor - before.min) / before.size();
    let after_uv = (cursor - after.min) / after.size();
    assert!((before_uv - after_uv).length() < 1e-5);
    let clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256.0, 128.0));
    let before_region = image_region(before, clip, [512, 256]).unwrap();
    let after_region = image_region(after, clip, [512, 256]).unwrap();
    assert!(after_region.z - after_region.x < before_region.z - before_region.x);

    let ctx = egui::Context::default();
    let mut view = ImageView {
        zoom: Some(1.0),
        ..Default::default()
    };
    let before = frame(&ctx, &mut view, vec![egui::Event::PointerMoved(cursor)]);
    frame(
        &ctx,
        &mut view,
        vec![egui::Event::PointerButton {
            pos: cursor,
            button: egui::PointerButton::Middle,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let after = frame(
        &ctx,
        &mut view,
        vec![egui::Event::PointerMoved(cursor + egui::vec2(20.0, 10.0))],
    );
    assert_eq!(after.min - before.min, egui::vec2(20.0, 10.0));
    let after_region = image_region(after, clip, [512, 256]).unwrap();
    assert_eq!(after_region.x, before_region.x - 20);
    assert_eq!(after_region.y, before_region.y - 10);
    assert_eq!(after_region.z, before_region.z - 20);
    assert_eq!(after_region.w, before_region.w - 10);
}

/// Checks that the focus dot can be dragged to pick a pixel, and that it ignores input when picking is disabled.
#[test]
fn focus_dot_can_be_dragged_and_disabled() {
    for enabled in [true, false] {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "focus-test",
            egui::ColorImage::new([512, 256], vec![egui::Color32::BLACK; 512 * 256]),
            egui::TextureOptions::default(),
        );
        let mut focus = [256, 128];
        for (position, button) in [
            (egui::pos2(128.25, 64.25), None),
            (egui::pos2(128.25, 64.25), Some(true)),
            (egui::pos2(160.0, 80.0), None),
            (egui::pos2(160.0, 80.0), Some(false)),
        ] {
            let mut events = vec![egui::Event::PointerMoved(position)];
            if let Some(pressed) = button {
                events.push(egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(256.0, 128.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            if let (Some(pixel), _) = viewer_image(
                                ui,
                                &texture,
                                egui::vec2(256.0, 128.0),
                                enabled,
                                None,
                                Some(focus),
                            ) {
                                focus = pixel;
                            }
                        });
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(focus, if enabled { [320, 160] } else { [256, 128] });
    }
}

/// Checks that the stripe marker follows the image scale and disappears when no partial render is pending.
#[test]
fn stripe_marker_tracks_image_scale_and_disappears_without_progress() {
    let ctx = egui::Context::default();
    let texture = ctx.load_texture(
        "stripe-test",
        egui::ColorImage::new([512, 256], vec![egui::Color32::BLACK; 512 * 256]),
        egui::TextureOptions::default(),
    );
    for stripe in [
        Some(IVector4 {
            x: 100,
            y: 0,
            z: 400,
            w: 64,
        }),
        None,
    ] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(256.0, 128.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        viewer_image(ui, &texture, egui::vec2(256.0, 128.0), false, stripe, None);
                    });
            },
        );
        let lines: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::LineSegment { points, stroke } = &shape.shape {
                    (stroke.color == egui::Color32::WHITE).then_some((*points, stroke.width))
                } else {
                    None
                }
            })
            .collect();
        if stripe.is_some() {
            assert_eq!(
                lines,
                vec![([egui::pos2(50.0, 32.0), egui::pos2(200.0, 32.0)], 1.0)]
            );
        } else {
            assert_eq!(lines, Vec::new());
        }
        output.textures_delta.clear();
    }
}

/// Checks that a click on the displayed image maps to the image pixel shown at that position.
#[test]
fn focus_coordinates_follow_displayed_image() {
    let fit = egui::Rect::from_min_size(egui::pos2(100.0, 40.0), egui::vec2(256.0, 128.0));
    assert_eq!(
        image_pixel(fit, egui::pos2(228.0, 104.0), [512, 256]),
        Some([256, 128])
    );
    assert_eq!(image_pixel(fit, fit.max, [512, 256]), Some([511, 255]));
    assert_eq!(image_pixel(fit, egui::pos2(99.0, 40.0), [512, 256]), None);
    let scrolled = egui::Rect::from_min_size(egui::pos2(-200.0, -100.0), egui::vec2(512.0, 256.0));
    assert_eq!(
        image_pixel(scrolled, egui::pos2(10.0, 20.0), [512, 256]),
        Some([210, 120])
    );
    let clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 100.0));
    assert_eq!(
        image_region(scrolled, clip, [512, 256]),
        Some(IVector4 {
            x: 200,
            y: 100,
            z: 320,
            w: 200
        })
    );
    assert_eq!(
        image_region(fit, clip, [512, 256]),
        Some(IVector4 {
            x: 0,
            y: 0,
            z: 40,
            w: 120
        })
    );
}

/// Checks that clicks in the letterbox area outside the image do not produce a pixel.
#[test]
fn focus_picking_ignores_letterboxing() {
    fn click(position: egui::Pos2) -> Option<[usize; 2]> {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "test",
            egui::ColorImage::new([512, 256], vec![egui::Color32::WHITE; 512 * 256]),
            egui::TextureOptions::default(),
        );
        let mut picked = None;
        for pressed in [None, Some(true), Some(false)] {
            let events = pressed.map_or_else(Vec::new, |pressed| {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            });
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 240.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                picked = viewer_image(
                                    ui,
                                    &texture,
                                    egui::vec2(256.0, 128.0),
                                    true,
                                    None,
                                    None,
                                )
                                .0;
                            });
                        });
                },
            );
            output.textures_delta.clear();
        }
        picked
    }
    assert_eq!(click(egui::pos2(100.0, 100.0)), Some([136, 88]));
    assert_eq!(click(egui::pos2(10.0, 10.0)), None);
}

/// Checks that changing the status text leaves the size of the viewer area unchanged.
#[test]
fn status_changes_preserve_viewer_area() {
    let ctx = egui::Context::default();
    let mut expected = None;
    for (status, dirty, error) in [
        ("512 × 512", false, None),
        ("512 × 512", true, None),
        (
            "1920 × 1080 | 1234.5 ms | 1.23 MP/s | GPU: A long device name that exceeds the window width",
            false,
            None,
        ),
        (
            "1920 × 1080",
            true,
            Some(
                "A rendering error with enough detail to exceed the window width and otherwise wrap onto several lines",
            ),
        ),
        ("512 × 512", false, None),
    ] {
        for busy in [false, true] {
            let mut viewer_area = egui::Rect::ZERO;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 240.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    status_bar(ui, status, dirty, busy, error);
                    viewer_area = ui.available_rect_before_wrap();
                },
            );
            output.textures_delta.clear();
            assert_eq!(*expected.get_or_insert(viewer_area), viewer_area);
        }
    }
}
