//! Render tests through the mock host on the hardware adapter: point sources, depth planes and offset regions are compared against direct reference renders.
//! They need a WGPU adapter and run with `cargo test -- --ignored`.

use super::*;

/// Side length in pixels of the square test frame used by the hardware render tests.
pub(super) const FRAME: usize = 64;

/// Prints the adapter the renderer selects, so the hardware test output names the device.
pub(super) fn print_adapter() {
    let mut settings = opendefocus::datamodel::Settings::default();
    match pollster::block_on(opendefocus::OpenDefocusRenderer::new(&mut settings)) {
        Ok(_) => eprintln!(
            "adapter: {}",
            settings.render.device_name.as_deref().unwrap_or("unknown")
        ),
        Err(error) => eprintln!("adapter: unavailable ({error})"),
    }
}

/// Premultiplied RGBA check: finite, alpha in [0, 1], colour in [0, alpha].
pub(super) fn assert_premultiplied(pixels: &[f32]) {
    const EPS: f32 = 1e-6;
    for (i, px) in pixels.as_chunks::<4>().0.iter().enumerate() {
        assert!(px.iter().all(|v| v.is_finite()), "pixel {i} is not finite");
        let alpha = px[3];
        assert!(
            (-EPS..=1.0 + EPS).contains(&alpha),
            "pixel {i} alpha {alpha}"
        );
        for c in &px[..3] {
            assert!(
                (-EPS..=alpha + EPS).contains(c),
                "pixel {i} colour {c} exceeds alpha {alpha}"
            );
        }
    }
}

/// Focal plane used by the Depth-mode renders, both through the plugin and directly.
pub(super) const FOCAL: f64 = 10.0;

/// Renders `logical` (OFX bottom-up rows) with the renderer called directly, without the
/// plugin. The plugin output must match this exactly, which proves the row flip and layout.
/// With `depth` (OFX bottom-up, one value per pixel) the direct call uses Depth mode with the
/// same focal plane, and the depth is flipped to top-down like the colour.
pub(super) fn direct_reference(logical: &[f32], size: f32, depth: Option<&[f32]>) -> Vec<f32> {
    let mut settings = opendefocus::datamodel::Settings::default();
    settings.defocus.circle_of_confusion.size = size;
    settings.render.gui = false;
    if depth.is_some() {
        settings.defocus.defocus_mode = 1;
        settings.defocus.circle_of_confusion.focal_plane = FOCAL as f32;
    }
    let renderer =
        pollster::block_on(opendefocus::OpenDefocusRenderer::new(&mut settings)).expect("renderer");
    let mut image = ndarray::Array3::<f32>::zeros((FRAME, FRAME, 4));
    for y in 0..FRAME {
        for x in 0..FRAME {
            for c in 0..4 {
                image[[FRAME - 1 - y, x, c]] = logical[(y * FRAME + x) * 4 + c];
            }
        }
    }
    let depth_top = depth.map(|values| {
        let mut top = ndarray::Array2::<f32>::zeros((FRAME, FRAME));
        for y in 0..FRAME {
            for x in 0..FRAME {
                top[[FRAME - 1 - y, x]] = values[y * FRAME + x];
            }
        }
        top
    });
    pollster::block_on(renderer.render(settings, image.view_mut(), depth_top, None))
        .expect("direct render");
    let mut out = vec![0.0_f32; FRAME * FRAME * 4];
    for y in 0..FRAME {
        for x in 0..FRAME {
            for c in 0..4 {
                out[(y * FRAME + x) * 4 + c] = image[[FRAME - 1 - y, x, c]];
            }
        }
    }
    out
}

/// Renders one bright pixel placed at OFX (x, y) = (20, 11), with the Output in either layout.
/// The position is asymmetric, so a transpose or a vertical mirror moves the centroid and fails.
pub(super) fn point_render(negative_output: bool) {
    const X0: usize = 20;
    const Y0: usize = 11;
    let session = ready_session();
    session.set_param_value("size", Value::Double(3.0));
    let mut source = vec![0.0_f32; FRAME * FRAME * 4];
    let at = (Y0 * FRAME + X0) * 4;
    source[at..at + 4].copy_from_slice(&[1.0; 4]);
    session.set_clip_image(
        "Source",
        [0, 0, FRAME as c_int, FRAME as c_int],
        4,
        &source,
        false,
    );
    session.set_clip_image(
        "Output",
        [0, 0, FRAME as c_int, FRAME as c_int],
        4,
        &vec![0.0; FRAME * FRAME * 4],
        negative_output,
    );
    print_adapter();
    assert_eq!(
        session.render([0, 0, FRAME as c_int, FRAME as c_int], 0.0),
        OFX_STAT_OK,
        "message: {:?}",
        session.last_message()
    );
    // 2D mode fetches Source and Output only; Depth is ignored.
    assert_eq!(session.images_fetched(), 2);
    assert_eq!(session.images_released(), 2);

    let output = session.clip_logical("Output");
    assert_premultiplied(&output);
    let mut reference_source = vec![0.0_f32; FRAME * FRAME * 4];
    reference_source[(Y0 * FRAME + X0) * 4..][..4].copy_from_slice(&[1.0; 4]);
    let reference = direct_reference(&reference_source, 3.0, None);
    let max_diff = output
        .iter()
        .zip(&reference)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_diff <= 1e-6,
        "plugin differs from direct render by {max_diff}"
    );
    let mut energy = 0.0_f64;
    let (mut cx, mut cy) = (0.0_f64, 0.0_f64);
    for (i, px) in output.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (i % FRAME, i / FRAME);
        let w = f64::from(px[0]);
        energy += w;
        cx += w * x as f64;
        cy += w * y as f64;
    }
    assert!((energy - 1.0).abs() < 1e-3, "energy {energy}");
    let (cx, cy) = (cx / energy, cy / energy);
    // The ~0.02 px offset at size 3 is renderer sample bias, not a plugin error: a direct
    // OpenDefocusRenderer call gives the same centroid (20.020 for a dot at 20). The direct
    // comparison above is the exact flip proof, so this bound only guards against a moved dot.
    assert!((cx - X0 as f64).abs() <= 0.1, "centroid x {cx}");
    assert!(
        (cy - Y0 as f64).abs() <= 0.1,
        "centroid y {cy} (OFX row, bottom-up)"
    );
}

