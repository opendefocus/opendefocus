//! Background image processing for the UI: loads inputs, samples the depth for
//! focus picking, and runs renders against a cached `OpenDefocusRenderer`.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::time::Duration;
use web_time::Instant;

/// Path of an input file on native targets.
#[cfg(not(target_arch = "wasm32"))]
pub type InputFile = PathBuf;
/// Browser input placeholder: file requests open an async picker instead of using filesystem paths.
#[cfg(target_arch = "wasm32")]
pub type InputFile = ();

use crate::samples::Sample;
use anyhow::{Context, Result, ensure};
use eframe::egui::ColorImage;
use image::{DynamicImage, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
use ndarray::{Array2, Array3, s};
use opendefocus::{
    OpenDefocusRenderer,
    datamodel::{IVector4, Settings, UVector2, defocus::DefocusMode, render::RenderSpecs},
};

/// Identifies the renderer configuration cached by the [`Processor`].
///
/// It is a unit value today, so one renderer is created on first use and reused afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RendererKey;

/// Work requested from the [`Processor`] by the UI.
pub enum Job {
    /// Load one of the embedded sample scenes, replacing the current image.
    Sample(&'static Sample),
    /// Read the depth value at a pixel to use as the focal plane.
    PickFocus {
        /// Column of the picked pixel.
        x: usize,
        /// Row of the picked pixel.
        y: usize,
    },
    /// Load an image file as the new source, clearing any depth map.
    Image(InputFile),
    /// Load a depth map for the current image; only its red channel is used.
    Depth(InputFile),
    /// Remove the depth map.
    ClearDepth,
    /// Load a bokeh filter image.
    Filter(InputFile),
    /// Remove the bokeh filter image.
    ClearFilter,
    /// Render the current image with the given settings.
    Render {
        /// Render settings to apply.
        settings: Settings,
        /// Pixel rectangle to render, or the whole image when `None`.
        region: Option<IVector4>,
    },
}

/// Result of a [`Job`], ready for the UI to display.
pub enum Output {
    /// The user dismissed the browser file picker.
    #[cfg(target_arch = "wasm32")]
    Cancelled,
    /// The picked focus point and the depth value found there.
    Focus {
        /// Column of the picked pixel.
        x: usize,
        /// Row of the picked pixel.
        y: usize,
        /// Depth value at the picked pixel.
        value: f32,
    },
    /// A sample scene was loaded.
    Sample {
        /// The sample that was loaded.
        sample: &'static Sample,
        /// Preview of the loaded image.
        preview: ColorImage,
        /// Largest absolute depth value, used to scale the depth display; 10.0 without depth.
        maximum: f32,
    },
    /// An image file was loaded.
    Image {
        /// Display name of the file.
        name: String,
        /// Preview of the loaded image.
        preview: ColorImage,
    },
    /// A depth map was loaded; carries its display name.
    Depth(String),
    /// The depth map was removed.
    DepthCleared,
    /// A bokeh filter image was loaded; carries its display name.
    Filter(String),
    /// The bokeh filter image was removed.
    FilterCleared,
    /// A bokeh filter preview was rendered.
    FilterPreview {
        /// Preview of the filter image.
        preview: ColorImage,
        /// Time spent rendering the preview.
        elapsed: Duration,
        /// Name of the device that rendered it.
        device: String,
    },
    /// A render result, which may cover only part of the image.
    Render {
        /// Preview of the rendered region.
        preview: ColorImage,
        /// Region of the image covered by `preview`.
        region: IVector4,
        /// Region the user asked for.
        requested: IVector4,
        /// Whether the full requested region was rendered.
        complete: bool,
        /// Time spent in the render call.
        elapsed: Duration,
        /// Time spent creating the renderer; zero when a cached renderer was reused.
        initialization: Duration,
        /// Name of the device that rendered it.
        device: String,
    },
}

/// Holds the loaded image, depth map, filter and cached renderer, and runs [`Job`]s against them.
#[derive(Default)]
pub struct Processor {
    /// Source image as height x width x 4 linear RGBA, or `None` before anything is loaded.
    image: Option<Array3<f32>>,
    /// Single-channel depth map with the same height and width as `image`.
    depth: Option<Array2<f32>>,
    /// Bokeh filter image used by the image filter mode.
    filter: Option<Array3<f32>>,
    /// Cached renderer, the key it was created for, and its device name.
    renderer: Option<(RendererKey, OpenDefocusRenderer, String)>,
}

/// Converts a float RGBA image into an 8-bit egui image for display.
fn preview(image: &Array3<f32>) -> Result<ColorImage> {
    let pixels = DynamicImage::from(Rgba32FImage::from_ndarray(image.clone())?).to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [pixels.width() as usize, pixels.height() as usize],
        pixels.as_raw(),
    ))
}

