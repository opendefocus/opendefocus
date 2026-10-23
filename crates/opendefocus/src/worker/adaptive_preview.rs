//! Preview rendering for the adaptive pipeline: bokeh-filter and focal plane previews, with a CPU reference for tests.
use crate::datamodel::{Settings, defocus::DefocusMode, render::ResultMode};
use crate::{
    error::{Error, Result},
    is_bokeh_filter_preview,
    runners::WgpuRunner,
};
use ndarray::{ArrayView2, ArrayViewMut3};

/// Render non-convolution outputs shared by the automatic adaptive path.
///
/// `depth` must contain the prepared `CoC` values used by the legacy focal-plane
/// preview. Bokeh previews do not consume depth.
///
/// # Errors
/// Returns an error when the preview settings are invalid or the GPU pass fails.
pub(crate) async fn render_adaptive_preview(
    runner: &WgpuRunner,
    settings: &Settings,
    image: ArrayViewMut3<'_, f32>,
    depth: Option<ArrayView2<'_, f32>>,
) -> Result<bool> {
    if is_bokeh_filter_preview(settings) {
        if !settings.bokeh.aspect_ratio.is_finite()
            || settings.bokeh.aspect_ratio < 0.0
            || settings.render.filter.resolution == 0
        {
            return Err(Error::InvalidParameter);
        }
        runner.render_preview(settings, image, None).await?;
        return Ok(true);
    }

    if settings.render.result_mode() != ResultMode::FocalPlaneSetup {
        return Ok(false);
    }
    if settings.defocus.defocus_mode() == DefocusMode::Twod {
        return Err(Error::FocalPlaneOverlayWhile2D);
    }
    let depth = depth.ok_or(Error::DepthNotFound)?;
    runner.render_preview(settings, image, Some(depth)).await?;
    Ok(true)
}

#[cfg(test)]
/// Test reference that renders the preview with the existing CPU helpers, for comparison with the GPU preview.
fn render_adaptive_preview_reference(
    settings: &Settings,
    mut image: ArrayViewMut3<'_, f32>,
    depth: Option<ArrayView2<'_, f32>>,
) -> Result<bool> {
    if is_bokeh_filter_preview(settings) {
        if !settings.bokeh.aspect_ratio.is_finite()
            || settings.bokeh.aspect_ratio < 0.0
            || settings.render.filter.resolution == 0
        {
            return Err(Error::InvalidParameter);
        }
        bokeh_creator::Renderer::render_to_array_f32(settings.bokeh, &mut image);
        return Ok(true);
    }
    if settings.render.result_mode() != ResultMode::FocalPlaneSetup {
        return Ok(false);
    }
    if settings.defocus.defocus_mode() == DefocusMode::Twod {
        return Err(Error::FocalPlaneOverlayWhile2D);
    }
    let depth = depth.ok_or(Error::DepthNotFound)?;
    crate::renders::focal_plane_setup::render_focal_plane_overlay(
        image,
        &depth,
        !settings.defocus.show_image,
    );
    Ok(true)
}

#[cfg(test)]
/// Tests for the adaptive preview.
mod tests {
    use super::*;
    use crate::datamodel::render::FilterMode;
    use bokeh_creator::Renderer;
    use ndarray::{Array2, Array3};

