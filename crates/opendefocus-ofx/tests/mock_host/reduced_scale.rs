//! Render tests at reduced render scale: tiles at half and quarter scale must match the full frame, and source size and failure handling must hold.

use super::*;

/// Canonical frame size of the scaled tests; the pixel frame is this times the render scale.
pub(super) const CANON_W: usize = 96;
/// Height of the canonical test frame in canonical units.
pub(super) const CANON_H: usize = 80;

/// Premultiplied pixels of a `w x h` frame: a hard vertical edge and one HDR dot.
pub(super) fn scaled_scene(w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; w * h * 4];
    for (i, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % w, i / w);
        let dot = (x as i32 - (w / 3) as i32).abs() <= 1 && (y as i32 - (h / 3) as i32).abs() <= 1;
        let rgb = if dot {
            [5.0, 4.0, 2.0]
        } else if x < w / 2 {
            [0.2, 0.5, 0.8]
        } else {
            [0.7, 0.3, 0.1]
        };
        px.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 1.0]);
    }
    out
}

/// Maps a canonical rectangle to pixels at `scale`. The test values divide exactly.
pub(super) fn to_pixels(canonical: [c_int; 4], scale: f64) -> [c_int; 4] {
    canonical.map(|v| (f64::from(v) * scale) as c_int)
}

/// Renders the scene at `scale` as one frame and as two uneven tiles whose windows are given in
/// canonical coordinates, and checks that the stitched tiles equal the frame.
pub(super) fn check_scaled_tiles_match_full_frame(scale: f64) {
    let session = ready_session();
    session.set_render_scale(scale, 1.0);
    let (pw, ph) = (
        (CANON_W as f64 * scale) as usize,
        (CANON_H as f64 * scale) as usize,
    );
    let frame = [0, 0, pw as c_int, ph as c_int];
    let colour = scaled_scene(pw, ph);

    session.set_clip_image("Source", frame, 4, &colour, false);
    session.set_clip_image("Output", frame, 4, &vec![0.0; pw * ph * 4], false);
    assert_eq!(
        session.render(frame, 0.0),
        OFX_STAT_OK,
        "message: {:?}",
        session.last_message()
    );
    let full = session.clip_logical("Output");

    let mut stitched = vec![f32::NAN; pw * ph * 4];
    for canonical in [[0, 0, 40, 80], [40, 0, 96, 80]] {
        let window = to_pixels(canonical, scale);
        let (status, roi) = session.regions_of_interest(Some(canonical.map(f64::from)), 0.0);
        assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
        let source_roi = to_pixels(roi_of(roi, "Source"), scale);
        session.set_clip_image_with_rod(
            "Source",
            source_roi,
            frame,
            4,
            &crop_rows(&colour, pw, source_roi),
            false,
        );
        let (w, h) = (window[2] - window[0], window[3] - window[1]);
        session.set_clip_image("Output", window, 4, &vec![0.0; (w * h * 4) as usize], false);
        assert_eq!(
            session.render(window, 0.0),
            OFX_STAT_OK,
            "message: {:?}",
            session.last_message()
        );
        let tile = session.clip_logical("Output");
        for y in window[1]..window[3] {
            for x in window[0]..window[2] {
                let src = ((y - window[1]) * w + (x - window[0])) as usize * 4;
                let dst = (y as usize * pw + x as usize) * 4;
                stitched[dst..dst + 4].copy_from_slice(&tile[src..src + 4]);
            }
        }
    }
    assert_stitched_matches_full(&full, &stitched);
}

/// Checks that tiles at half render scale match the full-frame render.
#[test]
#[ignore = "needs a WGPU adapter"]
fn half_scale_tiles_match_the_full_frame() {
    check_scaled_tiles_match_full_frame(0.5);
}

/// Checks that tiles at quarter render scale match the full-frame render.
#[test]
#[ignore = "needs a WGPU adapter"]
fn quarter_scale_tiles_match_the_full_frame() {
    check_scaled_tiles_match_full_frame(0.25);
}

