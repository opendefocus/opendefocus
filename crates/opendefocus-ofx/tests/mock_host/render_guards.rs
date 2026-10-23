//! Render-action guards through the mock host: component and bounds checks on each image, the
//! output window that a failed render clears (with host row order and an offset window), and the
//! instance lookup that every render and region request depends on.

use super::*;

/// Checks that a single-channel Source is rejected, reports its components, and clears the output.
#[test]
fn alpha_only_source_is_rejected_and_the_output_is_cleared() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 1, &vec![0.0; 16 * 16], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![1.0; 16 * 16 * 4], false);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with(
            "Source clip has unsupported components OfxImageComponentAlpha (window [0 0 16 16]"
        ),
        "message: {text}"
    );
    assert!(session.clip_logical("Output").iter().all(|&v| v == 0.0));
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Checks that an alpha-only Output is rejected because the renderer writes RGBA.
#[test]
fn alpha_only_output_is_rejected() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 1, &vec![0.0; 16 * 16], false);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with("Output clip has unsupported components OfxImageComponentAlpha"),
        "message: {text}"
    );
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Checks that an Output smaller than the window fails with its bounds and the window it misses.
#[test]
fn output_bounds_must_cover_the_render_window() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 8, 8], 4, &vec![0.0; 8 * 8 * 4], false);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with(
            "Output image bounds [0 0 8 8] do not cover the requested region [0 0 16 16]"
        ),
        "message: {text}"
    );
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Checks that a single-channel Depth clip is accepted as alpha but must still cover the window.
#[test]
fn alpha_depth_must_cover_the_render_window_and_the_output_is_cleared() {
    let session = ready_session();
    session.set_param_value("defocusMode", Value::Int(1));
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Depth", [0, 0, 8, 8], 1, &vec![0.0; 8 * 8], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![1.0; 16 * 16 * 4], false);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with(
            "Depth image bounds [0 0 8 8] do not cover the requested region [0 0 16 16]"
        ),
        "message: {text}"
    );
    assert!(session.clip_logical("Output").iter().all(|&v| v == 0.0));
    assert_eq!(session.images_fetched(), 3);
    assert_eq!(session.images_released(), 3);
}

/// Checks that a failed render clears only its window, at the window's offset in a host buffer
/// stored top-down (negative rowBytes), and leaves the rest of the output untouched.
#[test]
fn failed_render_clears_only_its_window_in_a_top_down_output() {
    let session = ready_session();
    // The Source covers only the corner, so the window at (16, 16)-(32, 32) is not covered.
    session.set_clip_image_with_rod(
        "Source",
        [0, 0, 8, 8],
        [0, 0, 64, 64],
        4,
        &vec![0.0; 8 * 8 * 4],
        false,
    );
    session.set_clip_image("Output", [0, 0, 64, 64], 4, &vec![1.0; 64 * 64 * 4], true);
    assert_eq!(session.render([16, 16, 32, 32], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with(
            "Source image bounds [0 0 8 8] do not cover the requested region [16 16 32 32]"
        ),
        "message: {text}"
    );
    let expected: Vec<f32> = (0..64 * 64)
        .flat_map(|i| {
            let (x, y) = (i % 64, i / 64);
            let inside = (16..32).contains(&x) && (16..32).contains(&y);
            [if inside { 0.0 } else { 1.0 }; 4]
        })
        .collect();
    assert_eq!(session.clip_logical("Output"), expected);
}

/// Checks that a render and a region request fail once the instance data is gone, before any
/// image is fetched.
#[test]
fn requests_without_instance_data_fail_before_fetching_images() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    assert_eq!(session.destroy_instance(), OFX_STAT_OK);
    assert_eq!(session.destroy_instance(), OFX_STAT_OK);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some("effect has no instance data")
    );
    let (status, _) = session.regions_of_interest(Some([0.0, 0.0, 8.0, 8.0]), 0.0);
    assert_eq!(status, OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some("effect has no instance data")
    );
    assert_eq!(session.images_fetched(), 0);
}
