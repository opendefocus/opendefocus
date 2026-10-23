//! Temporal gates of the QA motion sequences: focus occlusion, focus motion, moving points,
//! colour tracks, the moving disk and the transport-compensated boundary band.
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Map, Value, json};

use super::{
    context::Context,
    exr::read_exr,
    fields::{f64_field, field, pair, str_field},
    gates::{Gate, analytic::analytic_gate, channels::channel_grid_metrics},
    image::{Plane, Rgba},
    metrics::{
        delta::{image_delta, motion_delta},
        transport::transported_point_delta,
    },
    motion::{Frame, Images, Previous},
};

/// Focused surfaces must limit far-sample spread and keep their alpha.
pub fn focus_occlusion(
    ctx: &Context,
    case: &Value,
    label: &str,
    images: &Images,
    frame: &mut Frame,
) -> Result<()> {
    let color = PathBuf::from(str_field(case, "color")?);
    let expected = read_input(ctx, &color, label)?;
    let adaptive = &images.adaptive;
    let source = |x: usize, y: usize| expected.at(x, y, 1) > 1e-8_f32;
    let mut green_error = f64::NEG_INFINITY;
    let mut alpha_error = f64::NEG_INFINITY;
    let mut source_pixels = 0_u64;
    for y in 0..adaptive.height {
        for x in 0..adaptive.width {
            if source(x, y) {
                source_pixels += 1;
                continue;
            }
            let green = (f64::from(adaptive.at(x, y, 1)) - f64::from(expected.at(x, y, 1))).abs();
            let alpha = (f64::from(adaptive.at(x, y, 3)) - f64::from(expected.at(x, y, 3))).abs();
            green_error = green_error.max(green);
            alpha_error = alpha_error.max(alpha);
        }
    }
    let alpha = adaptive.channel(3);
    let alpha_min = f64::from(alpha.data.iter().copied().fold(f32::INFINITY, f32::min));
    let alpha_max = f64::from(alpha.data.iter().copied().fold(f32::NEG_INFINITY, f32::max));
    let input_green = plane_sum(&expected.channel(1));
    let adaptive_green = plane_sum(&adaptive.channel(1));
    let legacy_green = images
        .legacy
        .as_ref()
        .map(|legacy| plane_sum(&legacy.channel(1)));
    frame.fields.insert(
        "focused_occlusion_metrics".to_owned(),
        json!({
            "source_pixels": source_pixels,
            "input_green_energy": input_green,
            "adaptive_green_energy": adaptive_green,
            "focused_surrounding_green_max_abs_error": green_error,
            "focused_surrounding_alpha_max_abs_error": alpha_error,
            "adaptive_alpha_range": [alpha_min, alpha_max],
            "legacy_green_energy": legacy_green,
        }),
    );
    frame.gates = Some(vec![
        Gate::at_most("focused_surface_limits_far_spread", green_error, 1e-5),
        Gate::at_most("focused_surrounding_alpha_preserved", alpha_error, 1e-5),
        Gate::new(
            "alpha_in_unit_range",
            json!([alpha_min, alpha_max]),
            json!([0.0, 1.0]),
            alpha_min >= -1e-6 && alpha_max <= 1.0 + 1e-6,
        ),
    ]);
    Ok(())
}

/// Reads the input colour EXR `color` through the EXR dumper. Its channels are written under
/// a prefix derived from `label` in the output directory.
fn read_input(ctx: &Context, color: &Path, label: &str) -> Result<Rgba> {
    read_exr(
        &ctx.args.exr_dump,
        color,
        &ctx.output.join(format!("{label}-input")),
    )
}

/// Returns the sum of all values of `plane`, accumulated as f64.
fn plane_sum(plane: &Plane) -> f64 {
    plane.data.iter().map(|value| f64::from(*value)).sum()
}