/// Reads an image file from disk and decodes it, returning its pixels and display name.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or decoded, or the pixels are invalid.
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    clippy::unused_async,
    reason = "same signature as the wasm loader, which awaits the file dialog"
)]
async fn load(path: &InputFile) -> Result<Option<(Array3<f32>, String)>> {
    let image = image::open(path).with_context(|| format!("Could not load {}", path.display()))?;
    Ok(Some((decode(&image)?, path.display().to_string())))
}

/// Opens the browser file picker and decodes the chosen file.
///
/// Returns `None` when the picker is dismissed.
///
/// # Errors
///
/// Returns an error if the chosen file cannot be decoded.
#[cfg(target_arch = "wasm32")]
async fn load(_: &InputFile) -> Result<Option<(Array3<f32>, String)>> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "tif", "tiff", "exr", "hdr"],
        )
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let name = file.file_name();
    let bytes = file.read().await;
    let image =
        image::load_from_memory(&bytes).with_context(|| format!("Could not load {name}"))?;
    Ok(Some((decode(&image)?, name)))
}

/// Converts a decoded image into height x width x 4 float RGBA values.
///
/// # Errors
///
/// Returns an error if the image is empty or any converted value is NaN or infinite.
fn decode(image: &DynamicImage) -> Result<Array3<f32>> {
    ensure!(image.width() > 0 && image.height() > 0, "Image is empty");
    let image = image.to_rgba32f().to_ndarray();
    ensure!(
        image.iter().all(|v| v.is_finite()),
        "Image contains NaN or infinity"
    );
    Ok(image)
}

/// Checks that `depth` matches the height and width of `image` and holds only finite values.
///
/// # Errors
///
/// Returns an error on a dimension mismatch or a NaN or infinite depth value.
fn validate_depth(image: &Array3<f32>, depth: &Array2<f32>) -> Result<()> {
    ensure!(
        depth.dim() == (image.dim().0, image.dim().1),
        "Depth must have the same dimensions as the image"
    );
    ensure!(
        depth.iter().all(|v| v.is_finite()),
        "Depth contains NaN or infinity"
    );
    Ok(())
}

impl Processor {
    /// Runs `job` without an update callback; used by the tests.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Processor::process_with_updates`].
    #[cfg(test)]
    pub async fn process(&mut self, job: Job) -> Result<Output> {
        self.process_with_updates(job, |_| Ok(())).await
    }

