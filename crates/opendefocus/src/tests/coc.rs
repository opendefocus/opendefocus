//! Checks that adaptive CoC preparation matches the established depth conversions and sign convention.
use super::*;
use crate::datamodel::{Settings, UVector2, defocus::DefocusMode};
use ndarray::Array2;

#[test]
/// Verifies direct CoC input passes through unchanged.
fn adaptive_direct_depth_preparation_matches_legacy_passthrough() {
    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = true;
    settings.defocus.circle_of_confusion.max_size = 4.0;
    let depth = Array2::from_shape_vec((1, 5), vec![-12.0, -2.0, 0.0, 2.0, 12.0]).unwrap();
    assert_eq!(adaptive_coc_from_depth(&depth, &settings), depth);
}

#[test]
/// Verifies 2D mode produces the constant CoC plane.
fn adaptive_2d_depth_preparation_matches_legacy_constant() {
    let mut settings = Settings::default();
    settings
        .defocus
        .set_defocus_mode(datamodel::defocus::DefocusMode::Twod);
    settings.defocus.circle_of_confusion.size = 7.0;
    settings.defocus.size_multiplier = 1.5;
    let depth = Array2::from_shape_vec((2, 3), vec![0.0; 6]).unwrap();
    assert_eq!(
        adaptive_coc_from_depth(&depth, &settings),
        Array2::from_elem((1, 1), settings.defocus.get_size())
    );
}

#[test]
/// Verifies camera-derived CoC matches the legacy calculator.
fn adaptive_camera_depth_preparation_matches_legacy_calculator() {
    use datamodel::circle_of_confusion::{Filmback, Math, Resolution, WorldUnit};

    let mut settings = Settings::default();
    settings
        .defocus
        .set_defocus_mode(datamodel::defocus::DefocusMode::Depth);
    settings.defocus.use_direct_math = false;
    settings.defocus.camera_max_size = 24.0;
    settings.defocus.use_camera_focal = true;
    settings.render.resolution = datamodel::UVector2 { x: 3, y: 1 };
    settings.defocus.circle_of_confusion = datamodel::circle_of_confusion::Settings {
        size: 10.0,
        max_size: 24.0,
        math: Math::Real.into(),
        focal_plane: 30.0,
        protect: 0.0,
        pixel_aspect: 1.0,
        camera_data: Some(datamodel::circle_of_confusion::CameraData {
            focal_length: 100.0,
            f_stop: 2.0,
            filmback: Filmback {
                width: 24.576,
                height: 18.672,
            },
            near_field: 0.1,
            far_field: 10_000.0,
            world_unit: WorldUnit::M.into(),
            resolution: Resolution {
                width: 1,
                height: 1,
            },
        }),
    };
    let depth = Array2::from_shape_vec((1, 3), vec![10.0, 30.0, 100.0]).unwrap();

    let mut reference_settings = settings.defocus.circle_of_confusion;
    reference_settings.max_size = settings.defocus.get_max_size();
    reference_settings.size = settings.defocus.get_size();
    if let Some(camera) = reference_settings.camera_data.as_mut() {
        camera.resolution = datamodel::circle_of_confusion::Resolution {
            width: settings.render.resolution.x,
            height: settings.render.resolution.y,
        };
    }
    reference_settings.math = match reference_settings.math() {
        datamodel::circle_of_confusion::Math::OneDividedByZ => {
            datamodel::circle_of_confusion::Math::OneDividedByZ.into()
        }
        datamodel::circle_of_confusion::Math::Real => {
            datamodel::circle_of_confusion::Math::Real.into()
        }
    };
    let calculator = datamodel::circle_of_confusion::Calculator::new(reference_settings);
    let expected =
        depth.mapv(|value| -datamodel::circle_of_confusion::calculate(&calculator, value));
    assert_eq!(adaptive_coc_from_depth(&depth, &settings), expected);
}

#[test]
/// Verifies camera CoC tracks the focal plane and keeps the legacy polarity.
fn adaptive_camera_coc_tracks_focal_plane_and_keeps_legacy_polarity() {
    use datamodel::circle_of_confusion::{Filmback, Math, Resolution, WorldUnit};

    let mut settings = Settings::default();
    settings.defocus.set_defocus_mode(DefocusMode::Depth);
    settings.defocus.use_direct_math = false;
    settings.defocus.use_camera_focal = true;
    settings.render.resolution = UVector2 { x: 1920, y: 1080 };
    settings.defocus.camera_max_size = 24.0;
    settings.defocus.circle_of_confusion = datamodel::circle_of_confusion::Settings {
        size: 10.0,
        max_size: 24.0,
        math: Math::Real.into(),
        focal_plane: 30.0,
        protect: 0.0,
        pixel_aspect: 1.0,
        camera_data: Some(datamodel::circle_of_confusion::CameraData {
            focal_length: 100.0,
            f_stop: 2.0,
            filmback: Filmback {
                width: 24.576,
                height: 18.672,
            },
            near_field: 0.1,
            far_field: 10_000.0,
            world_unit: WorldUnit::M.into(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
        }),
    };
    let depth = Array2::from_shape_vec((1, 3), vec![10.0, 30.0, 100.0]).unwrap();
    let at_thirty = adaptive_coc_from_depth(&depth, &settings);
    assert!(at_thirty[[0, 0]] > 0.0, "near CoC is foreground-positive");
    assert_eq!(at_thirty[[0, 1]], 0.0, "the focal plane stays sharp");
    assert!(at_thirty[[0, 2]] < 0.0, "far CoC is background-negative");

    settings.defocus.circle_of_confusion.focal_plane = 10.0;
    let at_ten = adaptive_coc_from_depth(&depth, &settings);
    assert_eq!(at_ten[[0, 0]], 0.0);
    assert_ne!(at_ten[[0, 2]], at_thirty[[0, 2]]);
}
