//! Integration tests for the published circle of confusion conventions: camera
//! radius, sign, world units, reciprocal depth, pixel aspect and clipping.

use circle_of_confusion::{
    Calculator, CameraData, Filmback, Math, Resolution, Settings, WorldUnit,
};

/// Returns the reference camera setup (100 mm at f/2, 1920x1080) used by these tests.
fn camera_settings() -> Settings {
    Settings {
        size: 10.0,
        max_size: 100.0,
        math: Math::Real.into(),
        focal_plane: 30.0,
        protect: 0.0,
        pixel_aspect: 1.0,
        camera_data: Some(CameraData {
            focal_length: 100.0,
            f_stop: 2.0,
            filmback: Filmback {
                width: 24.576,
                height: 18.672,
            },
            near_field: 0.1,
            far_field: 10000.0,
            world_unit: WorldUnit::M.into(),
            resolution: Resolution {
                width: 1920,
                height: 1080,
            },
        }),
    }
}

/// Checks a camera-based CoC value against the published radius, its sign on each side of focus, and zero at focus.
#[test]
fn test_published_camera_radius_and_sign() {
    let calculator = Calculator::new(camera_settings());
    assert_eq!(
        calculator.calculate(10.0).to_bits(),
        (-11.935329_f32).to_bits()
    );
    assert_eq!(calculator.calculate(30.0), 0.0);
    assert!(calculator.calculate(60.0) > 0.0);
}

/// Checks that world-unit scaling and 1/Z depth mode reproduce the baseline CoC exactly.
#[test]
fn test_world_units_and_reciprocal_depth() {
    let baseline = Calculator::new(camera_settings()).calculate(10.0);
    let mut mm = camera_settings();
    mm.focal_plane *= 1000.0;
    mm.camera_data.as_mut().unwrap().world_unit = WorldUnit::Mm.into();
    assert_eq!(
        Calculator::new(mm).calculate(10000.0).to_bits(),
        baseline.to_bits()
    );
    let mut inverse = camera_settings();
    inverse.math = Math::OneDividedByZ.into();
    inverse.focal_plane = 1.0 / inverse.focal_plane;
    assert!((Calculator::new(inverse).calculate(0.1) - baseline).abs() < 0.00001);
}

/// Checks pixel-aspect scaling, max-size clamping, and that near/far clip planes do not change the result.
#[test]
fn test_pixel_aspect_clamp_and_unused_clip_planes() {
    let baseline = Calculator::new(camera_settings()).calculate(10.0);
    let mut settings = camera_settings();
    settings.pixel_aspect = 2.0;
    assert_eq!(Calculator::new(settings).calculate(10.0), baseline * 2.0);
    settings.max_size = 5.0;
    assert_eq!(Calculator::new(settings).calculate(10.0), -5.0);
    let mut settings = camera_settings();
    let camera = settings.camera_data.as_mut().unwrap();
    camera.near_field = 100.0;
    camera.far_field = 200.0;
    assert_eq!(
        Calculator::new(settings).calculate(10.0).to_bits(),
        baseline.to_bits()
    );
}

/// Checks that depth zero yields the same CoC as the published far sentinel of 9999.
#[test]
fn test_zero_depth_keeps_published_sentinel() {
    let calculator = Calculator::new(camera_settings());
    assert_eq!(
        calculator.calculate(0.0).to_bits(),
        calculator.calculate(9999.0).to_bits()
    );
}