    #[test]
    /// Verifies the bokeh preview matches the existing bokeh renderer.
    fn bokeh_preview_matches_the_existing_bokeh_renderer() -> Result<()> {
        let mut settings = Settings::default();
        settings.render.filter.preview = true;
        settings.render.filter.mode = FilterMode::BokehCreator.into();
        settings.render.filter.resolution = 24;
        settings.bokeh.aspect_ratio = 0.0;
        settings.bokeh.radius = 0.3;
        settings.bokeh.angle = 31.0;

        let mut expected = Array3::<f32>::zeros((24, 24, 4));
        Renderer::render_to_array_f32(settings.bokeh, &mut expected.view_mut());
        let mut actual = Array3::<f32>::zeros((24, 24, 4));
        assert!(render_adaptive_preview_reference(
            &settings,
            actual.view_mut(),
            None
        )?);
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    /// Verifies the bokeh preview rejects a non-finite aspect ratio.
    fn bokeh_preview_rejects_non_finite_aspect_ratio() {
        let mut settings = Settings::default();
        settings.render.filter.preview = true;
        settings.render.filter.mode = FilterMode::BokehCreator.into();
        settings.bokeh.aspect_ratio = f32::NAN;
        let mut image = Array3::<f32>::zeros((2, 2, 4));
        assert!(matches!(
            render_adaptive_preview_reference(&settings, image.view_mut(), None),
            Err(Error::InvalidParameter)
        ));
    }

    #[test]
    /// Verifies the focal plane preview matches the existing overlay helper.
    fn focal_plane_preview_matches_the_existing_overlay_helper() -> Result<()> {
        let mut settings = Settings::default();
        settings.render.set_result_mode(ResultMode::FocalPlaneSetup);
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.show_image = false;
        let depth = Array2::from_shape_fn((3, 4), |(y, x)| (x as f32) - (y as f32));
        let source = Array3::from_shape_fn((3, 4, 4), |(y, x, channel)| {
            (y * 12 + x * 4 + channel) as f32 / 20.0
        });
        let mut expected = source.clone();
        crate::renders::focal_plane_setup::render_focal_plane_overlay(
            expected.view_mut(),
            &depth.view(),
            true,
        );
        let mut actual = source;

        assert!(render_adaptive_preview_reference(
            &settings,
            actual.view_mut(),
            Some(depth.view())
        )?);
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    /// Verifies a standard render is left to the convolution path.
    fn standard_result_is_left_for_the_convolution_path() -> Result<()> {
        let settings = Settings::default();
        let mut image = Array3::<f32>::zeros((2, 2, 4));
        assert!(!render_adaptive_preview_reference(
            &settings,
            image.view_mut(),
            None
        )?);
        Ok(())
    }

    #[test]
    /// Verifies a stale preview flag does not override the image filter mode.
    fn stale_filter_preview_flag_does_not_override_image_filter_mode() -> Result<()> {
        let mut settings = Settings::default();
        settings.render.filter.preview = true;
        settings.render.filter.mode = FilterMode::Image.into();
        let mut image = Array3::<f32>::from_elem((2, 2, 4), 0.25);
        let original = image.clone();
        assert!(!render_adaptive_preview_reference(
            &settings,
            image.view_mut(),
            None
        )?);
        assert_eq!(image, original);
        Ok(())
    }

    #[test]
    #[ignore = "requires a WGPU adapter; run with the shared validation lock"]
    /// Hardware check that the analytic bokeh and focal previews match the CPU reference.
    fn wgpu_analytic_bokeh_and_focal_previews_match_the_cpu_reference() -> Result<()> {
        tokio::runtime::Runtime::new()?.block_on(async {
            let runner = WgpuRunner::new_adaptive().await?;

            let mut bokeh_settings = Settings::default();
            bokeh_settings.render.filter.preview = true;
            bokeh_settings.render.filter.mode = FilterMode::BokehCreator.into();
            bokeh_settings.render.filter.resolution = 24;
            bokeh_settings.bokeh.radius = 0.3;
            bokeh_settings.bokeh.angle = 31.0;
            // The analytic center-color check must not depend on feature-unified noise.
            bokeh_settings.bokeh.noise.intensity = 0.0;
            for filter_type in [bokeh_creator::FilterType::Disc, bokeh_creator::FilterType::Blade] {
                bokeh_settings.bokeh.filter_type = filter_type.into();
                let mut bokeh_expected = Array3::<f32>::zeros((24, 24, 4));
                Renderer::render_to_array_f32(
                    bokeh_settings.bokeh,
                    &mut bokeh_expected.view_mut(),
                );
                let mut bokeh_actual = Array3::<f32>::zeros((24, 24, 4));
                runner
                    .render_preview(&bokeh_settings, bokeh_actual.view_mut(), None)
                    .await?;
                assert!((bokeh_expected[[12, 12, 3]] - 0.4).abs() <= f32::EPSILON);
                assert!(
                    (bokeh_actual[[12, 12, 3]] - 0.4).abs() <= 2.0e-5,
                    "bokeh center alpha should equal inner color at the zero-angle center: gpu={}, cpu={}, filter={filter_type:?}, settings={:?}",
                    bokeh_actual[[12, 12, 3]],
                    bokeh_expected[[12, 12, 3]],
                    bokeh_settings.bokeh,
                );
                let (index, actual, expected, delta) = bokeh_actual
                    .iter()
                    .zip(&bokeh_expected)
                    .enumerate()
                    .map(|(index, (actual, expected))| {
                        (index, *actual, *expected, (actual - expected).abs())
                    })
                    .max_by(|left, right| left.3.total_cmp(&right.3))
                    .expect("bokeh preview is nonempty");
                assert!(
                    delta <= 2.0e-5,
                    "bokeh preview max delta {delta} at ({}, {}, channel {}) actual={actual}, expected={expected}; filter={filter_type:?}, radius={}, aspect={}, angle={}, curvature={}, blades={}, ring_size={}, outer_blur={}, inner_blur={}, ring_color={}, inner_color={}",
                    index / (24 * 4),
                    (index / 4) % 24,
                    index % 4,
                    bokeh_settings.bokeh.radius,
                    bokeh_settings.bokeh.aspect_ratio,
                    bokeh_settings.bokeh.angle,
                    bokeh_settings.bokeh.curvature,
                    bokeh_settings.bokeh.blades,
                    bokeh_settings.bokeh.ring_size,
                    bokeh_settings.bokeh.outer_blur,
                    bokeh_settings.bokeh.inner_blur,
                    bokeh_settings.bokeh.ring_color,
                    bokeh_settings.bokeh.inner_color,
                );
            }

            let mut focal_settings = Settings::default();
            focal_settings
                .render
                .set_result_mode(ResultMode::FocalPlaneSetup);
            focal_settings.defocus.set_defocus_mode(DefocusMode::Depth);
            focal_settings.defocus.show_image = false;
            let depth = Array2::from_shape_fn((3, 4), |(y, x)| x as f32 - y as f32);
            let source = Array3::from_shape_fn((3, 4, 4), |(y, x, channel)| {
                (y * 12 + x * 4 + channel) as f32 / 20.0
            });
            let mut expected = source.clone();
            crate::renders::focal_plane_setup::render_focal_plane_overlay(
                expected.view_mut(),
                &depth.view(),
                true,
            );
            let mut actual = source;
            runner
                .render_preview(&focal_settings, actual.view_mut(), Some(depth.view()))
                .await?;
            assert_eq!(actual, expected);
            Ok(())
        })
    }

    #[test]
    /// Verifies the reference preview refuses focal-plane setup in 2D, requires depth for depth focal planes, and rejects bokeh filters with no resolution or a negative aspect ratio.
    fn preview_reference_rejects_unsupported_focal_and_bokeh_inputs() {
        let mut focal = Settings::default();
        focal.render.set_result_mode(ResultMode::FocalPlaneSetup);
        let mut image = Array3::<f32>::zeros((2, 2, 4));
        assert!(matches!(
            render_adaptive_preview_reference(&focal, image.view_mut(), None),
            Err(Error::FocalPlaneOverlayWhile2D)
        ));
        focal.defocus.set_defocus_mode(DefocusMode::Depth);
        assert!(matches!(
            render_adaptive_preview_reference(&focal, image.view_mut(), None),
            Err(Error::DepthNotFound)
        ));

        let mut bokeh = Settings::default();
        bokeh.render.filter.preview = true;
        bokeh.render.filter.mode = FilterMode::BokehCreator.into();
        bokeh.render.filter.resolution = 0;
        assert!(matches!(
            render_adaptive_preview_reference(&bokeh, image.view_mut(), None),
            Err(Error::InvalidParameter)
        ));
        bokeh.render.filter.resolution = 8;
        bokeh.bokeh.aspect_ratio = -1.0;
        assert!(matches!(
            render_adaptive_preview_reference(&bokeh, image.view_mut(), None),
            Err(Error::InvalidParameter)
        ));
    }

    #[tokio::test]
    #[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
    /// Verifies on hardware that the live preview path rejects a NaN bokeh aspect ratio and focal-plane setup in 2D with their specific errors.
    async fn live_preview_rejects_invalid_bokeh_and_2d_focal_setup() {
        let runner = WgpuRunner::new_adaptive().await.unwrap();
        let mut bokeh = Settings::default();
        bokeh.render.filter.preview = true;
        bokeh.render.filter.mode = FilterMode::BokehCreator.into();
        bokeh.bokeh.aspect_ratio = f32::NAN;
        let mut image = Array3::<f32>::zeros((2, 2, 4));
        let result = render_adaptive_preview(&runner, &bokeh, image.view_mut(), None).await;
        assert!(matches!(result, Err(Error::InvalidParameter)));

        let mut focal = Settings::default();
        focal.render.set_result_mode(ResultMode::FocalPlaneSetup);
        let result = render_adaptive_preview(&runner, &focal, image.view_mut(), None).await;
        assert!(matches!(result, Err(Error::FocalPlaneOverlayWhile2D)));
    }
}
