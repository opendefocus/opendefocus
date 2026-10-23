//! Public render entry points for `OpenDefocusRenderer`: adapter setup, the adaptive stage calls, source padding and the
//! full, stripe and ROI render functions. Settings are validated before any GPU work starts.
use crate::datamodel;
use crate::datamodel::{
    IVector4, UVector2,
    defocus::DefocusMode,
    render::{FilterMode, RenderSpecs},
};
use crate::error::Error;
use crate::error::Result;
#[cfg(feature = "wgpu")]
use crate::runners::TileFeatures;
#[cfg(feature = "wgpu")]
use crate::runners::WgpuRunner;
#[cfg(feature = "wgpu")]
use crate::runners::adaptive::{AdaptiveSourcePyramid, CocPolarity, PreparedLayers};
#[cfg(feature = "wgpu")]
use crate::worker;
#[cfg(feature = "wgpu")]
use crate::{AdaptiveSampleBudget, adaptive_filter_resolution, validate_adaptive_settings};
use crate::{OpenDefocusRenderer, adaptive_source_padding, is_bokeh_filter_preview};
use ndarray::{Array2, Array3, ArrayViewMut3};

impl OpenDefocusRenderer {
    /// Create a renderer using the adaptive WGPU pipeline.
    ///
    /// Device initialization errors are returned directly; rendering always uses WGPU.
    ///
    /// # Errors
    /// Returns an error when the hardware WGPU adapter or device cannot be initialized.
    /// Without the `wgpu` feature, returns [`Error::AdaptiveWgpuRequired`].
    pub async fn new(settings: &mut datamodel::Settings) -> Result<Self> {
        #[cfg(feature = "wgpu")]
        {
            let runner = WgpuRunner::new_adaptive().await?;
            settings.render.device_name = Some(runner.device_description());
            Ok(Self { runner })
        }
        #[cfg(not(feature = "wgpu"))]
        {
            let _ = settings;
            Err(Error::AdaptiveWgpuRequired)
        }
    }

    /// Run the opt-in adaptive tile analysis on this renderer's hardware GPU.
    ///
    /// Inputs are linear premultiplied RGBA32F and signed `CoC` in full-resolution pixels.
    /// `origin` preserves tile alignment for offset frame regions. This diagnostic prepass
    /// does not change the output of `render` or `render_stripe`.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU preparation, execution, or readback fails.
    #[cfg(feature = "wgpu")]
    pub async fn classify_adaptive_tiles(
        &self,
        rgba: &Array3<f32>,
        coc: &Array2<f32>,
        origin: [i32; 2],
        max_radius_px: f32,
    ) -> Result<Vec<TileFeatures>> {
        self.runner
            .classify_tiles(rgba, coc, origin, max_radius_px)
            .await
    }

    /// Prepare near/far layers for the adaptive GPU gather.
    ///
    /// `polarity` explicitly states which `CoC` sign denotes foreground. The
    /// bounded far-layer extension never invents color when no valid far seed
    /// lies within `max_radius_px`; such output pixels remain invalid.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU preparation, execution, or readback fails.
    #[cfg(feature = "wgpu")]
    pub async fn prepare_adaptive_layers(
        &self,
        rgba: &Array3<f32>,
        coc: &Array2<f32>,
        max_radius_px: f32,
        polarity: CocPolarity,
    ) -> Result<PreparedLayers> {
        self.runner
            .prepare_adaptive_layers(rgba, coc, max_radius_px, polarity)
            .await
    }

    /// Build a diagnostic GPU source pyramid while keeping near/far color and
    /// metadata in separate reductions. This currently returns CPU-visible
    /// diagnostic levels and is not yet the adaptive render path.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU preparation, execution, or readback fails.
    #[cfg(feature = "wgpu")]
    pub async fn build_adaptive_pyramid(
        &self,
        prepared: &PreparedLayers,
    ) -> Result<AdaptiveSourcePyramid> {
        self.runner.build_adaptive_pyramid(prepared).await
    }

