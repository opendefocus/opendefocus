//! Camera input edge cases through the mock host: a camera input the host cannot define or look up,
//! and a connected camera whose parameter reads fail.

use super::*;

/// A session with a connected camera, loaded and described with its filter context.
fn described_camera(connected: bool) -> Session {
    let session = Session::with_nuke_camera(MockCamera {
        connected,
        focal: 50.0,
        haperture: 36.0,
        vaperture: 24.0,
    });
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    session
}

/// Checks that a camera whose descriptor property set is null fails the context with a bad handle.
#[test]
fn camera_without_a_property_set_is_a_bad_handle() {
    let session = described_camera(true);
    with(|s| {
        s.nuke_camera
            .as_mut()
            .expect("the session has a camera")
            .props = 0;
    });
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_ERR_BAD_HANDLE
    );
}

/// Checks that a connected camera the host no longer resolves is treated as no camera, so the settings
/// keep the plain parameter values and the render is not blocked.
#[test]
fn unresolved_camera_input_is_treated_as_no_camera() {
    let session = described_camera(true);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    with(|s| s.camera_inputs.clear());
    assert!(
        session.settings(0.0).is_ok(),
        "settings read without the camera"
    );
}

/// Checks that a camera reported as connected whose parameters cannot be read fails the settings with
/// the host's status.
#[test]
fn camera_parameter_read_failure_fails_the_settings() {
    let session = described_camera(true);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    with(|s| {
        s.nuke_camera
            .as_mut()
            .expect("the session has a camera")
            .connected = false;
    });
    let error = session.settings(0.0).err().map(|e| e.0);
    assert_eq!(error, Some(OFX_STAT_FAILED));
}
