//! Tile tests: a frame rendered in tiles, including uneven and edge-aligned tiles, must stitch back to the full-frame render.

use super::*;

/// Width in pixels of the tile test frame.
pub(super) const TILE_W: usize = 96;
/// Height in pixels of the tile test frame.
pub(super) const TILE_H: usize = 80;
/// The full frame, which is also the Source region of definition in these tests.
pub(super) const FULL: [c_int; 4] = [0, 0, TILE_W as c_int, TILE_H as c_int];

/// Colour scene in OFX order (row 0 bottom), premultiplied: a hard vertical edge at x = 50,
/// HDR dots (values up to 20) and a half-transparent block.
pub(super) fn tile_scene_colour() -> Vec<f32> {
    let dots = [(10, 10), (70, 20), (30, 60), (85, 70), (48, 40)];
    let mut out = vec![0.0_f32; TILE_W * TILE_H * 4];
    for (i, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % TILE_W, i / TILE_W);
        let straight = if x < 50 {
            [0.2, 0.5, 0.8]
        } else {
            [0.7, 0.3, 0.1]
        };
        let is_dot = dots
            .iter()
            .any(|&(cx, cy)| (x as i32 - cx).abs() <= 2 && (y as i32 - cy).abs() <= 2);
        let alpha = if is_dot {
            1.0
        } else if (60..70).contains(&x) && (40..50).contains(&y) {
            0.5
        } else {
            1.0
        };
        let straight = if is_dot { [20.0, 15.0, 8.0] } else { straight };
        for c in 0..3 {
            px[c] = straight[c] * alpha;
        }
        px[3] = alpha;
    }
    out
}

/// Depth scene in OFX order (RGBA, channel 0 is the depth): a step at x = 48 and a slope along
/// y. The focal plane used by the Depth tests is 10.
pub(super) fn tile_scene_depth() -> Vec<f32> {
    let mut out = vec![0.0_f32; TILE_W * TILE_H * 4];
    for (i, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % TILE_W, i / TILE_W);
        let depth = if x < 48 { 10.0 } else { 13.0 } + y as f32 * 0.05;
        px.copy_from_slice(&[depth, depth, depth, 1.0]);
    }
    out
}

/// The rows `roi` (x1, y1, x2, y2) of a frame `frame_width` pixels wide, in OFX order.
pub(super) fn crop_rows(logical: &[f32], frame_width: usize, roi: [c_int; 4]) -> Vec<f32> {
    let mut out = Vec::new();
    for y in roi[1]..roi[3] {
        for x in roi[0]..roi[2] {
            let i = (y as usize * frame_width + x as usize) * 4;
            out.extend_from_slice(&logical[i..i + 4]);
        }
    }
    out
}

/// The `RoI` the plugin set for `clip` in `GetRegionsOfInterest` out-args.
pub(super) fn roi_of(out: usize, clip: &str) -> [c_int; 4] {
    let name = CString::new(format!("OfxImageClipPropRoI_{clip}")).expect("no NUL");
    let mut values = [0; 4];
    for (i, value) in values.iter_mut().enumerate() {
        let read = prop_double(out, &name, i as c_int).expect("RoI is set");
        *value = read as c_int;
    }
    values
}