    /// Apply bounded circular gathers to a full-frame near/far preparation.
    /// Smooth tiles use 64 positions and detail tiles use 256. This entry point
    /// always uses the fixed Fast sample budget.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU preparation, execution, or readback fails.
    #[cfg(feature = "wgpu")]
    pub async fn gather_adaptive_frame(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
    ) -> Result<Array3<f32>> {
        self.runner
            .gather_adaptive_frame(source, prepared, features, max_radius_px)
            .await
    }

    /// Apply adaptive gathers using sample counts selected for renderer quality.
    ///
    /// # Errors
    /// Returns an error when the inputs are invalid or GPU preparation, execution, or readback fails.
    #[cfg(feature = "wgpu")]
    pub async fn gather_adaptive_frame_with_budget(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        max_radius_px: f32,
        sample_budget: AdaptiveSampleBudget,
    ) -> Result<Array3<f32>> {
        self.runner
            .gather_adaptive_frame_with_budget(
                source,
                prepared,
                features,
                max_radius_px,
                sample_budget,
            )
            .await
    }

    /// Gather a full frame with the adaptive budget for the selected quality,
    /// applying the supported per-sample spatial lens settings from the legacy renderer.
    ///
    /// # Errors
    /// Returns [`Error::AdaptiveSettingsUnsupported`] when the settings are outside the supported adaptive range.
    /// Returns an error when the GPU runner rejects the inputs or fails to create, run, or read back GPU resources.
    #[cfg(feature = "wgpu")]
    pub async fn gather_adaptive_frame_with_settings(
        &self,
        source: &Array3<f32>,
        prepared: &PreparedLayers,
        features: &[TileFeatures],
        settings: &datamodel::Settings,
    ) -> Result<Array3<f32>> {
        validate_adaptive_settings(settings)?;
        let sample_budget = AdaptiveSampleBudget::for_quality(settings.render.get_quality());
        let (height, width, _) = source.dim();
        let mut settings = settings.clone();
        settings.render.resolution = datamodel::UVector2 {
            x: width as u32,
            y: height as u32,
        };
        let region = datamodel::IVector4 {
            x: 0,
            y: 0,
            z: width as i32,
            w: height as i32,
        };
        let specs = datamodel::render::RenderSpecs {
            full_region: region,
            render_region: region,
        };
        let lens = crate::datamodel::settings_to_convolve_settings(
            &settings,
            &specs,
            adaptive_filter_resolution(&settings),
            (width * height) as u32,
        );
        let active = crate::convolve_settings::GlobalFlags::from_bits_retain(lens.flags)
            .intersects(
                crate::convolve_settings::GlobalFlags::USE_NON_UNIFORM
                    | crate::convolve_settings::GlobalFlags::AXIAL_ABERRATION_ENABLE
                    | crate::convolve_settings::GlobalFlags::INVERSE_FOREGROUND_BOKEH_SHAPE,
            );
        self.runner
            .gather_adaptive_frame_with_lens(
                source,
                prepared,
                features,
                settings.defocus.get_max_size(),
                sample_budget,
                active.then_some(&lens),
            )
            .await
    }

    /// Source context needed by preparation followed by Adaptive gathering.
    /// Keep aperture image resolution separate from this buffer padding.
    pub fn padding(&self, settings: &datamodel::Settings) -> u32 {
        if is_bokeh_filter_preview(settings)
            || settings.render.result_mode() == datamodel::render::ResultMode::FocalPlaneSetup
        {
            return 0;
        }
        adaptive_source_padding(settings)
    }

    /// Automatic stripe height for interactive hosts and viewers.
    pub fn stripe_height(&self, settings: &datamodel::Settings) -> u32 {
        if settings.render.result_mode() == datamodel::render::ResultMode::FocalPlaneSetup {
            return 32;
        }
        match settings.render.get_quality() {
            datamodel::render::Quality::Fast => 256,
            datamodel::render::Quality::Accurate => 64,
        }
    }

