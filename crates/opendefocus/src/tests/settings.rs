//! Checks that adaptive settings validation accepts valid settings and rejects invalid ones.
use super::*;
use crate::datamodel::{
    Settings, UVector2,
    defocus::DefocusMode,
    render::{FilterMode, Quality, RenderSpecs},
};

#[cfg(feature = "wgpu")]
#[test]
/// Verifies non-uniform apertures are accepted and invalid lens values are rejected.
fn adaptive_settings_accept_nonuniform_apertures_and_reject_invalid_lens_values() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.bokeh.aspect_ratio = 2.0;
    settings.defocus.circle_of_confusion.pixel_aspect = 1.5;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.render.filter.mode = FilterMode::BokehCreator as i32;
    settings.bokeh.aspect_ratio = 0.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.bokeh.aspect_ratio = f32::NAN;
    assert!(validate_adaptive_settings(&settings).is_err());
    settings.bokeh.aspect_ratio = 1.0;
    settings.defocus.circle_of_confusion.pixel_aspect = 1.0;
    settings.non_uniform.catseye.enable = true;
    settings.non_uniform.catseye.softness = f32::NAN;
    assert!(validate_adaptive_settings(&settings).is_err());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a radius that would stall the GPU is rejected.
fn adaptive_settings_reject_an_effective_radius_that_would_stall_the_gpu() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.circle_of_confusion.max_size = 1000.0;
    settings.defocus.size_multiplier = 10.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.defocus.size_multiplier = 100.0;
    assert!(validate_adaptive_settings(&settings).is_err());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies unknown enum values and a zero filter resolution are rejected.
fn adaptive_settings_reject_unknown_enums_and_zero_filter_resolution() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.render.result_mode = 99;
    assert!(validate_adaptive_settings(&settings).is_err());

    settings.render.result_mode = datamodel::render::ResultMode::Result as i32;
    settings.render.filter.mode = 99;
    assert!(validate_adaptive_settings(&settings).is_err());

    settings.render.filter.mode = FilterMode::Simple as i32;
    settings.render.quality = 99;
    assert!(validate_adaptive_settings(&settings).is_err());

    settings.render.quality = Quality::Fast as i32;
    settings.defocus.defocus_mode = 99;
    assert!(validate_adaptive_settings(&settings).is_err());

    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.non_uniform.axial_aberration.enable = true;
    settings.non_uniform.axial_aberration.color_type = 99;
    assert!(validate_adaptive_settings(&settings).is_err());

    settings.non_uniform.axial_aberration.color_type = 0;
    settings.non_uniform.axial_aberration.enable = false;
    settings.non_uniform.inverse_foreground = false;
    settings.render.filter.resolution = 0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.render.filter.mode = FilterMode::BokehCreator as i32;
    assert!(validate_adaptive_settings(&settings).is_err());
    settings.render.filter.mode = FilterMode::Simple as i32;
    settings.non_uniform.inverse_foreground = true;
    assert!(validate_adaptive_settings(&settings).is_err());
    settings.render.filter.resolution = 128;
    settings.bokeh.aspect_ratio = 0.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.bokeh.aspect_ratio = -0.1;
    assert!(validate_adaptive_settings(&settings).is_err());
    settings.bokeh.aspect_ratio = f32::NAN;
    assert!(validate_adaptive_settings(&settings).is_err());
    settings.non_uniform.inverse_foreground = false;
    settings.render.filter.mode = FilterMode::Image as i32;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.render.filter.preview = true;
    settings.defocus.defocus_mode = 99;
    assert!(validate_adaptive_settings(&settings).is_err());
}