    /// Runs `job` to completion and returns its output.
    ///
    /// The `_update` callback is accepted for progress reporting, but no intermediate outputs
    /// are sent to it yet.
    ///
    /// # Errors
    ///
    /// Returns an error if a required image, depth map or filter has not been loaded, a file
    /// cannot be read or decoded, the depth or render region is invalid, or rendering fails.
    pub async fn process_with_updates(
        &mut self,
        job: Job,
        _update: impl FnMut(Output) -> Result<()>,
    ) -> Result<Output> {
        match job {
            Job::PickFocus { x, y } => {
                let depth = self.depth.as_ref().context("Load a depth image first")?;
                let value = *depth
                    .get((y, x))
                    .context("Focus point is outside the depth image")?;
                ensure!(
                    value.is_finite() && value >= 0.0,
                    "Pick a pixel with nonnegative, finite depth"
                );
                // Both Real and 1/Z focal-plane controls use the input depth's units.
                Ok(Output::Focus { x, y, value })
            }
            Job::Sample(sample) => self.load_sample(sample),
            Job::Image(path) => {
                let loaded = load(&path).await?;
                #[cfg(target_arch = "wasm32")]
                if loaded.is_none() {
                    return Ok(Output::Cancelled);
                }
                let (image, name) = loaded.context("File selection cancelled")?;
                let preview = preview(&image)?;
                self.image = Some(image);
                self.depth = None;
                Ok(Output::Image { name, preview })
            }
            Job::Depth(path) => {
                let image = self.image.as_ref().context("Load an image first")?;
                // Use the red channel verbatim; luminance conversion changes depth values.
                let loaded = load(&path).await?;
                #[cfg(target_arch = "wasm32")]
                if loaded.is_none() {
                    return Ok(Output::Cancelled);
                }
                let (depth, name) = loaded.context("File selection cancelled")?;
                let depth = depth.slice(s![.., .., 0]).to_owned();
                validate_depth(image, &depth)?;
                self.depth = Some(depth);
                Ok(Output::Depth(name))
            }
            Job::ClearDepth => {
                self.depth = None;
                Ok(Output::DepthCleared)
            }
            Job::Filter(path) => {
                let loaded = load(&path).await?;
                #[cfg(target_arch = "wasm32")]
                if loaded.is_none() {
                    return Ok(Output::Cancelled);
                }
                let (filter, name) = loaded.context("File selection cancelled")?;
                self.filter = Some(filter);
                Ok(Output::Filter(name))
            }
            Job::ClearFilter => {
                self.filter = None;
                Ok(Output::FilterCleared)
            }
            Job::Render { settings, region } => self.render_job(settings, region).await,
        }
    }

    /// Loads a bundled sample scene as the current image and depth map.
    ///
    /// # Errors
    ///
    /// Returns an error if the sample cannot be decoded or its depth does not match the image.
    fn load_sample(&mut self, sample: &'static Sample) -> Result<Output> {
        let image = decode(&image::load_from_memory(sample.image)?)?;
        let depth = sample
            .depth
            .map(|bytes| -> Result<Array2<f32>> {
                let depth = decode(&image::load_from_memory(bytes)?)?
                    .slice(s![.., .., 0])
                    .to_owned();
                Ok(if sample.invert_direct_coc {
                    depth.mapv(|value| -value)
                } else {
                    depth
                })
            })
            .transpose()?;
        if let Some(depth) = &depth {
            validate_depth(&image, depth)?;
        }
        let maximum = depth.as_ref().map_or(10.0, |depth| {
            depth
                .iter()
                .fold(1.0_f32, |max, value| max.max(value.abs()))
        });
        let preview = preview(&image)?;
        self.image = Some(image);
        self.depth = depth;
        self.filter = None;
        Ok(Output::Sample {
            sample,
            preview,
            maximum,
        })
    }

    /// Runs a render job: creates the renderer if needed, then renders the filter preview or the image region.
    ///
    /// # Errors
    ///
    /// Returns an error if the renderer cannot be created or the render fails.
    async fn render_job(
        &mut self,
        mut settings: Settings,
        region: Option<IVector4>,
    ) -> Result<Output> {
        let initialization = self.ensure_renderer(&mut settings).await?;
        if settings.render.filter.preview
            && settings.render.filter.mode()
                == opendefocus::datamodel::render::FilterMode::BokehCreator
        {
            return self.render_filter_preview(settings).await;
        }
        self.render_image(settings, region, initialization).await
    }

    /// Creates the renderer when none is cached and returns the time that took.
    ///
    /// # Errors
    ///
    /// Returns an error if the renderer cannot be created.
    async fn ensure_renderer(&mut self, settings: &mut Settings) -> Result<Duration> {
        let key = RendererKey;
        if self
            .renderer
            .as_ref()
            .is_some_and(|(cached, _, _)| *cached == key)
        {
            return Ok(Duration::ZERO);
        }
        let start = Instant::now();
        let renderer = OpenDefocusRenderer::new(settings)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let device = settings.render.device_name.clone().unwrap_or_default();
        self.renderer = Some((key, renderer, device));
        Ok(start.elapsed())
    }

