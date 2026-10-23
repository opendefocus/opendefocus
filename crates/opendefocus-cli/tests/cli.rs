//! End-to-end tests that run the `opendefocus-cli` binary: output formats, settings
//! overlays, validation errors, GPU/library parity, and interrupt handling.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly with a panic; only #[test] functions are exempt from these lints by clippy.toml"
)]

use std::{
    path::Path,
    process::{Command, Output},
};

use image::{Rgba, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
#[cfg(feature = "gpu")]
use opendefocus::{OpenDefocusRenderer, datamodel::Settings};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Runs the built `opendefocus-cli` binary with `args` and returns its captured output.
fn cli(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_opendefocus-cli"));
    command.args(args);
    command.output().unwrap()
}
/// Returns `path` as text, panicking if it is not valid UTF-8.
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}
/// Asserts that the command succeeded and parses its stdout as JSON.
fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
/// Asserts that the command fails and that its stderr contains `message`.
fn fails(args: &[&str], message: &str) {
    let output = cli(args);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
/// Writes a 19x21 gradient test image to `path` and returns the same pixels.
fn source(path: &Path) -> Rgba32FImage {
    let image = Rgba32FImage::from_fn(19, 21, |x, y| {
        Rgba([
            (x % 5) as f32 * 0.7,
            (y % 7) as f32 * 0.4,
            ((x + y) % 9) as f32 * 0.3,
            0.25 + (x % 4) as f32 * 0.2,
        ])
    });
    image.save(path).unwrap();
    image
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that GPU full-frame output matches the library, keeps HDR and alpha, and reports two repeats.
fn test_full_matches_library_preserves_hdr_alpha_and_reports_repeats() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    let output = dir.path().join("output.exr");
    let report_path = dir.path().join("report.json");
    let original = source(&input);
    let report = success(&cli(&[
        path(&input),
        "-o",
        path(&output),
        "--radius",
        "2",
        "--quality",
        "accurate",
        "--repeat",
        "2",
        "--report",
        path(&report_path),
    ]));
    assert_eq!(report["backend"], "gpu");
    assert_eq!(report["algorithm"], "adaptive");
    assert_eq!(report["rendered_algorithm"], "adaptive");
    assert_eq!(report["classifier_diagnostic"], false);
    assert_eq!(report["frames"].as_array().unwrap().len(), 2);
    assert_eq!(
        report,
        serde_json::from_slice::<Value>(&std::fs::read(report_path).unwrap()).unwrap()
    );
    for frame in report["frames"].as_array().unwrap() {
        assert!(
            frame["frame_ms"].as_f64().unwrap() >= frame["renderer_calls_ms"].as_f64().unwrap()
        );
    }
    assert!(
        report["end_to_end_ms"].as_f64().unwrap() >= report["initialization_ms"].as_f64().unwrap()
    );
    let actual = image::open(output).unwrap().to_rgba32f();
    let mut expected = original.to_ndarray();
    let mut settings: Settings = serde_json::from_value(report["settings"].clone()).unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        renderer
            .render(settings, expected.view_mut(), None, None)
            .await
            .unwrap();
    });
    assert!(actual.pixels().any(|pixel| pixel[0] > 1.0));
    assert!(actual.pixels().any(|pixel| pixel[3] < 0.9));
    for (a, b) in actual.as_raw().iter().zip(expected.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that striped output matches full-frame output across band edges with lens effects enabled.
fn test_stripes_match_full_across_edges_with_lens_effects() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    let full = dir.path().join("full.exr");
    let striped = dir.path().join("striped.exr");
    let config = dir.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"render":{"center":{"x":9.5,"y":10.5}},
        "non_uniform":{"catseye":{"enable":true,"amount":0.4}}})
        .to_string(),
    )
    .unwrap();
    success(&cli(&[
        path(&input),
        "-o",
        path(&full),
        "--radius",
        "3",
        "--settings",
        path(&config),
    ]));
    let report = success(&cli(&[
        path(&input),
        "-o",
        path(&striped),
        "--radius",
        "3",
        "--settings",
        path(&config),
        "--render-mode",
        "striped",
        "--stripe-height",
        "4",
    ]));
    assert_eq!(report["stripe_height"], 4);
    let full = image::open(full).unwrap().to_rgba32f();
    let striped = image::open(striped).unwrap().to_rgba32f();
    for (a, b) in full.as_raw().iter().zip(striped.as_raw()) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that only the depth red channel is used, a custom bokeh applies, and float TIFF output matches the library.
