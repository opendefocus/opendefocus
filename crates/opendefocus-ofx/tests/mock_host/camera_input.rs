//! Tests for the connected Nuke camera input: whether the camera suite is defined, and whether camera values replace the plain camera parameters.

use super::*;

// Each session holds the test serialization lock, so a test builds one session at a time and
// reads its values before the next one starts (see `settings_in`).

/// Load, Describe and `DescribeInContext` on a session, with the Nuke camera suite when given.
pub(super) fn described_session(camera: Option<MockCamera>) -> Session {
    let session = match camera {
        Some(camera) => Session::with_nuke_camera(camera),
        None => Session::new(&[]),
    };
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    session
}

/// The settings a render reads for a session with `camera` and the plain rows in `rows`, then
/// the session is dropped.
pub(super) fn settings_in(camera: Option<MockCamera>, rows: &[(&str, Value)]) -> Settings {
    let session = described_session(camera);
    for (name, value) in rows {
        session.set_param_value(name, value.clone());
    }
    session.settings(0.0).expect("settings are read")
}

/// The settings `read_settings` gives for the table defaults, with the render fields it sets.
pub(super) fn default_render_settings() -> Settings {
    let mut settings = default_settings();
    settings.render.gui = false;
    settings.render.use_gpu_if_available = true;
    settings
}

/// Creates a mock camera with the given focal length and filmback sizes.
pub(super) fn connected(focal: f64, haperture: f64, vaperture: f64) -> MockCamera {
    MockCamera {
        connected: true,
        focal,
        haperture,
        vaperture,
    }
}

/// (a) Without the suite the plugin defines no Camera input and reads the plain rows as before.
#[test]
fn without_the_camera_suite_no_camera_input_is_defined() {
    let session = described_session(None);
    assert_eq!(session.camera_inputs(), [] as [std::string::String; 0]);
    assert_eq!(
        session.settings(0.0).expect("settings are read"),
        default_render_settings()
    );
}

/// Checks that with the camera suite the camera input is defined and marked optional.
#[test]
fn with_the_camera_suite_the_camera_input_is_defined_optional() {
    let session = described_session(Some(connected(35.0, 36.0, 24.0)));
    assert_eq!(session.camera_inputs(), vec!["Camera".to_string()]);
    let props = session.camera_props().expect("camera property set");
    assert_eq!(prop_int(props, OFX_IMAGE_CLIP_PROP_OPTIONAL, 0), Some(1));
}

/// (b) A connected camera replaces the focal length and filmback of the plain rows.
#[test]
fn connected_camera_supplies_focal_length_and_filmback() {
    // The plain rows are set too; the camera's values must win for focal length and filmback.
    let settings = settings_in(
        Some(connected(35.0, 36.0, 24.0)),
        &[
            ("focalLength", Value::Double(80.0)),
            ("filmbackWidth", Value::Double(10.0)),
            ("filmbackHeight", Value::Double(5.0)),
        ],
    );
    let camera = settings
        .defocus
        .circle_of_confusion
        .camera_data
        .expect("the camera is used");
    assert_eq!(camera.focal_length, 35.0);
    assert_eq!(camera.filmback.width, 36.0);
    assert_eq!(camera.filmback.height, 24.0);
    // F-stop and world unit have no Nuke source, so they keep their plain-row values.
    assert_eq!(camera.f_stop, CameraData::default().f_stop);
    assert_eq!(camera.world_unit, CameraData::default().world_unit);
}

/// (b) The connected camera gives the same settings as the plain rows set to its values, so
/// the Nuke path is the plain camera path with the values from the camera.
#[test]
fn connected_camera_settings_equal_plain_camera_rows_with_the_same_values() {
    let from_camera = settings_in(Some(connected(35.0, 36.0, 24.0)), &[]);
    let plain = settings_in(
        None,
        &[
            ("cameraEnabled", Value::Int(1)),
            ("focalLength", Value::Double(35.0)),
            ("filmbackWidth", Value::Double(36.0)),
            ("filmbackHeight", Value::Double(24.0)),
        ],
    );
    assert_eq!(from_camera, plain);
}

/// (b) Different camera values give different settings, so the render depends on the camera.
#[test]
fn different_camera_values_give_different_settings() {
    let near = settings_in(Some(connected(35.0, 36.0, 24.0)), &[]);
    let far = settings_in(Some(connected(85.0, 36.0, 24.0)), &[]);
    assert_ne!(near, far);
}

/// (c) A camera input that is defined but not connected falls back to the plain rows.
#[test]
fn unconnected_camera_falls_back_to_plain_parameters() {
    let rows = [
        ("cameraEnabled", Value::Int(1)),
        ("focalLength", Value::Double(80.0)),
    ];
    let unconnected = {
        let session = described_session(Some(MockCamera {
            connected: false,
            focal: 35.0,
            haperture: 36.0,
            vaperture: 24.0,
        }));
        assert_eq!(session.camera_inputs(), vec!["Camera".to_string()]);
        for (name, value) in &rows {
            session.set_param_value(name, value.clone());
        }
        session.settings(0.0).expect("settings are read")
    };
    let plain = settings_in(None, &rows);
    assert_eq!(unconnected, plain);
    let camera = unconnected
        .defocus
        .circle_of_confusion
        .camera_data
        .expect("plain camera toggle is on");
    assert_eq!(camera.focal_length, 80.0, "plain row, not the camera's 35");
}

/// (c) An unconnected camera with the plain defaults gives the plain defaults.
#[test]
fn unconnected_camera_with_defaults_gives_the_plain_defaults() {
    let settings = settings_in(
        Some(MockCamera {
            connected: false,
            focal: 35.0,
            haperture: 36.0,
            vaperture: 24.0,
        }),
        &[],
    );
    assert_eq!(settings, default_render_settings());
}

/// The connected camera's values reach the renderer: a render with two focal lengths differs.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_connected_camera_focal_length_changes_the_render() {
    let render_with_focal = |focal: f64| -> Vec<f32> {
        let session = described_session(Some(connected(focal, 24.576, 18.672)));
        session.set_param_value("defocusMode", Value::Int(1));
        session.set_param_value("focalPlane", Value::Double(10.0));
        assert_eq!(session.create_instance(), OFX_STAT_OK);
        let size = [0, 0, TILE_W as c_int, TILE_H as c_int];
        session.set_clip_image("Source", size, 4, &tile_scene_colour(), false);
        session.set_clip_image("Depth", size, 4, &tile_scene_depth(), false);
        session.set_clip_image("Output", size, 4, &vec![0.0; TILE_W * TILE_H * 4], false);
        assert_eq!(
            session.render(size, 0.0),
            OFX_STAT_OK,
            "message: {:?}",
            session.last_message()
        );
        session.clip_logical("Output")
    };
    let near = render_with_focal(35.0);
    let far = render_with_focal(200.0);
    assert_finite_alpha_within_eps(&near);
    assert_finite_alpha_within_eps(&far);
    assert_ne!(near, far, "the camera focal length must reach the renderer");
}