/// Signed-CoC focus sweep: PSF radius, energy and frame-to-frame deltas.
pub fn focus_motion(
    ctx: &Context,
    case: &Value,
    images: &Images,
    previous: &Previous,
    frame: &mut Frame,
) -> Result<()> {
    let psf_case = json!({"gate": "disk_psf", "radius": f64_field(case, "expected_radius")?});
    let analysis = analytic_gate(&psf_case, &images.adaptive, ctx.args.max_size)?;
    frame.fields.insert(
        "signed_coc".to_owned(),
        json!(f64_field(case, "signed_coc")?),
    );
    frame.fields.insert(
        "focus_response_metrics".to_owned(),
        Value::Object(analysis.metrics),
    );
    frame.gates = Some(analysis.gates);
    if let Some(prior) = &previous.adaptive {
        if let (Some(prior_legacy), Some(legacy)) = (&previous.legacy, &images.legacy) {
            let delta = image_delta(prior_legacy, legacy)?;
            frame
                .fields
                .insert("legacy_focus_frame_delta".to_owned(), delta.to_value());
        }
        let delta = image_delta(prior, &images.adaptive)?;
        frame
            .fields
            .insert("adaptive_focus_frame_delta".to_owned(), delta.to_value());
    }
    Ok(())
}

/// Fractional HDR point motion: energy continuity and transport-compensated residue.
/// Returns the frame's point energy for the next frame's continuity check.
pub fn motion_point(
    ctx: &Context,
    case: &Value,
    images: &Images,
    previous: &Previous,
    translation: f64,
    frame: &mut Frame,
) -> Result<f64> {
    let radius = f64_field(case, "expected_radius")?;
    let center = pair(field(case, "expected_center_xy")?)?;
    let psf_case = json!({
        "gate": "disk_psf",
        "radius": radius,
        "expected_centroid_xy": case["expected_center_xy"],
        "expected_signal_energy": case["expected_signal_energy"],
    });
    let analysis = analytic_gate(&psf_case, &images.adaptive, ctx.args.max_size)?;
    let energy = analysis
        .metrics
        .get("rgb_signal_energy")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    let mut gates = analysis.gates;
    frame.fields.insert(
        "point_response_metrics".to_owned(),
        Value::Object(analysis.metrics),
    );
    if let Some(prior_energy) = previous.point_energy {
        let relative = (energy - prior_energy).abs() / prior_energy.max(1e-12);
        gates.push(Gate::at_most(
            "fractional_motion_energy_continuity",
            relative,
            0.10,
        ));
    }
    if let Some(prior) = &previous.adaptive {
        let dx = translation - previous.translation_px;
        let mut transported = Map::new();
        let adaptive =
            transported_point_delta(&prior.luma(), &images.adaptive.luma(), dx, center, radius);
        transported.insert("adaptive".to_owned(), adaptive.to_value());
        if let (Some(prior_legacy), Some(legacy)) = (&previous.legacy, &images.legacy) {
            let point =
                transported_point_delta(&prior_legacy.luma(), &legacy.luma(), dx, center, radius);
            transported.insert("legacy".to_owned(), point.to_value());
        }
        gates.push(Gate::at_most(
            "fractional_motion_transport_energy",
            adaptive.normalized_difference_energy,
            0.05,
        ));
        gates.push(Gate::at_most(
            "fractional_motion_transport_p99",
            adaptive.p99_abs_delta,
            0.03,
        ));
        frame.fields.insert(
            "transport_compensated_point_delta".to_owned(),
            Value::Object(transported),
        );
    }
    frame.gates = Some(gates);
    Ok(energy)
}