fn test_depth_red_channel_custom_bokeh_and_float_tiff() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    let depth = dir.path().join("depth.exr");
    // Different green/blue values detect unwanted luminance conversion.
    Rgba32FImage::from_fn(19, 21, |_, _| Rgba([2.0, 8.0, 20.0, 1.0]))
        .save(&depth)
        .unwrap();
    let filter = dir.path().join("filter.png");
    image::RgbaImage::from_fn(8, 8, |x, y| {
        Rgba([
            if (2..6).contains(&x) && (2..6).contains(&y) {
                255
            } else {
                0
            },
            255,
            255,
            255,
        ])
    })
    .save(&filter)
    .unwrap();
    let config = dir.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"defocus":{"use_direct_math":true}}).to_string(),
    )
    .unwrap();
    let output = dir.path().join("output.tiff");
    let report = success(&cli(&[
        path(&input),
        "-o",
        path(&output),
        "--depth",
        path(&depth),
        "--bokeh",
        path(&filter),
        "--settings",
        path(&config),
        "--quality",
        "accurate",
    ]));
    assert_eq!(report["settings"]["render"]["farm_quality"], 1);
    let actual = image::open(output).unwrap().to_rgba32f();
    let mut expected = image::open(input).unwrap().to_rgba32f().to_ndarray();
    let filter = image::open(filter).unwrap().to_rgba32f().to_ndarray();
    let mut settings: Settings = serde_json::from_value(report["settings"].clone()).unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        renderer
            .render(
                settings,
                expected.view_mut(),
                Some(ndarray::Array2::from_elem((21, 19), 2.0)),
                Some(filter),
            )
            .await
            .unwrap();
    });
    for (a, b) in actual.as_raw().iter().zip(expected.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
    let striped_path = dir.path().join("striped.exr");
    success(&cli(&[
        path(&dir.path().join("input.exr")),
        "-o",
        path(&striped_path),
        "--depth",
        path(&depth),
        "--bokeh",
        path(&dir.path().join("filter.png")),
        "--settings",
        path(&config),
        "--quality",
        "accurate",
        "--render-mode",
        "striped",
        "--stripe-height",
        "4",
    ]));
    let striped = image::open(striped_path).unwrap().to_rgba32f();
    for (a, b) in actual.as_raw().iter().zip(striped.as_raw()) {
        assert!((a - b).abs() < 1e-5);
    }
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that PNG, JPEG and HDR outputs have the expected size and color type.
fn test_ldr_and_hdr_output_formats() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    for extension in ["png", "jpg", "hdr"] {
        let output = dir.path().join(format!("output.{extension}"));
        success(&cli(&[path(&input), "-o", path(&output), "--radius", "2"]));
        let image = image::open(output).unwrap();
        assert_eq!((image.width(), image.height()), (19, 21));
        match extension {
            "png" => assert_eq!(image.color(), image::ColorType::Rgba16),
            "hdr" => assert!(image.to_rgb32f().pixels().any(|p| p[0] > 1.0)),
            _ => assert_eq!(image.color(), image::ColorType::Rgb8),
        }
    }
}

#[test]
#[cfg(feature = "gpu")]
/// Checks that a settings overlay is merged, unknown keys and invalid arguments are rejected, and a missing depth map fails.
fn test_settings_defaults_camera_overlay_and_errors() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"defocus":{"circle_of_confusion":{"camera_data":{"f_stop":2.8}}}}).to_string(),
    )
    .unwrap();
    let settings = success(&cli(&["--dump-settings", "--settings", path(&config)]));
    assert!(
        (settings["defocus"]["circle_of_confusion"]["camera_data"]["f_stop"]
            .as_f64()
            .unwrap()
            - 2.8)
            .abs()
            < 1e-6
    );
    assert_eq!(settings["render"]["farm_quality"], 1);
    std::fs::write(
        &config,
        r#"{"defocus":{"circle_of_confusion":{"camera_data":{"f_stpo":2.8}}}}"#,
    )
    .unwrap();
    fails(
        &["--dump-settings", "--settings", path(&config)],
        "Unknown setting",
    );
    fails(
        &["--dump-settings", "--radius", "NaN"],
        "radius must be finite",
    );
    fails(&["--dump-settings", "--quality", "medium"], "invalid value");
    fails(
        &["--dump-settings", "--samples", "10"],
        "unexpected argument",
    );
    fails(
        &["--dump-settings", "--algorithm", "legacy"],
        "unexpected argument",
    );
    fails(
        &["--dump-settings", "--stripe-height", "0"],
        "Stripe height must be positive",
    );
    fails(&["missing.exr", "-o", "result.exr"], "Loading missing.exr");
    fails(
        &["missing.exr", "-o", "result.bmp"],
        "Unsupported output format",
    );
    let input = dir.path().join("input.exr");
    source(&input);
    let depth = dir.path().join("depth.exr");
    Rgba32FImage::new(2, 2).save(&depth).unwrap();
    let output = dir.path().join("output.exr");
    fails(
        &[path(&input), "-o", path(&output), "--depth", path(&depth)],
        "same dimensions",
    );
    assert!(!output.exists());
    std::fs::write(&config, r#"{"defocus":{"defocus_mode":1}}"#).unwrap();
    fails(
        &[
            path(&input),
            "-o",
            path(&output),
            "--settings",
            path(&config),
        ],
        "Depth mode requires --depth",
    );
}

