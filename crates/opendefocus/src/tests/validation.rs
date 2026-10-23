//! Checks that each rejection branch of adaptive settings validation reports its exact reason.
use super::*;
use crate::datamodel::{
    Settings,
    circle_of_confusion::{CameraData, Filmback, Resolution, WorldUnit},
    defocus::DefocusMode,
    render::{FilterMode, Quality},
};
use crate::error::Error;

/// Returns depth-mode settings that validation accepts, with a unit bokeh aspect ratio.
fn depth_settings() -> Settings {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.bokeh.aspect_ratio = 1.0;
    settings
}

/// Returns the rejection reason of settings, panicking if they are accepted or fail with another error.
fn rejection(settings: &Settings) -> &'static str {
    match validate_adaptive_settings(settings) {
        Err(Error::AdaptiveSettingsUnsupported(reason)) => reason,
        other => panic!("expected an unsupported-settings rejection, got {other:?}"),
    }
}

/// Returns depth settings with a physical camera attached, which validation accepts.
fn camera_settings() -> Settings {
    let mut settings = depth_settings();
    settings.defocus.use_direct_math = false;
    settings.defocus.circle_of_confusion.focal_plane = 1.0;
    settings.defocus.circle_of_confusion.camera_data = Some(CameraData {
        focal_length: 50.0,
        f_stop: 2.0,
        filmback: Filmback {
            width: 24.0,
            height: 18.0,
        },
        near_field: 0.1,
        far_field: 10_000.0,
        world_unit: WorldUnit::M as i32,
        resolution: Resolution {
            width: 1920,
            height: 1080,
        },
    });
    settings
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a bokeh preview with a bad filter resolution or aspect ratio is rejected, and a valid one skips defocus checks.
fn bokeh_preview_rejects_bad_filter_values_and_accepts_valid_ones() {
    let mut settings = depth_settings();
    settings.render.filter.preview = true;
    settings.render.filter.mode = FilterMode::BokehCreator as i32;
    let message = "filter resolution must be positive and aspect ratio nonnegative and finite";
    settings.render.filter.resolution = 0;
    assert_eq!(rejection(&settings), message);
    settings.render.filter.resolution = 256;
    settings.bokeh.aspect_ratio = -1.0;
    assert_eq!(rejection(&settings), message);
    settings.bokeh.aspect_ratio = f32::NAN;
    assert_eq!(rejection(&settings), message);

    settings.bokeh.aspect_ratio = 0.5;
    settings.defocus.defocus_mode = 99;
    assert!(validate_adaptive_settings(&settings).is_ok());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies unknown defocus, filter and quality values report their specific reasons.
fn unknown_mode_and_quality_values_report_their_reason() {
    let mut settings = depth_settings();
    settings.defocus.defocus_mode = 5;
    assert_eq!(
        rejection(&settings),
        "only Depth and 2D defocus modes are supported"
    );

    let mut settings = depth_settings();
    settings.render.filter.mode = 2;
    assert_eq!(rejection(&settings), "unknown aperture/filter mode");

    let mut settings = depth_settings();
    settings.render.gui = true;
    settings.render.quality = 99;
    settings.render.farm_quality = Quality::Accurate as i32;
    assert_eq!(rejection(&settings), "unknown renderer quality");

    settings.render.quality = Quality::Fast as i32;
    settings.render.gui = false;
    settings.render.farm_quality = 99;
    assert_eq!(rejection(&settings), "unknown renderer quality");

    settings.render.quality = 99;
    settings.render.farm_quality = Quality::Fast as i32;
    assert!(validate_adaptive_settings(&settings).is_ok());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a pixel or aperture aspect ratio that is zero, negative or non-finite is rejected.
fn non_positive_or_non_finite_pixel_aspect_is_rejected() {
    let message = "pixel and aperture aspect ratios must be positive and finite";
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut settings = depth_settings();
        settings.defocus.circle_of_confusion.pixel_aspect = value;
        assert_eq!(rejection(&settings), message, "pixel aspect {value}");
    }
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies the simple filter phase check fires only when inverse foreground or axial aberration needs it.
fn simple_filter_phase_resolution_is_checked_only_when_needed() {
    let message = "simple-filter phase resolution must be positive";
    let mut settings = depth_settings();
    settings.bokeh.aspect_ratio = f32::NAN;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.render.filter.resolution = 0;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.bokeh.aspect_ratio = -0.5;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.non_uniform.inverse_foreground = false;
    settings.bokeh.aspect_ratio = f32::NAN;
    assert!(validate_adaptive_settings(&settings).is_ok());

    settings.non_uniform.axial_aberration.enable = true;
    settings.render.filter.resolution = 0;
    assert_eq!(rejection(&settings), message);
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a bokeh creator filter needs a positive resolution and a finite nonnegative aspect ratio.
fn bokeh_creator_filter_requires_positive_resolution_and_valid_aspect() {
    let message = "filter resolution must be positive";
    let mut settings = depth_settings();
    settings.render.filter.mode = FilterMode::BokehCreator as i32;
    settings.render.filter.resolution = 0;
    assert_eq!(rejection(&settings), message);

    settings.render.filter.resolution = 256;
    settings.bokeh.aspect_ratio = -1.0;
    assert_eq!(rejection(&settings), message);
    settings.bokeh.aspect_ratio = f32::INFINITY;
    assert_eq!(rejection(&settings), message);

    settings.bokeh.aspect_ratio = 2.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies non-finite or negative focus, protection, multiplier and proxy values are rejected.
fn focus_protect_multiplier_and_proxy_values_are_checked() {
    let message = "focus and effective radii must be finite and nonnegative";
    let mut settings = depth_settings();
    settings.defocus.circle_of_confusion.focal_plane = f32::NAN;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.defocus.circle_of_confusion.protect = -1.0;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.defocus.size_multiplier = -1.0;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.defocus.size_multiplier = f32::NAN;
    assert_eq!(rejection(&settings), message);

    for proxy in [0.0, -1.0, f32::NAN] {
        let mut settings = depth_settings();
        settings.defocus.proxy_scale = Some(proxy);
        assert_eq!(rejection(&settings), message, "proxy {proxy}");
    }
    let mut settings = depth_settings();
    settings.defocus.proxy_scale = Some(2.0);
    assert!(validate_adaptive_settings(&settings).is_ok());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies the effective radius limit is 16384 pixels inclusive in depth and 2D modes.
fn effective_radius_limit_is_inclusive_at_sixteen_thousand_pixels() {
    let message = "effective defocus radius (max size x multiplier) is too large (limit 16384 px)";
    let mut settings = depth_settings();
    settings.defocus.circle_of_confusion.max_size = 16384.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.defocus.circle_of_confusion.max_size = 20000.0;
    assert_eq!(rejection(&settings), message);

    let mut settings = depth_settings();
    settings.defocus.set_defocus_mode(DefocusMode::Twod);
    settings.defocus.circle_of_confusion.size = 16384.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.defocus.circle_of_confusion.size = 16385.0;
    assert_eq!(rejection(&settings), message);
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies the CoC size must be finite and nonnegative only when direct math is off.
fn coc_size_is_checked_only_without_direct_math() {
    let message = "CoC size must be finite and nonnegative";
    let mut settings = depth_settings();
    settings.defocus.circle_of_confusion.size = -1.0;
    assert_eq!(rejection(&settings), message);
    settings.defocus.circle_of_confusion.size = f32::NAN;
    assert_eq!(rejection(&settings), message);

    settings.defocus.use_direct_math = true;
    assert!(validate_adaptive_settings(&settings).is_ok());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies an unknown camera CoC depth math value is rejected unless direct math bypasses it.
fn unknown_coc_depth_math_is_rejected_without_direct_math() {
    let mut settings = depth_settings();
    settings.defocus.circle_of_confusion.math = 7;
    assert_eq!(rejection(&settings), "unknown camera CoC depth math");

    settings.defocus.use_direct_math = true;
    assert!(validate_adaptive_settings(&settings).is_ok());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a physical camera with valid optics is accepted and each invalid optic is rejected.
fn physical_camera_optics_must_be_positive_and_finite() {
    let mut settings = camera_settings();
    assert!(validate_adaptive_settings(&settings).is_ok());
    let camera_message = "physical camera dimensions and f-stop must be positive and finite";

    settings.defocus.circle_of_confusion.focal_plane = 0.0;
    assert_eq!(
        rejection(&settings),
        "focal plane must be positive when the physical camera is used"
    );
    settings.defocus.circle_of_confusion.focal_plane = 1.0;

    let cases: [fn(&mut CameraData); 4] = [
        |camera| camera.focal_length = 0.0,
        |camera| camera.f_stop = -1.0,
        |camera| camera.filmback.width = 0.0,
        |camera| camera.filmback.height = f32::NAN,
    ];
    for mutate in cases {
        let mut settings = camera_settings();
        mutate(
            settings
                .defocus
                .circle_of_confusion
                .camera_data
                .as_mut()
                .unwrap(),
        );
        assert_eq!(rejection(&settings), camera_message);
    }

    let mut settings = camera_settings();
    settings.defocus.use_camera_focal = false;
    settings
        .defocus
        .circle_of_confusion
        .camera_data
        .as_mut()
        .unwrap()
        .resolution = Resolution {
        width: 0,
        height: 0,
    };
    assert_eq!(rejection(&settings), camera_message);
    settings.defocus.use_camera_focal = true;
    assert!(validate_adaptive_settings(&settings).is_ok());

    let mut settings = camera_settings();
    settings
        .defocus
        .circle_of_confusion
        .camera_data
        .as_mut()
        .unwrap()
        .world_unit = 99;
    assert_eq!(rejection(&settings), "unknown physical camera world unit");
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a non-finite lens centre on either axis is rejected.
fn lens_center_must_be_finite() {
    let mut settings = depth_settings();
    settings.render.center.x = f32::NAN;
    assert_eq!(rejection(&settings), "lens center must be finite");

    let mut settings = depth_settings();
    settings.render.center.y = f32::INFINITY;
    assert_eq!(rejection(&settings), "lens center must be finite");
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies catseye amount, softness and gamma are checked only when catseye is enabled.
fn catseye_parameters_are_checked_only_when_enabled() {
    let mut settings = depth_settings();
    settings.non_uniform.catseye.amount = -0.1;
    settings.non_uniform.catseye.gamma = 0.0;
    assert!(validate_adaptive_settings(&settings).is_ok());

    settings.non_uniform.catseye.enable = true;
    assert_eq!(
        rejection(&settings),
        "catseye parameters must be finite and nonnegative"
    );
    settings.non_uniform.catseye.amount = 0.5;
    settings.non_uniform.catseye.softness = -1.0;
    assert_eq!(
        rejection(&settings),
        "catseye parameters must be finite and nonnegative"
    );
    settings.non_uniform.catseye.softness = 0.2;
    assert_eq!(rejection(&settings), "catseye gamma must be positive");
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies barndoor edge values and gamma are checked only when barndoors are enabled.
fn barndoor_parameters_are_checked_only_when_enabled() {
    let mut settings = depth_settings();
    settings.non_uniform.barndoors.top = -1.0;
    settings.non_uniform.barndoors.gamma = -1.0;
    assert!(validate_adaptive_settings(&settings).is_ok());

    settings.non_uniform.barndoors.enable = true;
    assert_eq!(
        rejection(&settings),
        "barndoor parameters must be finite and nonnegative"
    );
    settings.non_uniform.barndoors.top = 0.0;
    settings.non_uniform.barndoors.right = f32::NAN;
    assert_eq!(
        rejection(&settings),
        "barndoor parameters must be finite and nonnegative"
    );
    settings.non_uniform.barndoors.right = 0.0;
    assert_eq!(rejection(&settings), "barndoor gamma must be positive");
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies astigmatism amount and gamma are checked only when astigmatism is enabled.
fn astigmatism_parameters_are_checked_only_when_enabled() {
    let mut settings = depth_settings();
    settings.non_uniform.astigmatism.amount = -1.0;
    assert!(validate_adaptive_settings(&settings).is_ok());

    settings.non_uniform.astigmatism.enable = true;
    assert_eq!(
        rejection(&settings),
        "astigmatism amount must be finite and nonnegative"
    );
    settings.non_uniform.astigmatism.amount = 0.5;
    settings.non_uniform.astigmatism.gamma = 0.0;
    assert_eq!(rejection(&settings), "astigmatism gamma must be positive");
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies axial aberration amount and colour type are checked when the effect is enabled.
fn axial_aberration_amount_and_color_type_are_checked() {
    let mut settings = depth_settings();
    settings.non_uniform.axial_aberration.enable = true;
    settings.non_uniform.axial_aberration.amount = f32::NAN;
    assert_eq!(
        rejection(&settings),
        "axial aberration amount must be finite and nonnegative"
    );
    settings.non_uniform.axial_aberration.amount = -0.5;
    assert_eq!(
        rejection(&settings),
        "axial aberration amount must be finite and nonnegative"
    );

    settings.non_uniform.axial_aberration.amount = 0.5;
    for color_type in [3, -1] {
        settings.non_uniform.axial_aberration.color_type = color_type;
        assert_eq!(
            rejection(&settings),
            "unknown axial aberration color type",
            "color type {color_type}"
        );
    }
    settings.non_uniform.axial_aberration.color_type = 2;
    assert!(validate_adaptive_settings(&settings).is_ok());
}
