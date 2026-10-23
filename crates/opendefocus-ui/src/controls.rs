//! Side-panel controls of the viewer: the defocus, bokeh and non-uniform settings tabs.

use super::{DefocusMode, FilterMode, FilterType, Quality, ResultMode, Viewer, choose, egui};
use opendefocus::datamodel::{
    circle_of_confusion::{CameraData, WorldUnit},
    non_uniform::AxialAberrationType,
};

/// Draws a slider clamped to `range` with `label`, returning true if the value changed.
fn slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
) -> bool {
    ui.add(egui::Slider::new(value, range).text(label))
        .changed()
}

/// Draws a slider labelled `label` whose value may leave `range`, returning true if it changed.
///
/// The range only guides the slider; typed or dragged values outside it are kept, so the
/// setting is not limited to the displayed range.
fn slider_unclamped(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
) -> bool {
    ui.add(
        egui::Slider::new(value, range)
            .clamping(egui::SliderClamping::Never)
            .text(label),
    )
    .changed()
}

impl Viewer {
    /// Draws the whole control panel: tab selector, the selected tab, and the render button.
    ///
    /// Settings changes mark the result as dirty, and the render button requests a render.
    pub(super) fn controls(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.heading("OpenDefocus");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, 0, "Defocus");
            ui.selectable_value(&mut self.tab, 1, "Bokeh");
            ui.selectable_value(&mut self.tab, 2, "Non-uniform");
        });
        ui.separator();
        changed |= match self.tab {
            1 => self.bokeh_controls(ui),
            2 => self.non_uniform_controls(ui),
            _ => self.defocus_controls(ui),
        };
        ui.separator();
        ui.checkbox(&mut self.auto_render, "Auto render");
        if changed {
            self.mark_dirty();
        }
        if ui
            .add_enabled(
                self.can_render(),
                egui::Button::new("Render / measure again"),
            )
            .clicked()
        {
            self.render_requested = true;
        }
        ui.small("Render time for the requested region; excludes loading, renderer initialization and viewer conversion.");
    }

    /// Draws the defocus tab: mode, math, quality, focus and CoC sliders, and camera settings.
    ///
    /// Returns true if any setting changed.
    fn defocus_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.label("Renderer: Adaptive GPU");
        ui.separator();
        let mut mode = self.settings.defocus.defocus_mode();
        let modes = if self.depth_name.is_some() {
            vec![(DefocusMode::Twod, "2D"), (DefocusMode::Depth, "Depth")]
        } else {
            vec![(DefocusMode::Twod, "2D")]
        };
        changed |= choose(ui, "mode", &mut mode, &modes);
        self.settings.defocus.set_defocus_mode(mode);
        changed |= self.math_controls(ui, mode);
        changed |= self.quality_controls(ui);
        changed |= self.coc_controls(ui, mode);
        changed |= self.camera_controls(ui, mode);
        let defocus = &mut self.settings.defocus;
        changed |= slider(
            ui,
            &mut defocus.circle_of_confusion.pixel_aspect,
            0.25..=4.0,
            "pixel aspect",
        );
        changed |= slider(
            ui,
            &mut defocus.size_multiplier,
            0.0..=4.0,
            "size multiplier",
        );
        changed
    }

    /// Draws the focal-plane overlay switch and, in depth mode, the CoC math and world unit choices.
    ///
    /// Returns true if any setting changed.
    fn math_controls(&mut self, ui: &mut egui::Ui, mode: DefocusMode) -> bool {
        let mut changed = false;
        let defocus = &mut self.settings.defocus;
        if mode == DefocusMode::Depth {
            let mut overlay = self.settings.render.result_mode() == ResultMode::FocalPlaneSetup;
            changed |= ui.checkbox(&mut overlay, "Focal-plane setup").changed();
            self.settings.render.set_result_mode(if overlay {
                ResultMode::FocalPlaneSetup
            } else {
                ResultMode::Result
            });
            if overlay {
                changed |= ui
                    .checkbox(&mut defocus.show_image, "Overlay on image")
                    .changed();
            }
        } else {
            self.settings.render.set_result_mode(ResultMode::Result);
        }
        if mode == DefocusMode::Depth {
            let mut math = if defocus.use_direct_math {
                2
            } else {
                defocus.circle_of_confusion.math
            };
            changed |= choose(
                ui,
                "math",
                &mut math,
                &[(0, "Real"), (1, "1/Z"), (2, "Direct (radius px)")],
            );
            defocus.use_direct_math = math == 2;
            if math == 2 {
                defocus.circle_of_confusion.camera_data = None;
            }
            if math != 2 {
                defocus.circle_of_confusion.math = math;
            }
            if let Some(camera) = &mut defocus.circle_of_confusion.camera_data {
                let mut unit = camera.world_unit();
                changed |= choose(
                    ui,
                    "world unit",
                    &mut unit,
                    &[
                        (WorldUnit::Mm, "mm"),
                        (WorldUnit::Cm, "cm"),
                        (WorldUnit::Dm, "dm"),
                        (WorldUnit::M, "m"),
                        (WorldUnit::In, "in"),
                        (WorldUnit::Ft, "ft"),
                    ],
                );
                camera.set_world_unit(unit);
            }
        }
        changed
    }

    /// Draws the quality choice: Fast (sampled gather) or Accurate (exact source-pixel integration).
    ///
    /// Returns true if any setting changed.
    fn quality_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.separator();
        let mut quality = self.settings.render.quality();
        changed |= choose(
            ui,
            "quality",
            &mut quality,
            &[(Quality::Fast, "Fast"), (Quality::Accurate, "Accurate")],
        );
        self.settings.render.set_quality(quality);
        changed
    }

    /// Draws the focal plane, protect, size and maximum radius controls.
    ///
    /// Returns true if any setting changed.
    fn coc_controls(&mut self, ui: &mut egui::Ui, mode: DefocusMode) -> bool {
        let mut changed = false;
        let defocus = &mut self.settings.defocus;
        ui.separator();
        if mode == DefocusMode::Depth {
            let direct = defocus.use_direct_math;
            ui.add_enabled_ui(!direct, |ui| {
                changed |= ui
                    .add(
                        egui::Slider::new(
                            &mut defocus.circle_of_confusion.focal_plane,
                            0.0..=100.0,
                        )
                        .clamping(egui::SliderClamping::Never)
                        .text("focal plane"),
                    )
                    .changed();
                ui.small("Focus point: drag the dot in the image");
                changed |= slider(
                    ui,
                    &mut defocus.circle_of_confusion.protect,
                    0.0..=10.0,
                    "protect",
                );
            });
        }
        ui.add_enabled_ui(
            mode == DefocusMode::Twod
                || (defocus.circle_of_confusion.camera_data.is_none() && !defocus.use_direct_math),
            |ui| {
                changed |= slider(
                    ui,
                    &mut defocus.circle_of_confusion.size,
                    0.0..=100.0,
                    "size (radius px)",
                );
            },
        );
        if mode == DefocusMode::Depth {
            let maximum = if defocus.circle_of_confusion.camera_data.is_some() {
                &mut defocus.camera_max_size
            } else {
                &mut defocus.circle_of_confusion.max_size
            };
            changed |= slider(ui, maximum, 1.0..=200.0, "maximum radius");
        }
        changed
    }

    /// Draws the physical camera switch and its lens and filmback controls in depth mode.
    ///
    /// Returns true if any setting changed.
    fn camera_controls(&mut self, ui: &mut egui::Ui, mode: DefocusMode) -> bool {
        let mut changed = false;
        let defocus = &mut self.settings.defocus;
        if mode == DefocusMode::Depth {
            ui.add_enabled_ui(!defocus.use_direct_math, |ui| {
                let mut camera = defocus.circle_of_confusion.camera_data.is_some();
                if ui.checkbox(&mut camera, "Physical camera").changed() {
                    defocus.circle_of_confusion.camera_data = if camera {
                        Some(CameraData::default())
                    } else {
                        None
                    };
                    changed = true;
                }
                if let Some(camera) = &mut defocus.circle_of_confusion.camera_data {
                    changed |= slider(
                        ui,
                        &mut camera.focal_length,
                        1.0..=300.0,
                        "focal length (mm)",
                    );
                    changed |= slider(ui, &mut camera.f_stop, 0.1..=32.0, "f-stop");
                    changed |= slider(
                        ui,
                        &mut camera.filmback.width,
                        1.0..=70.0,
                        "filmback width (mm)",
                    );
                    changed |= slider(
                        ui,
                        &mut camera.filmback.height,
                        1.0..=70.0,
                        "filmback height (mm)",
                    );
                }
            });
        }
        changed
    }

    /// Draws the bokeh tab: filter type, generated bokeh shape and noise, and aspect ratio.
    ///
    /// Returns true if any setting changed.
    fn bokeh_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        changed |= ui
            .checkbox(&mut self.settings.render.filter.preview, "Preview filter")
            .changed();
        ui.separator();
        let mode = self.settings.render.filter.mode();
        let mut filter = (
            mode,
            if mode == FilterMode::BokehCreator {
                self.settings.bokeh.filter_type()
            } else {
                FilterType::Disc
            },
        );
        let filters = [
            ((FilterMode::Simple, FilterType::Disc), "Simple"),
            ((FilterMode::BokehCreator, FilterType::Disc), "Disc"),
            ((FilterMode::BokehCreator, FilterType::Blade), "Bladed"),
            ((FilterMode::Image, FilterType::Disc), "Image"),
        ];
        changed |= choose(ui, "filter type", &mut filter, &filters);
        self.settings.render.filter.set_mode(filter.0);
        if filter.0 == FilterMode::BokehCreator {
            self.settings.bokeh.set_filter_type(filter.1);
        }
        if filter.0 == FilterMode::Image {
            ui.small("Crop the filter to its bokeh bounds for accurate sampling.");
        }
        let shape = self.settings.bokeh.filter_type();
        let generated = filter.0 == FilterMode::BokehCreator;
        if filter.0 == FilterMode::Simple {
            ui.small("Select Disc or Bladed to edit the bokeh shape.");
        }
        let bokeh = &mut self.settings.bokeh;
        ui.add_enabled_ui(generated, |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.settings.render.filter.resolution)
                        .range(1..=u32::MAX)
                        .prefix("resolution "),
                )
                .changed();
            ui.separator();
            changed |= slider(ui, &mut bokeh.ring_color, 0.0..=1.0, "ring color");
            changed |= slider(ui, &mut bokeh.inner_color, 0.001..=1.0, "inner color");
            for (value, label) in [
                (&mut bokeh.ring_size, "ring size"),
                (&mut bokeh.outer_blur, "outer blur"),
                (&mut bokeh.inner_blur, "inner blur"),
            ] {
                changed |= slider_unclamped(ui, value, 0.0..=1.0, label);
            }
            ui.add_enabled_ui(shape == FilterType::Blade, |ui| {
                changed |= ui
                    .add(egui::Slider::new(&mut bokeh.blades, 3..=16).text("blades"))
                    .changed();
                changed |= slider_unclamped(ui, &mut bokeh.angle, -180.0..=180.0, "angle");
                changed |= slider_unclamped(ui, &mut bokeh.curvature, 0.0..=1.0, "curvature");
            });
            ui.separator();
            changed |= slider_unclamped(ui, &mut bokeh.noise.size, 0.0..=1.0, "noise size");
            changed |=
                slider_unclamped(ui, &mut bokeh.noise.intensity, 0.0..=1.0, "noise intensity");
            changed |= ui
                .add(egui::DragValue::new(&mut bokeh.noise.seed).prefix("noise seed "))
                .changed();
        });
        ui.separator();
        ui.add_enabled_ui(filter.0 != FilterMode::Image, |ui| {
            changed |= slider_unclamped(ui, &mut bokeh.aspect_ratio, 0.0..=2.0, "aspect ratio");
        });

        changed
    }

    /// Draws the non-uniform tab: catseye, barndoors, astigmatism, axial aberration, and center.
    ///
    /// Returns true if any setting changed.
    fn non_uniform_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let non_uniform = &mut self.settings.non_uniform;
        let catseye = &mut non_uniform.catseye;
        changed |= ui.checkbox(&mut catseye.enable, "Enable catseye").changed();
        ui.add_enabled_ui(catseye.enable, |ui| {
            changed |= slider(ui, &mut catseye.amount, 0.0..=2.0, "catseye amount");
            changed |= ui
                .checkbox(&mut catseye.inverse, "Inverse catseye")
                .changed();
            changed |= ui
                .checkbox(
                    &mut catseye.inverse_foreground,
                    "Inverse catseye in foreground",
                )
                .changed();
            changed |= slider(ui, &mut catseye.gamma, 0.2..=4.0, "catseye gamma");
            changed |= slider(ui, &mut catseye.softness, 0.01..=1.0, "softness");
            changed |= ui
                .checkbox(
                    &mut catseye.relative_to_screen,
                    "Relative to screen dimensions",
                )
                .changed();
        });
        ui.separator();
        let barndoors = &mut non_uniform.barndoors;
        changed |= ui
            .checkbox(&mut barndoors.enable, "Enable barndoors")
            .changed();
        ui.add_enabled_ui(barndoors.enable, |ui| {
            changed |= slider(ui, &mut barndoors.amount, 0.0..=2.0, "barndoors amount");
            changed |= ui
                .checkbox(&mut barndoors.inverse, "Inverse barndoors")
                .changed();
            changed |= ui
                .checkbox(
                    &mut barndoors.inverse_foreground,
                    "Inverse barndoors in foreground",
                )
                .changed();
            changed |= slider(ui, &mut barndoors.gamma, 0.2..=4.0, "barndoors gamma");
            changed |= slider(ui, &mut barndoors.top, 0.0..=100.0, "top");
            changed |= slider(ui, &mut barndoors.bottom, 0.0..=100.0, "bottom");
            changed |= slider(ui, &mut barndoors.left, 0.0..=100.0, "left");
            changed |= slider(ui, &mut barndoors.right, 0.0..=100.0, "right");
        });
        ui.separator();
        let astigmatism = &mut non_uniform.astigmatism;
        changed |= ui
            .checkbox(&mut astigmatism.enable, "Enable astigmatism")
            .changed();
        ui.add_enabled_ui(astigmatism.enable, |ui| {
            changed |= slider(ui, &mut astigmatism.amount, 0.0..=2.0, "astigmatism amount");
            changed |= slider(ui, &mut astigmatism.gamma, 0.2..=4.0, "astigmatism gamma");
        });
        ui.separator();
        let axial = &mut non_uniform.axial_aberration;
        changed |= ui
            .checkbox(&mut axial.enable, "Enable axial aberration")
            .changed();
        ui.add_enabled_ui(axial.enable, |ui| {
            changed |= slider(ui, &mut axial.amount, -1.0..=1.0, "axial amount");
            let mut color = axial.color_type();
            changed |= choose(
                ui,
                "colors",
                &mut color,
                &[
                    (AxialAberrationType::RedBlue, "Red / blue"),
                    (AxialAberrationType::BlueYellow, "Blue / yellow"),
                    (AxialAberrationType::GreenPurple, "Green / purple"),
                ],
            );
            axial.set_color_type(color);
        });
        changed |= ui
            .checkbox(
                &mut non_uniform.inverse_foreground,
                "Inverse bokeh in foreground",
            )
            .changed();
        ui.separator();
        if let Some(image) = &self.original {
            ui.separator();
            changed |= slider(
                ui,
                &mut self.settings.render.center.x,
                0.0..=image.size_vec2().x,
                "center X (px)",
            );
            changed |= slider(
                ui,
                &mut self.settings.render.center.y,
                0.0..=image.size_vec2().y,
                "center Y (px)",
            );
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Checks that every bokeh control is drawn, in legacy order, in each filter mode.
    #[test]
    fn all_bokeh_knobs_stay_visible_in_legacy_knob_order_in_every_mode() {
        let (jobs, _input) = mpsc::channel();
        let (_output, results) = mpsc::channel();
        let mut viewer = Viewer::from_channels(jobs, results);
        for (mode, shape) in [
            (FilterMode::Simple, FilterType::Disc),
            (FilterMode::BokehCreator, FilterType::Disc),
            (FilterMode::BokehCreator, FilterType::Blade),
            (FilterMode::Image, FilterType::Disc),
        ] {
            viewer.settings.render.filter.set_mode(mode);
            viewer.settings.bokeh.set_filter_type(shape);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 1500.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        viewer.bokeh_controls(ui);
                    });
                },
            );
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        Some(text.galley.job.text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            output.textures_delta.clear();
            let mut previous = 0;
            for label in [
                "Preview filter",
                "filter type",
                "resolution ",
                "ring color",
                "inner color",
                "ring size",
                "outer blur",
                "inner blur",
                "blades",
                "angle",
                "curvature",
                "noise size",
                "noise intensity",
                "noise seed ",
                "aspect ratio",
            ] {
                let index = labels
                    .iter()
                    .position(|text| text.starts_with(label))
                    .unwrap_or_else(|| panic!("Missing {label} in {mode:?}: {labels:?}"));
                assert!(index >= previous, "{label} is out of legacy knob order");
                previous = index;
            }
        }
    }
}