#[cfg(not(feature = "gpu"))]
#[test]
/// Checks that rendering in a build without the `gpu` feature reports GPU unavailable.
fn test_gpu_request_without_feature_is_an_error() {
    fails(&["missing.exr", "-o", "output.exr"], "GPU unavailable");
}

#[cfg(not(feature = "gpu"))]
#[test]
/// Checks that `--dump-settings` works without GPU support and still rejects a NaN radius.
fn test_settings_can_be_inspected_without_wgpu() {
    let settings = success(&cli(&["--dump-settings", "--radius", "12"]));
    assert_eq!(settings["defocus"]["circle_of_confusion"]["size"], 12.0);
    fails(
        &["--dump-settings", "--radius", "NaN"],
        "radius must be finite",
    );
}

#[cfg(not(feature = "gpu"))]
#[test]
/// Checks that rendering without GPU support fails with a clear message and writes no output.
fn test_scene_render_without_wgpu_reports_adaptive_requirement() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    let output = dir.path().join("output.exr");
    fails(
        &[path(&input), "-o", path(&output)],
        "GPU unavailable: CLI was built without the gpu feature",
    );
    assert!(!output.exists());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that CLI GPU output matches the library for full-frame and striped renders, band by band.
fn test_cli_and_library_use_the_same_wgpu_renderer() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    let output = dir.path().join("output.exr");
    let report = success(&cli(&[path(&input), "-o", path(&output), "--radius", "2"]));
    assert_eq!(report["backend"], "gpu");
    let gpu = image::open(&output).unwrap().to_rgba32f();
    let mut expected = image::open(&input).unwrap().to_rgba32f().to_ndarray();
    let mut settings: Settings = serde_json::from_value(report["settings"].clone()).unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        renderer
            .render(settings, expected.view_mut(), None, None)
            .await
            .unwrap();
    });
    for (a, b) in gpu.as_raw().iter().zip(expected.iter()) {
        assert!(
            (a - b).abs() < 1e-6,
            "CLI GPU {a} differs from library GPU {b}"
        );
    }
    let striped_path = dir.path().join("striped.exr");
    let report = success(&cli(&[
        path(&input),
        "-o",
        path(&striped_path),
        "--radius",
        "2",
        "--render-mode",
        "striped",
        "--stripe-height",
        "4",
    ]));
    assert_eq!(report["backend"], "gpu");
    let striped = image::open(striped_path).unwrap().to_rgba32f();
    let max_difference = gpu
        .as_raw()
        .iter()
        .zip(striped.as_raw())
        .fold(0.0_f32, |max, (a, b)| max.max((a - b).abs()));
    println!("Current renderer GPU full/striped max absolute difference: {max_difference}");
    let source = image::open(&input).unwrap().to_rgba32f().to_ndarray();
    let mut settings: Settings = serde_json::from_value(report["settings"].clone()).unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        // Independent reference requests cover top edge, interior and final partial
        // band in absolute image coordinates. GPU full/stripe equivalence is not
        // guaranteed by the current renderer; each CLI band must match its API.
        for (y, end, top, bottom) in [(0, 4, 0, 6), (4, 8, 2, 10), (20, 21, 18, 21)] {
            let mut band = source.slice(ndarray::s![top..bottom, .., ..]).to_owned();
            renderer
                .render_stripe(
                    opendefocus::datamodel::render::RenderSpecs {
                        full_region: opendefocus::datamodel::IVector4 {
                            x: 0,
                            y: top,
                            z: 19,
                            w: bottom,
                        },
                        render_region: opendefocus::datamodel::IVector4 {
                            x: 0,
                            y,
                            z: 19,
                            w: end,
                        },
                    },
                    settings.clone(),
                    band.view_mut(),
                    None,
                    None,
                )
                .await
                .unwrap();
            for row in y..end {
                for x in 0..19 {
                    for c in 0..4 {
                        let actual = striped.get_pixel(x as u32, row as u32)[c];
                        let expected = band[[(row - top) as usize, x as usize, c]];
                        assert!(
                            (actual - expected).abs() < 1e-6,
                            "CLI stripe {actual} != library stripe {expected}"
                        );
                    }
                }
            }
        }
    });
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that a physical camera with a generated bokeh renders finite output, and that disabling noise changes it.
fn test_physical_camera_and_generated_bokeh_render() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    let depth = dir.path().join("depth.exr");
    Rgba32FImage::from_fn(19, 21, |x, _| Rgba([3.0 + x as f32, 0.0, 0.0, 1.0]))
        .save(&depth)
        .unwrap();
    let config = dir.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"defocus":{"camera_max_size":3.0,
        "circle_of_confusion":{"focal_plane":5.0,"camera_data":{"f_stop":2.8,"world_unit":3}}},
        "render":{"filter":{"mode":1,"resolution":32}},"bokeh":{"filter_type":1,"blades":6,"aspect_ratio":0.0}})
        .to_string(),
    )
    .unwrap();
    let output = dir.path().join("output.exr");
    success(&cli(&[
        path(&input),
        "-o",
        path(&output),
        "--depth",
        path(&depth),
        "--settings",
        path(&config),
    ]));
    let noisy = image::open(&output).unwrap().to_rgba32f();
    assert!(noisy.as_raw().iter().all(|v| v.is_finite()));
    let mut settings: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    settings["bokeh"]["noise"] = json!({"intensity": 0.0});
    std::fs::write(&config, settings.to_string()).unwrap();
    success(&cli(&[
        path(&input),
        "-o",
        path(&output),
        "--depth",
        path(&depth),
        "--settings",
        path(&config),
    ]));
    let clean = image::open(output).unwrap().to_rgba32f();
    assert!(
        noisy
            .as_raw()
            .iter()
            .zip(clean.as_raw())
            .any(|(a, b)| (a - b).abs() > 1e-5),
        "Generated bokeh noise must reach the renderer's local bokeh crate"
    );
}