/// Checks on the hardware adapter that a point source stays in place with positive row bytes.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_2d_point_stays_in_place_with_positive_rows() {
    point_render(false);
}

/// Checks on the hardware adapter that a point source stays in place with negative row bytes.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_2d_point_stays_in_place_with_negative_rows() {
    point_render(true);
}

/// A premultiplied RGBA gradient with varying alpha, so colour and alpha both matter.
pub(super) fn gradient_source() -> Vec<f32> {
    let mut source = vec![0.0_f32; FRAME * FRAME * 4];
    for (i, px) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % FRAME, i / FRAME);
        let alpha = 0.25 + 0.75 * (((x + y) % 7) as f32) / 6.0;
        px[0] = x as f32 / FRAME as f32 * alpha;
        px[1] = y as f32 / FRAME as f32 * alpha;
        px[2] = 0.3 * alpha;
        px[3] = alpha;
    }
    source
}

/// Places `inner` (FRAME x FRAME RGBA) at (`ox`, `oy`) in a `width` x `height` RGBA buffer
/// filled with `fill`. Coordinates are in buffer order (row 0 first).
pub(super) fn embed(
    inner: &[f32],
    width: usize,
    height: usize,
    ox: usize,
    oy: usize,
    fill: [f32; 4],
) -> Vec<f32> {
    let mut out: Vec<f32> = fill
        .iter()
        .copied()
        .cycle()
        .take(width * height * 4)
        .collect();
    for y in 0..FRAME {
        for x in 0..FRAME {
            let src = (y * FRAME + x) * 4;
            let dst = ((oy + y) * width + ox + x) * 4;
            out[dst..dst + 4].copy_from_slice(&inner[src..src + 4]);
        }
    }
    out
}

/// Checks on the hardware adapter that a depth plane at focus is left unchanged.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_depth_plane_in_focus_is_unchanged() {
    let session = ready_session();
    session.set_param_value("defocusMode", Value::Int(1));
    session.set_param_value("focalPlane", Value::Double(FOCAL));
    let source = gradient_source();
    let depth = vec![10.0_f32; FRAME * FRAME * 4];
    session.set_clip_image(
        "Source",
        [0, 0, FRAME as c_int, FRAME as c_int],
        4,
        &source,
        false,
    );
    session.set_clip_image(
        "Depth",
        [0, 0, FRAME as c_int, FRAME as c_int],
        4,
        &depth,
        false,
    );
    session.set_clip_image(
        "Output",
        [0, 0, FRAME as c_int, FRAME as c_int],
        4,
        &vec![0.0; FRAME * FRAME * 4],
        false,
    );
    print_adapter();
    assert_eq!(
        session.render([0, 0, FRAME as c_int, FRAME as c_int], 0.0),
        OFX_STAT_OK,
        "message: {:?}",
        session.last_message()
    );
    let output = session.clip_logical("Output");
    assert_premultiplied(&output);
    let max_diff = output
        .iter()
        .zip(&source)
        .map(|(o, s)| (o - s).abs())
        .fold(0.0_f32, f32::max);
    assert!(max_diff <= 1e-6, "max abs diff {max_diff}");
}

