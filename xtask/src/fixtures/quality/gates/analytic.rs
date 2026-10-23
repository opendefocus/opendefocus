//! Analytic gates for one rendered case: alpha range, opaque and transparent edges, focus
//! identity, the zero-depth seam, and the disk PSF and disk edge checks.
use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::{Gate, profile_gates};
use crate::fixtures::quality::{
    fields::{f64_field, field, flatten_numbers, opt_f64, opt_str, pair, truthy},
    image::{Plane, Rgba, min_max},
    metrics::{
        disk::{DiskProfile, disk_signal_metrics},
        reference::{border_normalized_disk_reference, circle_overlap_fraction},
        seam::zero_depth_seam_metrics,
    },
};

/// Gates and scalar metrics computed for one rendered case.
#[derive(Default)]
pub struct Analysis {
    /// Gate results, in evaluation order.
    pub gates: Vec<Gate>,
    /// Scalar metrics written to the row.
    pub metrics: Map<String, Value>,
}

impl Analysis {
    /// Records a metric under `key`, replacing an earlier value with the same key.
    fn metric(&mut self, key: &str, value: Value) {
        self.metrics.insert(key.to_owned(), value);
    }

    /// Records every metric of `profile` under `prefix`, followed by the metric name.
    fn insert_profile(&mut self, prefix: &str, profile: &DiskProfile) {
        for (key, value) in profile.to_map() {
            self.metrics.insert(format!("{prefix}{key}"), value);
        }
    }

    /// Adds the transparent-black gate: the largest RGB magnitude over pixels with alpha at most
    /// 1e-6 must stay within 2e-5.
    fn transparent_black(&mut self, image: &Rgba) {
        let max_rgb = image
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] <= 1e-6_f32)
            .flat_map(|pixel| pixel[..3].iter().map(|value| f64::from(value.abs())))
            .fold(0.0, f64::max);
        self.metric("transparent_rgb_max", json!(max_rgb));
        self.gates
            .push(Gate::at_most("transparent_black", max_rgb, 2e-5));
    }
}

/// Analytic checks for one rendered image, dispatched on the fixture's `gate` kind.
pub fn analytic_gate(case: &Value, image: &Rgba, max_size: f64) -> Result<Analysis> {
    let mut analysis = Analysis::default();
    let gate = opt_str(case, "gate");
    let alpha = image.channel(3);
    let (alpha_min, alpha_max) = min_max(&alpha.data);
    let (alpha_min, alpha_max) = (f64::from(alpha_min), f64::from(alpha_max));
    let alpha_range_ok = alpha_min >= -1e-6 && alpha_max <= 1.0 + 1e-6;
    analysis.metric("alpha_range", json!([alpha_min, alpha_max]));
    analysis.gates.push(Gate::new(
        "alpha_in_unit_range",
        json!([alpha_min, alpha_max]),
        json!([0, 1]),
        alpha_range_ok,
    ));
    let opaque_kind = matches!(gate, Some("disk_psf" | "disk_edge" | "lens_psf"));
    if opaque_kind && !truthy(case.get("transparent_background")) {
        let error = alpha
            .data
            .iter()
            .map(|value| (f64::from(*value) - 1.0).abs())
            .fold(f64::NEG_INFINITY, f64::max);
        analysis.metric("opaque_alpha_max_abs_error", json!(error));
        analysis
            .gates
            .push(Gate::at_most("opaque_alpha_preserved", error, 1e-5));
    }
    let transparent = truthy(case.get("transparent_background"));
    match gate {
        Some("focused_identity") => focused_identity(case, image, &mut analysis)?,
        Some("lens_psf") => {
            if transparent {
                analysis.transparent_black(image);
            }
        }
        Some("focused_gradient_identity") => focused_gradient(image, &mut analysis),
        Some("zero_depth_seam") => zero_depth_seam(case, image, &mut analysis)?,
        Some("disk_psf") => disk_psf(case, image, &alpha, max_size, &mut analysis)?,
        Some("disk_edge") => disk_edge(case, image, max_size, &mut analysis)?,
        _ => {}
    }
    if opt_str(case, "name").is_some_and(|name| name.ends_with("alpha_edges")) {
        analysis.transparent_black(image);
    }
    Ok(analysis)
}

/// Adds the focused-identity gate: each value must match `expected_rgba` within 1e-5.
/// `expected_rgba` is either one RGBA pixel for the whole frame or one value per channel per
/// pixel. Fails when its size matches neither.
fn focused_identity(case: &Value, image: &Rgba, analysis: &mut Analysis) -> Result<()> {
    let expected = flatten_numbers(field(case, "expected_rgba")?)?;
    // NumPy broadcasting: a single RGBA pixel applies to every pixel of the frame.
    let broadcast = expected.len() == 4;
    if !broadcast && expected.len() != image.data.len() {
        bail!("expected_rgba shape does not match the render");
    }
    let value = image
        .data
        .iter()
        .enumerate()
        .map(|(index, actual)| {
            let reference = if broadcast {
                expected[index % 4]
            } else {
                expected[index]
            };
            (f64::from(*actual) - reference).abs()
        })
        .fold(f64::NEG_INFINITY, f64::max);
    analysis.metric("focused_identity_max_abs", json!(value));
    analysis
        .gates
        .push(Gate::at_most("focused_identity", value, 1e-5));
    Ok(())
}