/// The `RoI` answer is canonical: at scale 0.5 or 0.25 it contains the canonical request and stays
/// inside the canonical region of definition.
#[test]
#[ignore = "needs a WGPU adapter"]
fn regions_of_interest_are_canonical_at_reduced_scale() {
    for scale in [0.5, 0.25] {
        let session = ready_session();
        session.set_render_scale(scale, 1.0);
        let pixels = (CANON_W as f64 * scale) as usize;
        session.set_clip_image(
            "Source",
            [0, 0, pixels as c_int, (CANON_H as f64 * scale) as c_int],
            4,
            &scaled_scene(pixels, (CANON_H as f64 * scale) as usize),
            false,
        );
        let (status, out) = session.regions_of_interest(Some([40.0, 40.0, 56.0, 60.0]), 0.0);
        assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
        let roi = roi_of(out, "Source");
        assert!(
            roi[0] <= 40 && roi[1] <= 40 && roi[2] >= 56 && roi[3] >= 60,
            "{roi:?}"
        );
        assert!(
            roi[0] >= 0 && roi[1] >= 0 && roi[2] <= 96 && roi[3] <= 80,
            "{roi:?}"
        );
    }
}

/// The host may give a Source image larger than the window needs; the plugin only requires that
/// the image covers what it reads.
#[test]
#[ignore = "needs a WGPU adapter"]
fn source_image_larger_than_requested_is_accepted() {
    let session = ready_session();
    session.set_render_scale(0.5, 1.0);
    session.set_clip_image_with_rod(
        "Source",
        [0, 0, 48, 48],
        [0, 0, 32, 32],
        4,
        &vec![0.5; 48 * 48 * 4],
        false,
    );
    session.set_clip_image("Output", [0, 0, 32, 32], 4, &vec![0.0; 32 * 32 * 4], false);
    assert_eq!(
        session.render([0, 0, 32, 32], 0.0),
        OFX_STAT_OK,
        "message: {:?}",
        session.last_message()
    );
}

/// At half scale a Source image that does not cover the pixel window still fails, with the
/// bounds message, and releases what it fetched.
#[test]
fn source_smaller_than_the_window_fails_at_reduced_scale() {
    let session = ready_session();
    session.set_render_scale(0.5, 1.0);
    session.set_clip_image_with_rod(
        "Source",
        [0, 0, 8, 8],
        [0, 0, 32, 32],
        4,
        &vec![0.0; 8 * 8 * 4],
        false,
    );
    session.set_clip_image("Output", [0, 0, 32, 32], 4, &vec![0.0; 32 * 32 * 4], false);
    assert_eq!(session.render([0, 0, 32, 32], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with("Source image bounds")
            && text.contains("do not cover the requested region"),
        "message: {text}"
    );
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Quarter scale of the full-HD canonical frame: 480 x 270 pixels.
pub(super) const QUARTER_W: usize = 480;
/// Height in pixels of the quarter-scale test frame.
pub(super) const QUARTER_H: usize = 270;

/// The scene of the quarter-scale frame, cropped to `bounds` (global pixel coordinates). Outside
/// the frame the values are zero, so a host image larger than the frame carries zeros there.
pub(super) fn crop_quarter_scene(bounds: [c_int; 4]) -> Vec<f32> {
    let scene = scaled_scene(QUARTER_W, QUARTER_H);
    let (w, h) = (
        (bounds[2] - bounds[0]) as usize,
        (bounds[3] - bounds[1]) as usize,
    );
    let mut out = vec![0.0_f32; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let gx = bounds[0] as i64 + x as i64;
            let gy = bounds[1] as i64 + y as i64;
            if (0..QUARTER_W as i64).contains(&gx) && (0..QUARTER_H as i64).contains(&gy) {
                let src = (gy as usize * QUARTER_W + gx as usize) * 4;
                out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&scene[src..src + 4]);
            }
        }
    }
    out
}