    /// Render the provided image using the stripes functionality.
    ///
    /// Stripes are a subset of an entire image. Interactive hosts use this to keep the viewer responsive.
    /// Basically it calls this function a few times for the portion of the image.
    ///
    /// Using the provided renderspecs `OpenDefocus` knows which portion is being rendered
    /// and can calculate the actual screenspace position that way.
    ///
    /// Unless actually necessary, the [render](#method.render) function is a better choice.
    ///
    /// # Examples
    /// ## Skip padding region
    /// ```no_run
    /// use ndarray::Array3;
    /// use opendefocus::{
    ///    OpenDefocusRenderer,
    ///     datamodel::render::RenderSpecs,
    ///     datamodel::{IVector4, Settings, UVector2},
    /// };
    ///
    /// let mut image: Array3<f32> = Array3::zeros((256, 256, 4));
    /// let mut settings = Settings::default();
    /// settings.render.resolution = UVector2 { x: 256, y: 256 }; // resolution of full image
    /// let full_region = IVector4 {
    ///     x: 0,
    ///     y: 0,
    ///     z: 256,
    ///     w: 256,
    /// }; // full stripe size
    /// let render_region = IVector4 {
    ///     x: 10,
    ///     y: 10,
    ///     z: 246,
    ///     w: 246,
    /// }; // region we want to render (padding of 6)
    /// let render_specs = RenderSpecs {
    ///     full_region,
    ///     render_region,
    /// };
    /// # tokio_test::block_on(async {
    /// let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    /// renderer
    ///     .render_stripe(
    ///         render_specs,
    ///         settings,
    ///         image.view_mut(),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    ///
    /// ## Render sub portion of array
    /// ```no_run
    /// use ndarray::{Array3, s};
    /// use opendefocus::{
    ///    OpenDefocusRenderer,
    ///     datamodel::render::RenderSpecs,
    ///     datamodel::{IVector4, Settings, UVector2},
    /// };
    ///
    /// let mut image: Array3<f32> = Array3::zeros((256, 256, 4));
    /// let mut settings = Settings::default();
    /// settings.render.resolution = UVector2 { x: 256, y: 256 }; // resolution of full image
    /// let full_region = IVector4 {
    ///     x: 50,
    ///     y: 50,
    ///     z: 250,
    ///     w: 250,
    /// }; // full stripe size
    /// let render_region = IVector4 {
    ///     x: 56,
    ///     y: 56,
    ///     z: 200,
    ///     w: 200,
    /// }; // region we want to render (padding of 6)
    /// let render_specs = RenderSpecs {
    ///     full_region,
    ///     render_region,
    /// };
    /// # tokio_test::block_on(async {
    /// let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    /// renderer
    ///     .render_stripe(
    ///         render_specs,
    ///         settings,
    ///         image.slice_mut(s!(50..250, 50..250, ..)).view_mut(),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    ///
    /// # Errors
    /// Returns an error when the settings or inputs are missing or inconsistent, or rendering fails.
    /// Without the `wgpu` feature, returns [`Error::AdaptiveWgpuRequired`].
    pub async fn render_stripe(
        &self,
        render_specs: datamodel::render::RenderSpecs,
        settings: datamodel::Settings,
        image: ArrayViewMut3<'_, f32>,
        depth: Option<Array2<f32>>,
        filter: Option<Array3<f32>>,
    ) -> Result<()> {
        Self::validate(&settings, depth.as_ref(), filter.as_ref())?;
        #[cfg(feature = "wgpu")]
        {
            worker::adaptive_render::render(
                &self.runner,
                &render_specs,
                &settings,
                image,
                depth,
                filter,
            )
            .await
        }
        #[cfg(not(feature = "wgpu"))]
        {
            let _ = (render_specs, settings, image, depth, filter);
            Err(Error::AdaptiveWgpuRequired)
        }
    }