/// Adds the gradient gate: the image must match the analytic colour gradient within 1e-5.
fn focused_gradient(image: &Rgba, analysis: &mut Analysis) {
    let (width, height) = (image.width as f64, image.height as f64);
    let mut worst = f64::NEG_INFINITY;
    for y in 0..image.height {
        for x in 0..image.width {
            let (fx, fy) = (x as f64, y as f64);
            let expected = [
                fx / width * 4.0,
                fy / height * 2.0,
                (fx + fy) / (width + height),
                1.0,
            ];
            for (channel, reference) in expected.iter().enumerate() {
                worst = worst.max((f64::from(image.at(x, y, channel)) - reference).abs());
            }
        }
    }
    analysis.metric("focused_gradient_max_abs", json!(worst));
    analysis
        .gates
        .push(Gate::at_most("focused_gradient_identity", worst, 1e-5));
}

/// Adds the seam metrics at the `zero_depth_seam_x` column and the gate that the seam gradient
/// ratio stays below 3. Fails when the field is missing.
fn zero_depth_seam(case: &Value, image: &Rgba, analysis: &mut Analysis) -> Result<()> {
    let seam_x = f64_field(case, "zero_depth_seam_x")? as i64;
    let seam = zero_depth_seam_metrics(image, seam_x)?;
    let entries = [
        ("seam_gradient_mean", seam.gradient_mean),
        ("seam_gradient_max", seam.gradient_max),
        ("neighbor_gradient_median", seam.neighbor_gradient_median),
        ("gradient_ratio", seam.gradient_ratio),
    ];
    for (key, value) in entries {
        analysis.metric(&format!("zero_depth_{key}"), json!(value));
    }
    analysis.gates.push(Gate::at_most(
        "zero_depth_column_has_no_vertical_seam",
        seam.gradient_ratio,
        3.0,
    ));
    Ok(())
}

/// Adds the disk PSF gates. The expected disk centre is `expected_centroid_xy` minus
/// `expected_kernel_shift_xy`, and the radius is capped at `max_size`. The RGB profile gates
/// apply, and the alpha profile gates too when the background is transparent. Fails when a
/// field is missing or the reference cannot be built.
fn disk_psf(
    case: &Value,
    image: &Rgba,
    alpha: &Plane,
    max_size: f64,
    analysis: &mut Analysis,
) -> Result<()> {
    let radius = f64_field(case, "radius")?.min(max_size);
    let shift = match case.get("expected_kernel_shift_xy") {
        Some(value) => pair(value)?,
        None => (0.0, 0.0),
    };
    let expected_center = match case.get("expected_centroid_xy") {
        Some(value) if truthy(Some(value)) => pair(value)?,
        _ => ((image.width / 2) as f64, (image.height / 2) as f64),
    };
    let center = (expected_center.0 - shift.0, expected_center.1 - shift.1);
    let mut reference = border_normalized_disk_reference(image.shape(), center, radius, shift);
    if let Some(energy) = opt_f64(case, "expected_signal_energy") {
        for value in &mut reference {
            *value *= energy;
        }
    }
    let luma = image.luma();
    let profile = disk_signal_metrics(&luma, radius, Some(center), Some(&reference));
    analysis.insert_profile("rgb_", &profile);
    analysis.gates.extend(profile_gates("disk_rgb", &profile));
    if truthy(case.get("transparent_background")) {
        let alpha_profile = disk_signal_metrics(alpha, radius, Some(center), Some(&reference));
        analysis.insert_profile("alpha_", &alpha_profile);
        analysis
            .gates
            .extend(profile_gates("disk_alpha", &alpha_profile));
        analysis.transparent_black(image);
    }
    Ok(())
}

/// Adds the foreground disk-edge gate: the RMSE between the red channel and the expected
/// coverage of a disk of `foreground_radius` pixels at the frame centre must stay within 0.12.
fn disk_edge(case: &Value, image: &Rgba, max_size: f64, analysis: &mut Analysis) -> Result<()> {
    let foreground = f64_field(case, "foreground_radius")?;
    let blur = f64_field(case, "radius")?.min(max_size);
    let (center_x, center_y) = (image.width as f64 / 2.0, image.height as f64 / 2.0);
    let mut squared = 0.0;
    for y in 0..image.height {
        for x in 0..image.width {
            let distance = (x as f64 + 0.5 - center_x).hypot(y as f64 + 0.5 - center_y);
            let target = circle_overlap_fraction(distance, foreground, blur);
            let error = f64::from(image.at(x, y, 0)) - target;
            squared += error * error;
        }
    }
    let rmse = (squared / (image.width * image.height) as f64).sqrt();
    analysis.metric("disk_edge_red_rmse", json!(rmse));
    analysis
        .gates
        .push(Gate::at_most("foreground_disk_coverage_rmse", rmse, 0.12));
    Ok(())
}
