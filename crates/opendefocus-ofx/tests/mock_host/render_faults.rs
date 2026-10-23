//! Render and region-of-interest failures through the mock host that the plugin reports as messages:
//! an empty window, a missing instance, a missing region request, a byte-depth clip. The abort
//! case, which needs the renderer, is ignored with the other GPU tests.

use super::*;
use opendefocus_ofx::ofx_sys::OFX_STAT_ERR_UNKNOWN;

/// Checks that a render window with no area fails with the empty-window message.
#[test]
fn empty_render_window_is_reported() {
    let session = ready_session();
    assert_eq!(session.render([0, 0, 0, 0], 0.0), OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some("render window is empty")
    );
}

/// Checks that a render on an effect with no instance reports the failed lookup of its instance data,
/// with the host's status, and fetches no image.
#[test]
fn render_without_an_instance_reports_the_failed_lookup() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some(
            format!(
                "OFX host call failed: OFX host call failed with status {OFX_STAT_ERR_UNKNOWN}"
            )
            .as_str()
        )
    );
    assert_eq!(session.images_fetched(), 0);
}

/// Checks that a region-of-interest request without its region property fails with the host's status
/// rather than defaulting the region.
#[test]
fn regions_without_a_region_of_interest_fail() {
    let session = ready_session();
    let out = with(State::alloc) as OfxPropertySetHandle;
    let status = Session::action_with_out(
        OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST,
        session.effect(),
        time_args(0.0),
        out,
    );
    assert_eq!(status, OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some(
            format!(
                "OFX host call failed: OFX host call failed with status {OFX_STAT_ERR_UNKNOWN}"
            )
            .as_str()
        )
    );
}

/// Checks that a clip that is not float is rejected before the render, and the message names the depth.
#[test]
fn byte_depth_clip_is_rejected() {
    let session = ready_session();
    session.use_byte_depth();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with("Source clip must be float, found OfxBitDepthByte (window [0 0 16 16]"),
        "message: {text}"
    );
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Checks that a host abort request stops the render without a message and releases its images. Needs
/// a WGPU adapter, since the abort is checked after the renderer is created.
#[test]
#[ignore = "needs a WGPU adapter"]
fn render_stops_when_the_host_asks_to_abort() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    session.request_abort();
    assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
    assert_eq!(message_text(&session), None);
    assert_eq!(session.images_fetched(), session.images_released());
}