/// Renders each window (OFX coordinates) through the plugin, one Render call per window. Before
/// each call, `GetRegionsOfInterest` asks for the window's `RoI`, and Source and Depth then receive
/// only that region (bounds = `RoI`, region of definition = frame). The Output bounds are the
/// window, and the output is written in the requested layout. Returns the stitched output in
/// OFX order over the frame.
pub(super) fn render_windows(
    session: &Session,
    colour: &[f32],
    depth: Option<&[f32]>,
    windows: &[[c_int; 4]],
    negative_output: bool,
) -> Vec<f32> {
    // Full frames first: the RoI action reads the region of definition from the clip images.
    session.set_clip_image("Source", FULL, 4, colour, false);
    if let Some(depth) = depth {
        session.set_clip_image("Depth", FULL, 4, depth, false);
    }
    let mut stitched = vec![f32::NAN; TILE_W * TILE_H * 4];
    for &window in windows {
        let (status, roi) = session.regions_of_interest(Some(window.map(f64::from)), 0.0);
        assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
        let source_roi = roi_of(roi, "Source");
        session.set_clip_image_with_rod(
            "Source",
            source_roi,
            FULL,
            4,
            &crop_rows(colour, TILE_W, source_roi),
            false,
        );
        if let Some(depth) = depth {
            let depth_roi = roi_of(roi, "Depth");
            assert_eq!(depth_roi, source_roi, "Depth RoI follows the Source RoI");
            session.set_clip_image_with_rod(
                "Depth",
                depth_roi,
                FULL,
                4,
                &crop_rows(depth, TILE_W, depth_roi),
                false,
            );
        }
        let (w, h) = (
            (window[2] - window[0]) as usize,
            (window[3] - window[1]) as usize,
        );
        session.set_clip_image("Output", window, 4, &vec![0.0; w * h * 4], negative_output);
        assert_eq!(
            session.render(window, 0.0),
            OFX_STAT_OK,
            "message: {:?}",
            session.last_message()
        );
        let tile = session.clip_logical("Output");
        for y in window[1]..window[3] {
            for x in window[0]..window[2] {
                let src = ((y - window[1]) as usize * w + (x - window[0]) as usize) * 4;
                let dst = (y as usize * TILE_W + x as usize) * 4;
                stitched[dst..dst + 4].copy_from_slice(&tile[src..src + 4]);
            }
        }
    }
    stitched
}

/// Finite values and alpha in [0, 1]. The tile scene has HDR dots, so colour may exceed alpha
/// (HDR light spreads into neighbouring pixels); the strict premultiplied check does not apply.
pub(super) fn assert_finite_unit_alpha(pixels: &[f32]) {
    for (i, px) in pixels.as_chunks::<4>().0.iter().enumerate() {
        assert!(px.iter().all(|v| v.is_finite()), "pixel {i} is not finite");
        assert!((0.0..=1.0).contains(&px[3]), "pixel {i} alpha {}", px[3]);
    }
}

/// Finite values, and alpha within the premultiplied check's tolerance of [0, 1]. The renderer
/// rounds the alpha of some HDR pixels a few ulp past 1 (1.0000001 was measured), which is not
/// an error. The tolerance is the same 1e-6 as [`assert_premultiplied`].
pub(super) fn assert_finite_alpha_within_eps(pixels: &[f32]) {
    const EPS: f32 = 1e-6;
    for (i, px) in pixels.as_chunks::<4>().0.iter().enumerate() {
        assert!(px.iter().all(|v| v.is_finite()), "pixel {i} is not finite");
        assert!(
            (-EPS..=1.0 + EPS).contains(&px[3]),
            "pixel {i} alpha {}",
            px[3]
        );
    }
}

/// Asserts that every pixel was written, the output is finite with unit alpha, and the stitched tiles
/// match the full-frame render within 1e-5, the renderer's own ROI/block gate (`hardware_roi_and_block_renders_match_full_frame`). Measured 3.3e-6 to 3.8e-6 (4-7 ulp on values of 5-10), identical to a direct `render_stripe` call without the OFX layer, so the residual is renderer partition rounding, not plugin error.
pub(super) fn assert_stitched_matches_full(full: &[f32], stitched: &[f32]) {
    assert!(
        stitched.iter().all(|v| v.is_finite()),
        "a pixel was not written by any tile"
    );
    assert_finite_unit_alpha(stitched);
    let (worst, max_diff) = full
        .iter()
        .zip(stitched)
        .map(|(f, t)| (f - t).abs())
        .enumerate()
        .fold(
            (0, 0.0_f32),
            |best, (i, d)| if d > best.1 { (i, d) } else { best },
        );
    let (pixel, channel) = (worst / 4, worst % 4);
    eprintln!(
        "stitched max abs diff {max_diff:e} at x={} y={} channel {channel}: full {} stitched {}",
        pixel % TILE_W,
        pixel / TILE_W,
        full[worst],
        stitched[worst]
    );
    assert!(max_diff <= 1e-5, "stitched max abs diff {max_diff}");
}