    /// Renders the bokeh filter preview image with the cached renderer.
    ///
    /// # Errors
    ///
    /// Returns an error if the renderer is missing, the filter format is empty or rendering fails.
    async fn render_filter_preview(&self, mut settings: Settings) -> Result<Output> {
        let Some((_, renderer, device)) = self.renderer.as_ref() else {
            anyhow::bail!("Renderer is not initialized");
        };
        ensure!(
            settings.render.filter.resolution > 0
                && settings.bokeh.aspect_ratio.is_finite()
                && settings.bokeh.aspect_ratio >= 0.0,
            "Filter resolution must be positive and aspect ratio nonnegative and finite"
        );
        let bounds = settings
            .render
            .filter
            .calculate_filter_box(settings.bokeh.aspect_ratio);
        let width = (bounds[2] - bounds[0]) as usize;
        let height = (bounds[3] - bounds[1]) as usize;
        ensure!(
            width > 0 && height > 0,
            "Filter preview has an empty format"
        );
        settings.render.resolution = UVector2 {
            x: width as u32,
            y: height as u32,
        };
        let mut image = Array3::<f32>::zeros((height, width, 4));
        let start = Instant::now();
        renderer
            .render(settings, image.view_mut(), None, None)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let elapsed = start.elapsed();
        Ok(Output::FilterPreview {
            preview: preview(&image)?,
            elapsed,
            device: device.clone(),
        })
    }

    /// Renders `region` of the loaded image with the cached renderer and returns its preview.
    ///
    /// # Errors
    ///
    /// Returns an error if the image, depth or filter is missing, the region is outside the image or rendering fails.
    async fn render_image(
        &self,
        mut settings: Settings,
        region: Option<IVector4>,
        initialization: Duration,
    ) -> Result<Output> {
        let Some((_, renderer, device)) = self.renderer.as_ref() else {
            anyhow::bail!("Renderer is not initialized");
        };
        let source = self.image.as_ref().context("Load an image first")?;
        let depth = if settings.defocus.defocus_mode() == DefocusMode::Depth {
            let depth = self.depth.as_ref().context("Load a depth image first")?;
            validate_depth(source, depth)?;
            Some(depth)
        } else {
            None
        };
        let width = source.dim().1 as i32;
        let height = source.dim().0 as i32;
        let entire_image = IVector4 {
            x: 0,
            y: 0,
            z: width,
            w: height,
        };
        // Filter preview paints the entire image, independently of scene coordinates.
        let region = if settings.render.filter.preview {
            entire_image
        } else {
            region.unwrap_or(entire_image)
        };
        ensure!(
            region.x >= 0
                && region.y >= 0
                && region.z <= width
                && region.w <= height
                && region.x < region.z
                && region.y < region.w,
            "Render region is outside the image"
        );
        settings.render.resolution = UVector2 {
            x: width as u32,
            y: height as u32,
        };
        let filter =
            if settings.render.filter.mode() == opendefocus::datamodel::render::FilterMode::Image {
                Some(self.filter.as_ref().context("Load a filter image first")?)
            } else {
                None
            };
        let padding = renderer.padding(&settings).min(width.max(height) as u32) as i32;
        let full_region = IVector4 {
            x: region.x.saturating_sub(padding).max(0),
            y: region.y.saturating_sub(padding).max(0),
            z: region.z.saturating_add(padding).min(width),
            w: region.w.saturating_add(padding).min(height),
        };
        // The renderer blocks oversized frames internally. Keep one request per UI
        // render so preparation and its integral hierarchies are built only once.
        let mut image = source
            .slice(s![
                full_region.y..full_region.w,
                full_region.x..full_region.z,
                ..
            ])
            .to_owned();
        let depth = depth.map(|depth| {
            depth
                .slice(s![
                    full_region.y..full_region.w,
                    full_region.x..full_region.z
                ])
                .to_owned()
        });
        let start = Instant::now();
        renderer
            .render_stripe(
                RenderSpecs {
                    full_region,
                    render_region: region,
                },
                settings,
                image.view_mut(),
                depth,
                filter.cloned(),
            )
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let elapsed = start.elapsed();
        Ok(Output::Render {
            preview: preview(
                &image
                    .slice(s![
                        region.y - full_region.y..region.w - full_region.y,
                        region.x - full_region.x..region.z - full_region.x,
                        ..
                    ])
                    .to_owned(),
            )?,
            region,
            requested: region,
            complete: true,
            elapsed,
            initialization,
            device: device.clone(),
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