/// Independent RGB point tracks: per-channel PSF gates, continuity and transport residue.
/// Returns each channel's signal energy for the next frame.
pub fn motion_channel_grid(
    case: &Value,
    images: &Images,
    previous: &Previous,
    translation: f64,
    frame: &mut Frame,
) -> Result<Vec<(String, f64)>> {
    let (mut gates, metrics) = channel_grid_metrics(case, &images.adaptive)?;
    let channels = field(case, "channels")?
        .as_array()
        .cloned()
        .unwrap_or_default();
    let energies: Vec<(String, f64)> = channels
        .iter()
        .map(|channel| {
            let name = channel
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let energy = metrics
                .get(name)
                .and_then(|entry| entry.get("signal_energy"))
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN);
            (name.to_owned(), energy)
        })
        .collect();
    for (name, energy) in &energies {
        if let Some((_, prior)) = previous
            .channel_energy
            .iter()
            .find(|(prior_name, _)| prior_name == name)
        {
            let relative = (energy - prior).abs() / prior.max(1e-12);
            gates.push(Gate::at_most(
                format!("{name}_fractional_motion_energy_continuity"),
                relative,
                0.10,
            ));
        }
    }
    frame.fields.insert(
        "channel_response_metrics".to_owned(),
        Value::Object(metrics),
    );
    if let Some(prior) = &previous.adaptive {
        let dx = translation - previous.translation_px;
        let mut transported = Map::new();
        for channel in &channels {
            let name = channel
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let index = f64_field(channel, "index")? as usize;
            let center = pair(field(channel, "center_xy")?)?;
            let radius = f64_field(channel, "radius")?;
            let delta = transported_point_delta(
                &prior.channel(index),
                &images.adaptive.channel(index),
                dx,
                center,
                radius,
            );
            transported.insert(name.to_owned(), delta.to_value());
            gates.push(Gate::at_most(
                format!("{name}_transport_energy"),
                delta.normalized_difference_energy,
                0.05,
            ));
            gates.push(Gate::at_most(
                format!("{name}_transport_p99"),
                delta.p99_abs_delta,
                0.03,
            ));
        }
        frame.fields.insert(
            "transport_compensated_channel_delta".to_owned(),
            Value::Object(transported),
        );
    }
    frame.gates = Some(gates);
    Ok(energies)
}

/// Opaque moving disk: only the alpha of the frame is checked.
pub fn motion_disk(images: &Images, frame: &mut Frame) {
    let error = images
        .adaptive
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| (f64::from(pixel[3]) - 1.0).abs())
        .fold(f64::NEG_INFINITY, f64::max);
    frame.gates = Some(vec![Gate::at_most("opaque_alpha_preserved", error, 1e-5)]);
}

/// Boundary-band residue after shifting the previous frame by the whole-pixel motion.
pub fn transport_boundary(
    images: &Images,
    previous: &Previous,
    translation: f64,
    frame: &mut Frame,
) -> Result<()> {
    let Some(prior) = &previous.adaptive else {
        return Ok(());
    };
    let step = (translation - previous.translation_px) as i64;
    let (width, height) = (images.adaptive.width, images.adaptive.height);
    let center = (width as f64 / 2.0, height as f64 / 2.0);
    let aligned = |before: &Rgba, after: &Rgba| -> (Rgba, Rgba) {
        match step {
            1 => (
                before.crop_columns(0..width - 1),
                after.crop_columns(1..width),
            ),
            -1 => (
                before.crop_columns(1..width),
                after.crop_columns(0..width - 1),
            ),
            _ => (before.clone(), after.clone()),
        }
    };
    let mut pairs = vec![("adaptive", aligned(prior, &images.adaptive))];
    if let (Some(prior_legacy), Some(legacy)) = (&previous.legacy, &images.legacy) {
        pairs.push(("legacy", aligned(prior_legacy, legacy)));
    }
    let mut compensated = Map::new();
    let mut adaptive_band = None;
    for (name, (before, after)) in &pairs {
        let delta = motion_delta(before, after, center, 8.0 + 2.0)?;
        if *name == "adaptive" {
            adaptive_band = Some(delta.clone());
        }
        compensated.insert((*name).to_owned(), delta.to_value());
    }
    let Some(band) = adaptive_band else {
        return Ok(());
    };
    let pass = band.boundary_band_mae_rgba <= 0.01 && band.boundary_band_p99_abs_channel <= 0.03;
    frame.gate = Some(Gate::new(
        "transport_compensated_boundary_band",
        json!({
            "mae_rgba": band.boundary_band_mae_rgba,
            "p99_abs_channel": band.boundary_band_p99_abs_channel,
        }),
        json!({"mae_rgba": 0.01, "p99_abs_channel": 0.03}),
        pass,
    ));
    frame.fields.insert(
        "transport_compensated".to_owned(),
        Value::Object(compensated),
    );
    if let (Some(prior_legacy), Some(legacy)) = (&previous.legacy, &images.legacy) {
        let delta = image_delta(prior_legacy, legacy)?;
        frame
            .fields
            .insert("legacy_raw_frame_delta".to_owned(), delta.to_value());
    }
    let delta = image_delta(prior, &images.adaptive)?;
    frame
        .fields
        .insert("adaptive_raw_frame_delta".to_owned(), delta.to_value());
    Ok(())
}
