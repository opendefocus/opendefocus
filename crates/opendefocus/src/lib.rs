#![warn(unused_extern_crates)]

pub mod abort;
mod error;
mod renders;
mod runners;
mod traits;
mod worker;

/// Exported datastructure containing all settings for configuring the convolution
pub mod datamodel {
    pub use bokeh_creator;
    pub use circle_of_confusion;
    pub use opendefocus_datastructure::*;
}

use crate::{error::Error, runners::ConvolveRunner, traits::TraitBounds};
use error::Result;
use ndarray::{Array2, Array3, ArrayViewMut3};
use opendefocus_datastructure::render::FilterMode;
use worker::engine::RenderEngine;

use crate::runners::shared_runner::SharedRunner;

#[derive(Debug, Clone)]
/// OpenDefocus rendering instance that stores the device configuration
pub struct OpenDefocusRenderer {
    /// Runner that is able to interpret the kernel
    runner: SharedRunner,
    gpu: bool,
}

impl OpenDefocusRenderer {
    /// Create a new `OpenDefocus` instance.
    pub async fn new(prefer_gpu: bool, settings: &mut datamodel::Settings) -> Result<Self> {
        let runner = SharedRunner::init(prefer_gpu).await;
        let mut gpu = false;
        #[cfg(feature = "wgpu")]
        if let SharedRunner::Cpu(_) = runner {
            log::warn!(
                "Using CPU software rendering for OpenDefocus. This is significantly slower compared to GPU rendering."
            )
        } else {
            gpu = true;
        };

        let backend_info = runner.backend();
        let (device_name, backend) = (backend_info.device_name, backend_info.backend);
        let device_name = format!("{device_name} - {backend}").to_string();

        settings.render.device_name = Some(device_name);
        Ok(Self { runner, gpu })
    }

    pub fn is_gpu(&self) -> bool {
        self.gpu
    }

    /// Performs the main rendering operation according to the settings.
    pub async fn render<'image, T: TraitBounds>(
        &self,
        render_specs: datamodel::render::RenderSpecs,
        settings: datamodel::Settings,
        image: &mut ArrayViewMut3<'image, T>,
        depth: Array2<T>,
        filter: Option<Array3<T>>,
    ) -> Result<()> {
        self.validate(&settings, &filter)?;
        let engine = RenderEngine::new(settings, render_specs);
        engine.render(&self.runner, image, depth, filter).await?;
        Ok(())
    }

    fn validate<'image, T: TraitBounds>(
        &self,
        settings: &datamodel::Settings,
        filter: &Option<Array3<T>>,
    ) -> Result<()> {
        if settings.render.filter.mode() == FilterMode::Image && filter.is_none() {
            return Err(Error::NoFilterProvided);
        }

        if settings.render.result_mode() == datamodel::render::ResultMode::FocalPlaneSetup
            && settings.defocus.defocus_mode() == datamodel::defocus::DefocusMode::Twod
        {
            return Err(Error::FocalPlaneOverlayWhile2D);
        }

        Ok(())
    }
}
