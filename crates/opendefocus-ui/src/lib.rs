//! The OpenDefocus viewer: an egui application that loads images and depth maps,
//! runs renders on a background worker, and shows the results. It builds for native
//! windows and for the browser through wasm.

/// Settings panels for the viewer controls.
mod controls;
/// Zoom, pan and pixel mapping for the displayed image.
mod image_view;
/// Background job processing: file loading, focus sampling and renders.
mod processing;
/// Built-in sample scenes.
mod samples;
/// Bottom status panel.
mod status_bar;

/// Layout and interaction tests for the viewer.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod layout_tests;

/// State and availability tests for the viewer.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod state_tests;

use eframe::egui::{self, TextureHandle};
use image_view::{ImageView, viewer_image_at};
#[cfg(test)]
use image_view::{image_pixel, image_region, viewer_image};
use opendefocus::datamodel::{
    IVector4, Settings,
    bokeh_creator::FilterType,
    defocus::DefocusMode,
    render::{FilterMode, Quality, ResultMode},
};
use processing::{InputFile, Job, Output, Processor};
use status_bar::status_bar;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::Duration,
};

use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use futures_channel::mpsc::UnboundedSender as Sender;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::Sender;

/// Starts the viewer on an HTML canvas in the browser.
///
/// # Errors
///
/// Returns a JavaScript error if the web runner cannot start on `canvas`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn start(
    canvas: eframe::web_sys::HtmlCanvasElement,
) -> Result<(), wasm_bindgen::JsValue> {
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| Ok(Box::new(Viewer::new(cc)))),
        )
        .await
}

/// Runs the viewer in a native window until it is closed.
///
/// # Errors
///
/// Returns an error when the native window or graphics context cannot be created.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() -> eframe::Result {
    eframe::run_native(
        "OpenDefocus",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(Viewer::new(cc)))),
    )
}

/// Application state of the viewer: textures, the channel to the worker, UI flags, and the settings being edited.
#[allow(
    clippy::struct_excessive_bools,
    reason = "the viewer tracks independent UI state flags"
)]
struct Viewer {
    /// Sends jobs to the background worker.
    jobs: Sender<Job>,
    /// Receives outputs and errors from the background worker.
    results: Receiver<Result<Output, String>>,
    /// Render settings the user is editing.
    settings: Settings,
    /// Texture of the loaded source image.
    original: Option<TextureHandle>,
    /// Pixels of the source image, used as the base for progressive render updates.
    original_pixels: Option<egui::ColorImage>,
    /// Texture of the latest render result.
    rendered: Option<TextureHandle>,
    /// Texture of the latest bokeh filter preview.
    filter_preview: Option<TextureHandle>,
    /// Display name of the source image.
    image_name: String,
    /// Display name of the loaded depth map, if any.
    depth_name: Option<String>,
    /// Display name of the loaded bokeh filter image, if any.
    filter_name: Option<String>,
    /// Status line text.
    status: String,
    /// Last error message, shown in red.
    error: Option<String>,
    /// True while the worker is processing a job.
    busy: bool,
    /// True when settings changed and the result needs rendering.
    dirty: bool,
    /// Time of the last settings change, used to debounce automatic renders.
    changed_at: Instant,
    /// Whether settings changes render automatically.
    auto_render: bool,
    /// Whether the original image is shown instead of the render.
    show_original: bool,
    /// Zoom and pan state of the image view.
    image_view: ImageView,
    /// Index of the selected control tab.
    tab: u8,
    /// Pixel the user picked as the focus point, if any.
    focus_position: Option<[usize; 2]>,
    /// True when a focus pick still has to be sent to the worker.
    focus_pending: bool,
    /// Part of the image visible in the viewport, which is what gets rendered.
    visible_region: Option<IVector4>,
    /// True when the visible region changed and needs a render.
    viewport_dirty: bool,
    /// True when a render was requested while the worker was busy.
    render_requested: bool,
    /// Row boundary of an in-progress striped render, drawn as a line.
    stripe_boundary: Option<IVector4>,
}

/// Returns the default evaluation settings: depth mode off, reciprocal depth, disc bokeh and high quality.
fn evaluation_settings() -> Settings {
    let mut settings = Settings::default();
    settings.defocus.use_direct_math = false;
    settings.defocus.circle_of_confusion.math = 1;
    settings.bokeh.set_filter_type(FilterType::Disc);
    settings
        .render
        .set_quality(opendefocus::datamodel::render::Quality::Accurate);
    settings
}

