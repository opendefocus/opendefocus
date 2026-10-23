//! Region-of-interest requests through the mock host: the padded Source and Depth windows, the
//! whole region when astigmatism can read remote Source pixels, and the failure when the Source
//! region of definition is unavailable.

use super::*;

/// Checks that a window gets the renderer's padding on each side for Source and Depth alike.
#[test]
fn padded_window_is_requested_for_source_and_connected_depth() {
    let session = ready_session();
    let mut settings = default_settings();
    let renderer =
        pollster::block_on(opendefocus::OpenDefocusRenderer::new(&mut settings)).expect("renderer");
    let padding = renderer.padding(&settings) as c_int;
    let frame = [0, 0, 96, 80];
    session.set_clip_image("Source", frame, 4, &vec![0.0; 96 * 80 * 4], false);
    session.set_clip_image("Depth", frame, 1, &vec![0.0; 96 * 80], false);
    let window = [40, 30, 50, 40];
    let (status, out) = session.regions_of_interest(Some(window.map(f64::from)), 0.0);
    assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
    let expected = [
        window[0] - padding,
        window[1] - padding,
        window[2] + padding,
        window[3] + padding,
    ];
    assert_eq!(roi_of(out, "Source"), expected);
    assert_eq!(roi_of(out, "Depth"), expected);
}

/// Checks that a window at the frame corner is clamped to the region of definition.
#[test]
fn padded_window_is_clamped_to_the_region_of_definition() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 96, 80], 4, &vec![0.0; 96 * 80 * 4], false);
    let (status, out) = session.regions_of_interest(Some([0.0, 0.0, 4.0, 4.0]), 0.0);
    assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
    let roi = roi_of(out, "Source");
    assert_eq!(roi[0], 0);
    assert_eq!(roi[1], 0);
    assert!(roi[2] > 4 && roi[3] > 4, "{roi:?}");
    assert!(roi[2] <= 96 && roi[3] <= 80, "{roi:?}");
}

/// Checks that astigmatism, which can read remote Source pixels, requests the whole region.
#[test]
fn astigmatism_requests_the_whole_source_region() {
    let session = ready_session();
    session.set_param_value("astigmatismEnable", Value::Int(1));
    session.set_clip_image("Source", [0, 0, 96, 80], 4, &vec![0.0; 96 * 80 * 4], false);
    let (status, out) = session.regions_of_interest(Some([40.0, 30.0, 50.0, 40.0]), 0.0);
    assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
    assert_eq!(roi_of(out, "Source"), [0, 0, 96, 80]);
}

/// Checks that a request with no Source image fails, naming the unavailable region of definition.
#[test]
fn missing_source_image_fails_the_region_request() {
    let session = ready_session();
    let (status, out) = session.regions_of_interest(Some([40.0, 30.0, 50.0, 40.0]), 0.0);
    assert_eq!(status, OFX_STAT_FAILED);
    assert_eq!(prop_count(out, c"OfxImageClipPropRoI_Source"), 0);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with("Source region of definition is unavailable"),
        "message: {text}"
    );
}