    /// Render the provided image.
    ///
    /// # Examples
    /// ## Render provided image
    /// ```no_run
    /// use ndarray::{Array3};
    /// use opendefocus::{
    ///    OpenDefocusRenderer,
    ///     datamodel::render::RenderSpecs,
    ///     datamodel::{IVector4, Settings, UVector2},
    /// };
    ///
    /// let mut image: Array3<f32> = Array3::zeros((256, 256, 4)); // obviously load the actual image, this is just empty data
    /// let mut settings = Settings::default();
    ///
    /// # tokio_test::block_on(async {
    /// let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    /// renderer
    ///     .render(
    ///         settings,
    ///         image.view_mut(),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    ///
    /// # Errors
    /// Returns an error when the settings or inputs are missing or inconsistent, or rendering fails.
    /// Without the `wgpu` feature, returns [`Error::AdaptiveWgpuRequired`].
    pub async fn render(
        &self,
        mut settings: datamodel::Settings,
        image: ArrayViewMut3<'_, f32>,
        depth: Option<Array2<f32>>,
        filter: Option<Array3<f32>>,
    ) -> Result<()> {
        Self::validate(&settings, depth.as_ref(), filter.as_ref())?;
        let region = IVector4 {
            x: 0,
            y: 0,
            z: image.dim().1 as i32,
            w: image.dim().0 as i32,
        };
        let render_specs = RenderSpecs {
            full_region: region,
            render_region: region,
        };
        settings.render.resolution = UVector2 {
            x: image.dim().1 as u32,
            y: image.dim().0 as u32,
        };
        #[cfg(feature = "wgpu")]
        {
            worker::adaptive_render::render(
                &self.runner,
                &render_specs,
                &settings,
                image,
                depth,
                filter,
            )
            .await
        }
        #[cfg(not(feature = "wgpu"))]
        {
            let _ = (render_specs, settings, image, depth, filter);
            Err(Error::AdaptiveWgpuRequired)
        }
    }

    /// Internal validator which performs some checks ahead of actual rendering.
    ///
    /// # Errors
    /// Returns the first settings validation failure, such as an unsupported or invalid combination.
    fn validate(
        settings: &datamodel::Settings,
        depth: Option<&Array2<f32>>,
        filter: Option<&Array3<f32>>,
    ) -> Result<()> {
        // Bokeh previews return before consuming depth, filter input, or focal-plane mode.
        if is_bokeh_filter_preview(settings) {
            return Ok(());
        }
        if settings.render.filter.mode() == FilterMode::Image && filter.is_none() {
            return Err(Error::NoFilterProvided);
        }

        if settings.defocus.defocus_mode() != DefocusMode::Twod && depth.is_none() {
            return Err(Error::DepthNotFound);
        }

        if settings.render.result_mode() == datamodel::render::ResultMode::FocalPlaneSetup
            && settings.defocus.defocus_mode() == datamodel::defocus::DefocusMode::Twod
        {
            return Err(Error::FocalPlaneOverlayWhile2D);
        }

        Ok(())
    }
}

#[cfg(test)]
/// Tests for the input checks that run before adaptive rendering.
mod tests {
    use super::*;
    use crate::datamodel::{defocus::DefocusMode, render::ResultMode};
    use ndarray::{Array2, Array3};

    #[test]
    /// Verifies a bokeh preview skips the depth, filter and focal-plane checks entirely.
    fn validate_skips_inputs_for_bokeh_previews() {
        let mut settings = datamodel::Settings::default();
        settings.render.filter.preview = true;
        settings.render.filter.mode = FilterMode::BokehCreator as i32;
        settings.render.result_mode = ResultMode::FocalPlaneSetup as i32;
        assert!(OpenDefocusRenderer::validate(&settings, None, None).is_ok());
    }

    #[test]
    /// Verifies the image filter mode requires a filter image and that this check wins over a missing depth.
    fn validate_requires_filter_image_for_image_mode() {
        let mut settings = datamodel::Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.render.filter.mode = FilterMode::Image as i32;
        assert!(matches!(
            OpenDefocusRenderer::validate(&settings, None, None),
            Err(Error::NoFilterProvided)
        ));
        let filter = Array3::<f32>::zeros((1, 1, 4));
        let depth = Array2::<f32>::zeros((1, 1));
        assert!(OpenDefocusRenderer::validate(&settings, Some(&depth), Some(&filter)).is_ok());
    }