#[test]
/// Verifies padding keeps the extended support of axial and astigmatic effects.
fn adaptive_padding_retains_extended_axial_and_astigmatic_support() {
    let mut settings = Settings::default();
    settings.defocus.circle_of_confusion.max_size = 32.0;
    settings.defocus.camera_max_size = 32.0;
    settings.render.resolution = UVector2 { x: 640, y: 480 };
    let normal = adaptive_source_padding(&settings);
    settings.non_uniform.axial_aberration.enable = true;
    settings.non_uniform.axial_aberration.amount = 12.0;
    assert!(adaptive_source_padding(&settings) >= normal * 2 - 16);
    settings.non_uniform.axial_aberration.amount = -12.0;
    assert!(adaptive_source_padding(&settings) >= normal * 2 - 16);
    settings.non_uniform.astigmatism.enable = true;
    settings.non_uniform.astigmatism.amount = 2.0;
    assert!(adaptive_source_padding(&settings) >= 640);
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies the simple filter resolution uses the filter box extent.
fn adaptive_simple_filter_resolution_uses_filter_box_extent() {
    let mut settings = Settings::default();
    settings.render.filter.mode = FilterMode::Simple as i32;
    settings.render.filter.resolution = 256;
    settings.bokeh.aspect_ratio = 2.0;
    let bounds = settings
        .render
        .filter
        .calculate_filter_box(settings.bokeh.aspect_ratio);
    assert_eq!(
        adaptive_filter_resolution(&settings),
        glam::UVec2::new(bounds[2] - bounds[0], bounds[3] - bounds[1])
    );
    assert_eq!(
        adaptive_filter_resolution(&settings),
        glam::UVec2::new(256, 86)
    );

    let specs = RenderSpecs::from_rects(glam::IVec4::ZERO, glam::IVec4::ZERO);
    let lens = crate::datamodel::settings_to_convolve_settings(
        &settings,
        &specs,
        adaptive_filter_resolution(&settings),
        1,
    );
    let foreground_phase = glam::Vec2::splat(8.0) / lens.filter_resolution.as_vec2();
    assert!((foreground_phase.x - 8.0 / 256.0).abs() < f32::EPSILON);
    assert!((foreground_phase.y - 8.0 / 86.0).abs() < f32::EPSILON);
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies legacy focal offset and gamma values are accepted and ignored.
fn adaptive_settings_accept_legacy_ignored_focal_offset_and_gamma() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.focal_plane_offset = 1.0;
    settings.defocus.gamma_correction = 2.0;
    assert!(validate_adaptive_settings(&settings).is_ok());

    settings.non_uniform.catseye.gamma = 0.0;
    assert!(validate_adaptive_settings(&settings).is_ok());
    settings.non_uniform.catseye.enable = true;
    assert!(validate_adaptive_settings(&settings).is_err());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies valid 2D settings are accepted and an invalid 2D radius is rejected.
fn adaptive_settings_accept_valid_2d_and_reject_invalid_2d_radius() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Twod);
    assert!(validate_adaptive_settings(&settings).is_ok());

    settings.defocus.circle_of_confusion.size = f32::NAN;
    assert!(validate_adaptive_settings(&settings).is_err());

    settings.defocus.circle_of_confusion.size = 8.0;
    settings.defocus.size_multiplier = -1.0;
    assert!(validate_adaptive_settings(&settings).is_err());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies a non-positive physical camera focus is rejected.
fn adaptive_settings_reject_nonpositive_physical_camera_focus() {
    use datamodel::circle_of_confusion::{Filmback, Resolution, WorldUnit};

    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = false;
    settings.defocus.circle_of_confusion.camera_data =
        Some(datamodel::circle_of_confusion::CameraData {
            focal_length: 50.0,
            f_stop: 2.0,
            filmback: Filmback {
                width: 24.0,
                height: 18.0,
            },
            near_field: 0.1,
            far_field: 10_000.0,
            world_unit: WorldUnit::M.into(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
        });
    settings.defocus.circle_of_confusion.focal_plane = -1.0;
    assert!(validate_adaptive_settings(&settings).is_err());
}

#[cfg(feature = "wgpu")]
#[test]
/// Verifies unknown physical camera enum values are rejected.
fn adaptive_settings_reject_unknown_physical_camera_enums() {
    use datamodel::circle_of_confusion::{Filmback, Resolution, WorldUnit};

    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = false;
    settings.defocus.circle_of_confusion.camera_data =
        Some(datamodel::circle_of_confusion::CameraData {
            focal_length: 50.0,
            f_stop: 2.0,
            filmback: Filmback {
                width: 24.0,
                height: 18.0,
            },
            near_field: 0.1,
            far_field: 10_000.0,
            world_unit: WorldUnit::M.into(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
        });

    settings.defocus.circle_of_confusion.math = 99;
    assert!(validate_adaptive_settings(&settings).is_err());
    settings.defocus.circle_of_confusion.math = datamodel::circle_of_confusion::Math::Real.into();
    settings
        .defocus
        .circle_of_confusion
        .camera_data
        .as_mut()
        .unwrap()
        .world_unit = 99;
    assert!(validate_adaptive_settings(&settings).is_err());
}