/// A failed render leaves the output window zeroed, not whatever the host had there.
#[test]
fn failed_render_clears_the_output_window_at_quarter_scale() {
    let session = ready_session();
    session.set_render_scale(0.25, 1.0);
    let frame = [0, 0, QUARTER_W as c_int, QUARTER_H as c_int];
    // One pixel short in each direction: the window is not covered, so Render fails before the
    // renderer runs.
    session.set_clip_image_strided(
        "Source",
        [0, 0, QUARTER_W as c_int - 1, QUARTER_H as c_int - 1],
        frame,
        4,
        &crop_quarter_scene([0, 0, QUARTER_W as c_int - 1, QUARTER_H as c_int - 1]),
        false,
        2,
    );
    session.set_clip_image_strided(
        "Output",
        frame,
        frame,
        4,
        &vec![0.5; QUARTER_W * QUARTER_H * 4],
        false,
        6,
    );
    assert_eq!(session.render(frame, 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(text.contains("do not cover the requested region"), "{text}");
    assert!(text.contains("render scale 0.25 0.25"), "{text}");
    assert!(session.clip_logical("Output").iter().all(|&v| v == 0.0));
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Quarter scale of a 1920 x 1080 canonical frame: a full render and four uneven tiles, with
/// host images whose bounds are one pixel larger than the request (ceil and floor variants),
/// rows padded in memory, and alternating row order. Every output pixel must be written and
/// finite, and the stitched tiles must equal the full render.
#[test]
#[ignore = "needs a WGPU adapter"]
fn quarter_scale_full_hd_tiles_with_padded_rows_and_rounded_bounds() {
    let session = ready_session();
    session.set_render_scale(0.25, 1.0);
    let frame = [0, 0, QUARTER_W as c_int, QUARTER_H as c_int];
    let zeros = |w: c_int, h: c_int| vec![0.0_f32; (w * h * 4) as usize];

    session.set_clip_image_strided(
        "Source",
        frame,
        frame,
        4,
        &crop_quarter_scene(frame),
        false,
        5,
    );
    session.set_clip_image_strided("Output", frame, frame, 4, &zeros(480, 270), false, 3);
    assert_eq!(
        session.render(frame, 0.0),
        OFX_STAT_OK,
        "message: {:?}",
        session.last_message()
    );
    let full = session.clip_logical("Output");
    assert!(
        full.iter().all(|v| v.is_finite()),
        "full render writes every pixel"
    );

    let mut stitched = vec![f32::NAN; QUARTER_W * QUARTER_H * 4];
    // Tile windows in pixels; canonical RoIs are the same windows times four.
    let tiles: [[c_int; 4]; 4] = [
        [0, 0, 200, 135],
        [200, 0, 480, 135],
        [0, 135, 200, 270],
        [200, 135, 480, 270],
    ];
    for (n, window) in tiles.into_iter().enumerate() {
        let canonical = window.map(|v| f64::from(v) * 4.0);
        let (status, roi) = session.regions_of_interest(Some(canonical), 0.0);
        assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
        let request = roi_of(roi, "Source").map(|v| v / 4);
        // Bounds are one pixel larger than the request on each side.
        let bounds = [
            request[0] - 1,
            request[1] - 1,
            request[2] + 1,
            request[3] + 1,
        ];
        let negative = n % 2 == 1;
        session.set_clip_image_strided(
            "Source",
            bounds,
            frame,
            4,
            &crop_quarter_scene(bounds),
            negative,
            7,
        );
        let (w, h) = (window[2] - window[0], window[3] - window[1]);
        session.set_clip_image_strided("Output", window, window, 4, &zeros(w, h), !negative, 4);
        assert_eq!(
            session.render(window, 0.0),
            OFX_STAT_OK,
            "tile {n}: message: {:?}",
            session.last_message()
        );
        let tile = session.clip_logical("Output");
        assert!(
            tile.iter().all(|v| v.is_finite()),
            "tile {n} writes every pixel"
        );
        for y in window[1]..window[3] {
            for x in window[0]..window[2] {
                let src = ((y - window[1]) * w + (x - window[0])) as usize * 4;
                let dst = (y as usize * QUARTER_W + x as usize) * 4;
                stitched[dst..dst + 4].copy_from_slice(&tile[src..src + 4]);
            }
        }
    }
    assert_stitched_matches_full(&full, &stitched);
}