    #[test]
    /// Verifies depth mode requires a depth image while 2D mode does not.
    fn validate_requires_depth_only_in_depth_mode() {
        let mut settings = datamodel::Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        assert!(matches!(
            OpenDefocusRenderer::validate(&settings, None, None),
            Err(Error::DepthNotFound)
        ));
        let depth = Array2::<f32>::zeros((1, 1));
        assert!(OpenDefocusRenderer::validate(&settings, Some(&depth), None).is_ok());

        settings.defocus.set_defocus_mode(DefocusMode::Twod);
        assert!(OpenDefocusRenderer::validate(&settings, None, None).is_ok());
    }

    #[test]
    /// Verifies the focal-plane overlay is refused in 2D and allowed with depth.
    fn validate_refuses_focal_plane_overlay_in_2d() {
        let mut settings = datamodel::Settings::default();
        settings.render.result_mode = ResultMode::FocalPlaneSetup as i32;
        assert!(matches!(
            OpenDefocusRenderer::validate(&settings, None, None),
            Err(Error::FocalPlaneOverlayWhile2D)
        ));
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        let depth = Array2::<f32>::zeros((1, 1));
        assert!(OpenDefocusRenderer::validate(&settings, Some(&depth), None).is_ok());
    }

    #[tokio::test]
    #[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
    /// Verifies on hardware that the pyramid wrapper halves to 1x1 with level zero equal to the prepared input,
    /// and that the fixed-Fast gather wrapper matches the explicit Fast budget exactly and keeps a constant HDR frame constant.
    async fn renderer_pyramid_and_fast_gather_wrappers_match_their_inputs() {
        use crate::AdaptiveSampleBudget;
        use crate::datamodel::render::Quality;
        use crate::runners::TileFeatures;
        use crate::runners::adaptive::CocPolarity;

        let mut settings = datamodel::Settings::default();
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        let (w, h) = (8usize, 8usize);
        let color = [2.5_f32, 0.5, 0.25, 1.0];
        let source = Array3::from_shape_fn((h, w, 4), |(_, _, c)| color[c]);
        let coc = Array2::from_elem((h, w), 6.0_f32);
        let prepared = renderer
            .prepare_adaptive_layers(&source, &coc, 12.0, CocPolarity::PositiveForeground)
            .await
            .unwrap();

        let pyramid = renderer.build_adaptive_pyramid(&prepared).await.unwrap();
        let sizes: Vec<_> = pyramid
            .levels
            .iter()
            .map(|level| (level.width, level.height))
            .collect();
        assert_eq!(sizes, vec![(8, 8), (4, 4), (2, 2), (1, 1)]);
        assert!(
            pyramid.levels[0]
                .pixels
                .iter()
                .zip(&prepared.pixels)
                .all(|(level, input)| {
                    level.near_rgba.map(f32::to_bits) == input.near_rgba.map(f32::to_bits)
                        && level.far_rgba.map(f32::to_bits) == input.far_rgba.map(f32::to_bits)
                })
        );

        let mut feature = TileFeatures { values: [0.0; 11] };
        feature.values[0] = -6.0;
        feature.values[1] = 6.0;
        feature.values[9] = 2.0;
        let features = vec![feature; w.div_ceil(16) * h.div_ceil(16)];
        let wrapper = renderer
            .gather_adaptive_frame(&source, &prepared, &features, 12.0)
            .await
            .unwrap();
        let explicit = renderer
            .gather_adaptive_frame_with_budget(
                &source,
                &prepared,
                &features,
                12.0,
                AdaptiveSampleBudget::for_quality(Quality::Fast),
            )
            .await
            .unwrap();
        assert_eq!(wrapper, explicit);
        for value in wrapper.indexed_iter() {
            let (index, value) = value;
            let channel = index.2;
            assert!((value - color[channel]).abs() < 1e-3, "{index:?}: {value}");
        }
    }
}