#[cfg(unix)]
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a WGPU adapter; run with the shared validation lock"]
/// Checks that an interrupt during rendering exits with a cancellation error and saves no output.
fn test_ctrl_c_does_not_save_a_partial_frame() {
    use std::io::{BufRead, Read};
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    Rgba32FImage::from_fn(512, 512, |x, y| {
        Rgba([((x + y) % 17) as f32, 1.0, 0.0, 0.5])
    })
    .save(&input)
    .unwrap();
    let output = dir.path().join("output.exr");
    let mut child = Command::new(env!("CARGO_BIN_EXE_opendefocus-cli"))
        .args([
            path(&input),
            "-o",
            path(&output),
            "--radius",
            "50",
            "--repeat",
            "100",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut errors = std::io::BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    loop {
        assert!(
            errors.read_line(&mut line).unwrap() > 0,
            "CLI exited before render readiness: {line}"
        );
        if line.starts_with("Rendering ") {
            break;
        }
        line.clear();
    }
    // Signal only after initialization/handler registration, avoiding a startup race.
    // SAFETY: `child` is spawned above and not yet waited on, so its PID still names that
    // process; SIGINT is a valid signal number.
    let kill_status = unsafe { libc::kill(child.id() as i32, libc::SIGINT) };
    assert_eq!(kill_status, 0);
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    let mut stderr = String::new();
    errors.read_to_string(&mut stderr).unwrap();
    assert!(stderr.contains("cancelled"), "{stderr}");
    assert!(!output.exists());
}

#[test]
#[cfg(feature = "gpu")]
/// Checks that an output path naming the input is rejected, and that NaN depth values are rejected.
fn test_input_aliases_and_nonfinite_depth_are_rejected() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.exr");
    source(&input);
    let alias = dir.path().join(".").join("input.exr");
    fails(&[path(&input), "-o", path(&alias)], "Output must differ");
    let output = dir.path().join("output.exr");
    let depth = dir.path().join("depth.exr");
    Rgba32FImage::from_fn(19, 21, |_, _| Rgba([f32::NAN, 0.0, 0.0, 1.0]))
        .save(&depth)
        .unwrap();
    fails(
        &[path(&input), "-o", path(&output), "--depth", path(&depth)],
        "NaN or infinity",
    );
    assert!(!output.exists());
}