/// Splits the frame at the given interior x and y cuts into a grid of windows.
pub(super) fn grid(xs: &[c_int], ys: &[c_int]) -> Vec<[c_int; 4]> {
    let mut windows = Vec::new();
    for y in ys.windows(2) {
        for x in xs.windows(2) {
            windows.push([x[0], y[0], x[1], y[1]]);
        }
    }
    windows
}

/// An uneven 2x2 split: x cut at 40, y cut at 30. Every tile touches the frame edge.
pub(super) fn uneven_2x2() -> Vec<[c_int; 4]> {
    grid(&[0, 40, TILE_W as c_int], &[0, 30, TILE_H as c_int])
}

/// Renders the frame whole and in `windows`, then compares. `depth_mode` selects Depth mode
/// with the step scene; `negative_output` writes the tiles with negative rowBytes.
pub(super) fn tiles_match_full_frame(
    windows: &[[c_int; 4]],
    depth_mode: bool,
    negative_output: bool,
) {
    let session = ready_session();
    if depth_mode {
        session.set_param_value("defocusMode", Value::Int(1));
        session.set_param_value("focalPlane", Value::Double(FOCAL));
    }
    let colour = tile_scene_colour();
    let depth = tile_scene_depth();
    let depth = depth_mode.then_some(depth.as_slice());
    print_adapter();
    let full = render_windows(&session, &colour, depth, &[FULL], false);
    assert_finite_unit_alpha(&full);
    let stitched = render_windows(&session, &colour, depth, windows, negative_output);
    assert_stitched_matches_full(&full, &stitched);
}

/// Checks on the hardware adapter that 2D tiles match the full-frame render.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_tiles_2d_match_full_frame() {
    tiles_match_full_frame(&uneven_2x2(), false, false);
}

/// Checks on the hardware adapter that 2D tiles match the full frame when rows are stored with negative row bytes.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_tiles_2d_match_full_frame_with_negative_rows() {
    tiles_match_full_frame(&uneven_2x2(), false, true);
}

/// Checks on the hardware adapter that depth-driven tiles match the full-frame render.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_tiles_depth_match_full_frame() {
    tiles_match_full_frame(&uneven_2x2(), true, false);
}

/// Checks on the hardware adapter that depth tiles match the full frame with negative row bytes.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_tiles_depth_match_full_frame_with_negative_rows() {
    tiles_match_full_frame(&uneven_2x2(), true, true);
}

/// Checks on the hardware adapter that tiles at the frame edges and interior all match the full frame.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_tiles_at_frame_edges_and_interior_match_full_frame() {
    // Uneven 3x3 grid: the centre tile touches no frame edge, the others touch one or two.
    let windows = grid(&[0, 30, 62, TILE_W as c_int], &[0, 25, 55, TILE_H as c_int]);
    let session = ready_session();
    let colour = tile_scene_colour();
    // The RoI each tile gets must be the window grown by the renderer's padding, clamped to the
    // frame. Padding comes from a renderer built with the same default settings as the plugin.
    let mut settings = default_settings();
    let renderer =
        pollster::block_on(opendefocus::OpenDefocusRenderer::new(&mut settings)).expect("renderer");
    let padding = renderer.padding(&settings) as c_int;
    let (status, out) = session.regions_of_interest(Some(FULL.map(f64::from)), 0.0);
    assert_eq!(status, OFX_STAT_FAILED, "no Source image is set yet");
    assert_eq!(prop_count(out, c"OfxImageClipPropRoI_Source"), 0);
    session.set_clip_image("Source", FULL, 4, &colour, false);
    for window in &windows {
        let (status, out) = session.regions_of_interest(Some(window.map(f64::from)), 0.0);
        assert_eq!(status, OFX_STAT_OK, "message: {:?}", session.last_message());
        let expected = [
            (window[0] - padding).max(FULL[0]),
            (window[1] - padding).max(FULL[1]),
            (window[2] + padding).min(FULL[2]),
            (window[3] + padding).min(FULL[3]),
        ];
        assert_eq!(roi_of(out, "Source"), expected, "window {window:?}");
    }
    let full = render_windows(&session, &colour, None, &[FULL], false);
    let stitched = render_windows(&session, &colour, None, &windows, false);
    assert_stitched_matches_full(&full, &stitched);
}