/// Checks on the hardware adapter that an offset region of definition renders like the direct render.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_offset_region_of_definition_matches_direct_render() {
    // The Source region of definition and the render window are the same rectangle, offset
    // from the origin. The Source bounds pad it by 4 px and the Output bounds by 3 px, so
    // both buffers are larger than the window, and the window does not start at their origin.
    // temporary restriction until tiles (task 3b); assumes render scale 1 (supportsMultiResolution = 0)
    const WINDOW: [c_int; 4] = [8, 5, 72, 69];
    const SOURCE_BOUNDS: [c_int; 4] = [4, 1, 76, 73];
    const OUTPUT_BOUNDS: [c_int; 4] = [5, 2, 75, 72];
    const SENTINEL: [f32; 4] = [0.1, 0.2, 0.3, 0.5];

    let session = ready_session();
    session.set_param_value("size", Value::Double(3.0));
    let mut window_logical = vec![0.0_f32; FRAME * FRAME * 4];
    for (x, y, px) in [
        (20, 11, [1.0, 1.0, 1.0, 1.0]),
        (50, 40, [0.5, 0.25, 0.0, 0.5]),
    ] {
        window_logical[(y * FRAME + x) * 4..][..4].copy_from_slice(&px);
    }
    let width = |b: [c_int; 4]| (b[2] - b[0]) as usize;
    let height = |b: [c_int; 4]| (b[3] - b[1]) as usize;
    // Window origin inside each buffer: (8, 5) - (4, 1) for Source, (8, 5) - (5, 2) for Output.
    let source = embed(
        &window_logical,
        width(SOURCE_BOUNDS),
        height(SOURCE_BOUNDS),
        (WINDOW[0] - SOURCE_BOUNDS[0]) as usize,
        (WINDOW[1] - SOURCE_BOUNDS[1]) as usize,
        [0.0; 4],
    );
    session.set_clip_image_with_rod("Source", SOURCE_BOUNDS, WINDOW, 4, &source, false);
    let output_pixels: Vec<f32> = SENTINEL
        .iter()
        .copied()
        .cycle()
        .take(width(OUTPUT_BOUNDS) * height(OUTPUT_BOUNDS) * 4)
        .collect();
    session.set_clip_image("Output", OUTPUT_BOUNDS, 4, &output_pixels, true);
    print_adapter();
    assert_eq!(
        session.render(WINDOW, 0.0),
        OFX_STAT_OK,
        "message: {:?}",
        session.last_message()
    );
    assert_eq!(session.images_fetched(), 2);
    assert_eq!(session.images_released(), 2);

    let output = session.clip_logical("Output");
    let out_width = width(OUTPUT_BOUNDS);
    let (ox, oy) = (
        (WINDOW[0] - OUTPUT_BOUNDS[0]) as usize,
        (WINDOW[1] - OUTPUT_BOUNDS[1]) as usize,
    );
    let mut window_out = Vec::with_capacity(FRAME * FRAME * 4);
    for y in 0..FRAME {
        for x in 0..FRAME {
            let at = ((oy + y) * out_width + ox + x) * 4;
            window_out.extend_from_slice(&output[at..at + 4]);
        }
    }
    assert_premultiplied(&window_out);

    let reference = direct_reference(&window_logical, 3.0, None);
    let max_diff = window_out
        .iter()
        .zip(&reference)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_diff <= 1e-6,
        "offset-RoD output differs from direct render by {max_diff}"
    );
    // Pixels outside the render window are not written.
    for (i, px) in output.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (i % out_width, i / out_width);
        let inside = (ox..ox + FRAME).contains(&x) && (oy..oy + FRAME).contains(&y);
        if !inside {
            assert_eq!(
                px,
                &SENTINEL[..],
                "pixel ({x}, {y}) outside the window changed"
            );
        }
    }
}

/// Checks on the hardware adapter that a depth step matches the direct render in both row layouts.
#[test]
#[ignore = "needs a WGPU adapter"]
fn hardware_depth_step_matches_direct_render_in_both_layouts() {
    // A row gradient from 8 to 12 with an asymmetric +1.5 step (x >= 40, bottom rows), so the
    // depth flip and the step location both matter. Focal plane 10 keeps some blur.
    let mut depth = vec![0.0_f32; FRAME * FRAME];
    for (i, d) in depth.iter_mut().enumerate() {
        let (x, y) = (i % FRAME, i / FRAME);
        *d = 8.0 + 4.0 * y as f32 / (FRAME - 1) as f32 + if x >= 40 && y < 20 { 1.5 } else { 0.0 };
    }
    let depth_rgba: Vec<f32> = depth.iter().flat_map(|&d| [d, d, d, 1.0]).collect();
    let source = gradient_source();
    let reference = direct_reference(&source, 3.0, Some(&depth));

    for negative in [false, true] {
        let session = ready_session();
        session.set_param_value("defocusMode", Value::Int(1));
        session.set_param_value("focalPlane", Value::Double(FOCAL));
        session.set_param_value("size", Value::Double(3.0));
        let full = [0, 0, FRAME as c_int, FRAME as c_int];
        session.set_clip_image("Source", full, 4, &source, negative);
        session.set_clip_image("Depth", full, 4, &depth_rgba.clone(), negative);
        session.set_clip_image("Output", full, 4, &vec![0.0; FRAME * FRAME * 4], negative);
        assert_eq!(
            session.render(full, 0.0),
            OFX_STAT_OK,
            "negative rows {negative}, message: {:?}",
            session.last_message()
        );
        assert_eq!(session.images_fetched(), 3);
        assert_eq!(session.images_released(), 3);
        let output = session.clip_logical("Output");
        assert_premultiplied(&output);
        let max_diff = output
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            max_diff <= 1e-6,
            "negative rows {negative}: depth render differs from direct render by {max_diff}"
        );
    }
}