/// Draws a labelled combo box over `choices` and updates `value` when the user picks an entry.
///
/// `choices` pairs each value with its display name. Returns true if the selection changed.
fn choose<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    choices: &[(T, &str)],
) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label(label)
        .selected_text(
            choices
                .iter()
                .find(|(v, _)| v == value)
                .map_or("", |(_, name)| *name),
        )
        .show_ui(ui, |ui| {
            for &(option, name) in choices {
                changed |= ui.selectable_value(value, option, name).changed();
            }
        });
    changed
}

impl Viewer {
    /// Creates the viewer and starts the background worker that runs jobs with a [`Processor`].
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        #[cfg(not(target_arch = "wasm32"))]
        let (jobs, input) = mpsc::channel();
        #[cfg(target_arch = "wasm32")]
        let (jobs, mut input) = futures_channel::mpsc::unbounded();
        let (output, results) = mpsc::channel();
        let context = cc.egui_ctx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime,
                Err(error) => {
                    // The receiver is gone when the window closed, so there is nobody to tell.
                    output.send(Err(error.to_string())).ok();
                    context.request_repaint();
                    return;
                }
            };
            let mut processor = Processor::default();
            while let Ok(job) = input.recv() {
                let result = runtime
                    .block_on(processor.process_with_updates(job, |update| {
                        output
                            .send(Ok(update))
                            .map_err(|_| anyhow::anyhow!("Viewer closed"))?;
                        context.request_repaint();
                        Ok(())
                    }))
                    .map_err(|error| format!("{error:#}"));
                if output.send(result).is_err() {
                    break;
                }
                context.request_repaint();
            }
        });
        // shortcut: decoding/preparation share the UI thread; use a Web Worker if large inputs stall it.
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            use futures_util::StreamExt;
            let mut processor = Processor::default();
            while let Some(job) = input.next().await {
                let result = processor
                    .process_with_updates(job, |update| {
                        output
                            .send(Ok(update))
                            .map_err(|_| anyhow::anyhow!("Viewer closed"))?;
                        context.request_repaint();
                        Ok(())
                    })
                    .await
                    .map_err(|error| format!("{error:#}"));
                if output.send(result).is_err() {
                    break;
                }
                context.request_repaint();
            }
        });
        Self::from_channels(jobs, results)
    }

    /// Builds a viewer with default state around the given job sender and result receiver.
    fn from_channels(jobs: Sender<Job>, results: Receiver<Result<Output, String>>) -> Self {
        Self {
            jobs,
            results,
            settings: evaluation_settings(),
            original: None,
            original_pixels: None,
            rendered: None,
            filter_preview: None,
            image_name: String::new(),
            depth_name: None,
            filter_name: None,
            status: "Load an image to begin".into(),
            error: None,
            busy: false,
            dirty: false,
            changed_at: Instant::now(),
            auto_render: true,
            show_original: false,
            image_view: ImageView::default(),
            tab: 0,
            focus_position: None,
            focus_pending: false,
            visible_region: None,
            viewport_dirty: false,
            render_requested: false,
            stripe_boundary: None,
        }
    }

    /// Sends `job` to the worker and marks it busy.
    ///
    /// If the worker has stopped, the error is recorded in the UI instead.
    fn send(&mut self, job: Job) {
        #[cfg(not(target_arch = "wasm32"))]
        let sent = self.jobs.send(job);
        #[cfg(target_arch = "wasm32")]
        let sent = self.jobs.unbounded_send(job);
        match sent {
            Ok(()) => {
                self.busy = true;
                self.stripe_boundary = None;
                self.error = None;
            }
            Err(_) => self.error = Some("Rendering worker stopped; restart the application".into()),
        }
    }

    /// Opens a file dialog and sends `job` with the chosen path.
    ///
    /// Nothing is sent if the dialog is cancelled. In the browser, the job is sent without a path
    /// and the worker opens its own picker.
    fn pick_file(&mut self, job: fn(InputFile) -> Job) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.send(job(path));
        }
        #[cfg(target_arch = "wasm32")]
        self.send(job(()));
    }

    /// Marks the settings as changed and records the time, so automatic rendering can wait
    /// until the user stops editing.
    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.changed_at = Instant::now();
    }

    /// Sends a render of the visible region with the current settings.
    ///
    /// Does nothing unless a filter preview is active or a visible region is known.
    fn render(&mut self) {
        if !self.filter_preview_only() && self.visible_region.is_none() {
            return;
        }
        self.dirty = false;
        self.viewport_dirty = false;
        self.send(Job::Render {
            settings: self.settings.clone(),
            region: self.visible_region,
        });
    }

    /// Applies all pending worker results to the viewer state without blocking.
    fn receive(&mut self, ctx: &egui::Context) {
        loop {
            match self.results.try_recv() {
                Ok(result) => {
                    self.busy = matches!(
                        &result,
                        Ok(Output::Render {
                            complete: false,
                            ..
                        })
                    );
                    if !self.busy {
                        self.stripe_boundary = None;
                    }
                    self.apply_result(ctx, result);
                }
                Err(TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.stripe_boundary = None;
                    self.error = Some("Rendering worker stopped; restart the application".into());
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
    }

    /// Applies one worker result to the viewer state.
    fn apply_result(&mut self, ctx: &egui::Context, result: Result<Output, String>) {
        match result {
            #[cfg(target_arch = "wasm32")]
            Ok(Output::Cancelled) => {}
            Ok(Output::Focus { x, y, value }) => {
                self.settings.defocus.circle_of_confusion.focal_plane = value;
                self.status = format!("Focus at ({x}, {y}): {value}");
                self.mark_dirty();
            }
            Ok(Output::Sample {
                sample,
                preview,
                maximum,
            }) => {
                self.apply_sample(ctx, sample, preview, maximum);
            }
            Ok(Output::Image { name, preview }) => {
                self.set_image(ctx, name, preview);
            }
            Ok(Output::Depth(name)) => {
                self.depth_name = Some(name);
                self.settings.defocus.set_defocus_mode(DefocusMode::Depth);
                self.settings.defocus.use_direct_math = false;
                self.settings.defocus.circle_of_confusion.math = 1;
                self.focus_position = None;
                self.focus_pending = false;
                self.mark_dirty();
            }
            Ok(Output::DepthCleared) => {
                self.depth_name = None;
                self.settings.defocus.set_defocus_mode(DefocusMode::Twod);
                self.settings.render.set_result_mode(ResultMode::Result);
                self.mark_dirty();
            }
            Ok(Output::Filter(name)) => {
                self.filter_name = Some(name);
                self.settings.render.filter.set_mode(FilterMode::Image);
                self.settings.render.filter.preview = false;
                self.mark_dirty();
            }
            Ok(Output::FilterCleared) => {
                self.filter_name = None;
                self.settings.render.filter.set_mode(FilterMode::Simple);
                self.settings.render.filter.preview = false;
                self.mark_dirty();
            }
            Ok(Output::FilterPreview {
                preview,
                elapsed,
                device,
            }) => {
                self.apply_filter_preview(ctx, preview, elapsed, &device);
            }
            Ok(Output::Render {
                preview,
                region,
                requested,
                complete,
                elapsed,
                initialization,
                device,
            }) => {
                self.apply_render(
                    ctx,
                    preview,
                    (region, requested),
                    complete,
                    (elapsed, initialization),
                    &device,
                );
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// Replaces the state with a bundled sample scene and its evaluation settings.
    fn apply_sample(
        &mut self,
        ctx: &egui::Context,
        sample: &'static samples::Sample,
        preview: egui::ColorImage,
        maximum: f32,
    ) {
        self.settings = evaluation_settings();
        self.settings.defocus.circle_of_confusion.size = 10.0;
        self.settings.defocus.circle_of_confusion.max_size = maximum;
        self.filter_name = None;
        self.set_image(ctx, sample.name.to_owned(), preview);
        if sample.depth.is_some() {
            self.depth_name = Some(if sample.direct_coc {
                format!("{} (bundled, sign-correct direct CoC)", sample.name)
            } else {
                format!("{} (bundled depth)", sample.name)
            });
            self.settings.defocus.set_defocus_mode(DefocusMode::Depth);
            if sample.direct_coc {
                self.settings.defocus.use_direct_math = true;
                self.settings.defocus.circle_of_confusion.max_size = maximum;
                self.settings.defocus.camera_max_size = maximum;
            }
            if let Some(lens) = &sample.lens {
                let coc = &mut self.settings.defocus.circle_of_confusion;
                coc.math = lens.math;
                coc.focal_plane = lens.focal_plane;
                coc.size = lens.size;
                coc.max_size = lens.max_size;
            }
        }
    }

    /// Shows the rendered bokeh filter preview and reports its render time.
    fn apply_filter_preview(
        &mut self,
        ctx: &egui::Context,
        preview: egui::ColorImage,
        elapsed: Duration,
        device: &str,
    ) {
        self.status = format!(
            "Filter {} × {} | {:.1} ms | {}: {}",
            preview.size[0],
            preview.size[1],
            elapsed.as_secs_f64() * 1000.0,
            "WGPU",
            device
        );
        if self
            .filter_preview
            .as_ref()
            .is_none_or(|texture| texture.size() != preview.size)
        {
            self.image_view = ImageView::default();
        }
        self.filter_preview =
            Some(ctx.load_texture("filter-preview", preview, egui::TextureOptions::default()));
    }

    /// Merges a rendered region into the result texture and updates the status line.
    fn apply_render(
        &mut self,
        ctx: &egui::Context,
        preview: egui::ColorImage,
        (region, requested): (IVector4, IVector4),
        complete: bool,
        (elapsed, initialization): (Duration, Duration),
        device: &str,
    ) {
        self.stripe_boundary = (!complete).then_some(region);
        let megapixels =
            ((requested.z - requested.x) as f64 * (region.w - requested.y) as f64) / 1e6;
        let progress = (region.w - requested.y) as f64 / (requested.w - requested.y) as f64 * 100.0;
        self.status = format!(
            "Adaptive renderer | {} × {} | {:.0}% | {:.1} ms | {:.2} MP/s | {:.1} ms init | {}: {}",
            requested.z - requested.x,
            requested.w - requested.y,
            progress,
            elapsed.as_secs_f64() * 1000.0,
            megapixels / elapsed.as_secs_f64().max(f64::EPSILON),
            initialization.as_secs_f64() * 1000.0,
            "WGPU",
            device
        );
        if let Some(original) = &self.original_pixels {
            let texture = self.rendered.get_or_insert_with(|| {
                ctx.load_texture("result", original.clone(), egui::TextureOptions::default())
            });
            texture.set_partial(
                [region.x as usize, region.y as usize],
                preview,
                egui::TextureOptions::default(),
            );
        }
    }

    /// Draws the top panel: the file buttons, view shortcuts and the names of the loaded files.
    fn files_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("files").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(!self.busy, |ui| {
                    self.file_buttons(ui);
                });
                ui.checkbox(&mut self.show_original, "Show original");
                if ui.button("Fit").clicked() {
                    self.image_view = ImageView::default();
                }
                if ui.button("1:1").clicked() {
                    self.image_view = ImageView {
                        zoom: Some(1.0),
                        ..Default::default()
                    };
                }
            });
            if !self.image_name.is_empty() {
                ui.label(&self.image_name);
            }
            if let Some(depth) = &self.depth_name {
                ui.label(format!("Depth (red): {depth}"));
            }
            if let Some(filter) = &self.filter_name {
                ui.label(format!("Filter: {filter}"));
            }
        });
    }

    /// Draws the buttons that load and clear the test images, the image, the depth map and the filter.
    fn file_buttons(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Test images", |ui| {
            for sample in samples::SAMPLES {
                if ui.button(sample.name).clicked() {
                    self.send(Job::Sample(sample));
                    ui.close();
                }
            }
        });
        if ui.button("Load image…").clicked() {
            self.pick_file(Job::Image);
        }
        if ui
            .add_enabled(self.original.is_some(), egui::Button::new("Load depth…"))
            .clicked()
        {
            self.pick_file(Job::Depth);
        }
        if ui
            .add_enabled(self.depth_name.is_some(), egui::Button::new("Clear depth"))
            .clicked()
        {
            self.send(Job::ClearDepth);
        }
        if ui.button("Load filter…").clicked() {
            self.pick_file(Job::Filter);
        }
        if ui
            .add_enabled(
                self.filter_name.is_some(),
                egui::Button::new("Clear filter"),
            )
            .clicked()
        {
            self.send(Job::ClearFilter);
        }
    }

    /// Shows `preview` as the new source image.
    ///
    /// Resets the view, render output, focus and depth link, and sets the evaluation defaults.
    fn set_image(&mut self, ctx: &egui::Context, name: String, preview: egui::ColorImage) {
        self.focus_position = None;
        self.focus_pending = false;
        self.image_view = ImageView::default();
        self.visible_region = None;
        self.status = format!("{} × {}", preview.size[0], preview.size[1]);
        self.settings.render.center.x = preview.size[0] as f32 * 0.5;
        self.settings.render.center.y = preview.size[1] as f32 * 0.5;
        self.original =
            Some(ctx.load_texture("original", preview.clone(), egui::TextureOptions::default()));
        self.original_pixels = Some(preview);
        self.rendered = None;
        self.filter_preview = None;
        self.image_name = name;
        self.depth_name = None;
        self.settings.defocus.set_defocus_mode(DefocusMode::Twod);
        self.settings.defocus.use_direct_math = false;
        self.settings.defocus.circle_of_confusion.math = 1;
        self.settings.render.set_result_mode(ResultMode::Result);
        self.mark_dirty();
    }

    /// Returns true when a focus point can be picked: a depth map is loaded, depth mode is active,
    /// and direct math and the filter preview are off.
    fn can_pick_focus(&self) -> bool {
        self.depth_name.is_some()
            && self.settings.defocus.defocus_mode() == DefocusMode::Depth
            && !self.settings.defocus.use_direct_math
            && !self.settings.render.filter.preview
    }

    /// Returns true when the bokeh filter preview is the only thing to render.
    fn filter_preview_only(&self) -> bool {
        self.settings.render.filter.preview
            && self.settings.render.filter.mode() == FilterMode::BokehCreator
    }

    /// Returns true when an image is loaded or the filter preview can run.
    fn can_render(&self) -> bool {
        self.original.is_some() || self.filter_preview_only()
    }
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        self.files_panel(ui);
        status_bar(
            ui,
            &self.status,
            self.dirty,
            self.busy,
            self.error.as_deref(),
        );
        egui::Panel::right("controls")
            .default_size(310.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy, |ui| {
                        self.controls(ui);
                    });
                });
            });
        let picking = self.can_pick_focus();
        let focus_position = if self.settings.defocus.defocus_mode() == DefocusMode::Depth
            && !self.settings.render.filter.preview
        {
            self.original.as_ref().map(|image| {
                self.focus_position
                    .unwrap_or([image.size()[0] / 2, image.size()[1] / 2])
            })
        } else {
            None
        };
        let stripe = if self.show_original {
            None
        } else {
            self.stripe_boundary
        };
        let mut picked = None;
        let mut visible = None;
        egui::CentralPanel::default().show(ui, |ui| {
            let texture = if self.show_original {
                self.original.as_ref()
            } else if self.filter_preview_only() {
                self.filter_preview.as_ref().or(self.original.as_ref())
            } else {
                self.rendered.as_ref().or(self.original.as_ref())
            };
            if let Some(texture) = texture {
                let rect = self.image_view.image_rect(ui, texture.size_vec2());
                (picked, visible) =
                    viewer_image_at(ui, texture, rect, picking, stripe, focus_position);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Load an image and optionally a matching depth map");
                });
            }
        });
        if visible != self.visible_region {
            self.visible_region = visible;
            if visible.is_some() {
                self.viewport_dirty = true;
                self.mark_dirty();
            }
        }
        if let Some([x, y]) = picked
            && self.focus_position != Some([x, y])
        {
            self.focus_position = Some([x, y]);
            self.focus_pending = true;
        }
        if self.focus_pending && !self.busy && picking {
            self.focus_pending = false;
            if let Some([x, y]) = self.focus_position {
                self.send(Job::PickFocus { x, y });
            }
        }
        if self.render_requested && !self.busy && self.can_render() {
            self.render_requested = false;
            self.render();
        }
        if (self.auto_render || self.viewport_dirty)
            && self.dirty
            && !self.busy
            && self.can_render()
            && (self.visible_region.is_some() || self.filter_preview_only())
        {
            if self.changed_at.elapsed() >= Duration::from_millis(150)
                && !ctx.input(|i| i.pointer.any_down())
            {
                self.render();
            } else {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
        }
    }
}
