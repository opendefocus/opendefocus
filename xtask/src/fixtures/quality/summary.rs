//! Writes the QA summary: rows, failures with their gates, the method and provenance sections,
//! the acceptance thresholds, and the console report.
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};

use super::{
    case::Row,
    contact::export_visual_review,
    context::Context,
    fields::field,
    gates::Gate,
    manifest::Selection,
    matrix::{Failure, MatrixRun},
    motion::Frame,
    provenance,
};

/// Build the matrix summary, write it and fail when any gate did not pass.
pub fn write_matrix(
    ctx: &Context,
    manifest_path: &Path,
    manifest: &Value,
    selection: &Selection,
    run: MatrixRun,
) -> Result<()> {
    let failures = matrix_failures(&run.rows, &run.execution_failures);
    let mut summary = Map::new();
    summary.insert(
        "manifest".to_owned(),
        json!(manifest_path.display().to_string()),
    );
    summary.insert(
        "rows".to_owned(),
        Value::Array(run.rows.into_iter().map(Row::into_value).collect()),
    );
    summary.insert(
        "execution_failures".to_owned(),
        Value::Array(
            run.execution_failures
                .iter()
                .map(execution_failure_value)
                .collect(),
        ),
    );
    summary.insert("failures".to_owned(), Value::Array(failures));
    finish(ctx, manifest, selection, summary)
}

/// Build the temporal-sequence summary, write it and fail when any gate did not pass.
pub fn write_motion(
    ctx: &Context,
    manifest_path: &Path,
    manifest: &Value,
    selection: &Selection,
    frames: &[Frame],
) -> Result<()> {
    let mut failures: Vec<Value> = frames
        .iter()
        .flat_map(|frame| frame.gates.iter().flatten())
        .filter(|gate| !gate.pass)
        .map(Gate::to_value)
        .collect();
    failures.extend(
        frames
            .iter()
            .filter_map(|frame| frame.gate.as_ref())
            .filter(|gate| !gate.pass)
            .map(Gate::to_value),
    );
    let mut summary = Map::new();
    summary.insert(
        "manifest".to_owned(),
        json!(manifest_path.display().to_string()),
    );
    summary.insert(
        selection.sequence_key.to_owned(),
        Value::Array(frames.iter().map(Frame::to_value).collect()),
    );
    summary.insert("failures".to_owned(), Value::Array(failures));
    finish(ctx, manifest, selection, summary)
}

/// Lists every failed gate of the matrix rows and every render that failed, each tagged with
/// its case and lens variant.
fn matrix_failures(rows: &[Row], execution_failures: &[Failure]) -> Vec<Value> {
    let mut failures = Vec::new();
    for row in rows {
        let case = row.fields.get("case").cloned().unwrap_or(Value::Null);
        let lens = row.fields.get("lens").cloned().unwrap_or(Value::Null);
        for gate in row.gates.iter().filter(|gate| !gate.pass) {
            failures.push(json!({"case": case, "lens": lens, "gate": gate.to_value()}));
        }
    }
    failures.extend(execution_failures.iter().map(|failure| {
        json!({
            "case": failure.case,
            "lens": failure.lens,
            "gate": Gate::new(
                "render_succeeded",
                json!(failure.error),
                json!("successful finite render"),
                false,
            ).to_value(),
        })
    }));
    failures
}

/// Converts a failed render to a JSON record with its case, lens and error text.
fn execution_failure_value(failure: &Failure) -> Value {
    json!({"case": failure.case, "lens": failure.lens, "error": failure.error})
}

/// Add the shared review, method, provenance and threshold sections, then write and report.
fn finish(
    ctx: &Context,
    manifest: &Value,
    selection: &Selection,
    mut summary: Map<String, Value>,
) -> Result<()> {
    let visual_review = export_visual_review(&ctx.output)?;
    let storage = field(manifest, "storage").context("manifest has no storage description")?;
    summary.insert("visual_review".to_owned(), Value::Object(visual_review));
    summary.insert(
        "method".to_owned(),
        json!({
            "legacy": "paired comparative reference with known algorithmic defects; not ground truth",
            "legacy_lens_spot_metrics": "diagnostic only: absolute per-channel moments do not isolate the lens effect, and edge points have clipped support",
            "analytic": "focused identity, high-resolution disk/pixel coverage, exact disk overlap, per-destination clipped-aperture normalization, premultiplied alpha and transport-compensated motion",
            "input_storage": storage,
        }),
    );
    summary.insert(
        "provenance".to_owned(),
        provenance::build(ctx, manifest, selection)?,
    );
    summary.insert("initial_acceptance_thresholds".to_owned(), thresholds());
    let summary_path = ctx.output.join("summary.json");
    let failures = summary
        .get("failures")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    std::fs::write(
        &summary_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Value::Object(summary))?
        ),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "summary": summary_path.display().to_string(),
            "failure_count": failures.len(),
            "failures": failures,
        }))?
    );
    if !failures.is_empty() {
        bail!(
            "{} adaptive quality gate(s) failed; see {}",
            failures.len(),
            summary_path.display()
        );
    }
    Ok(())
}

/// Returns the initial acceptance thresholds recorded with every summary.
fn thresholds() -> Value {
    json!({
        "focused_identity_max_abs": 1e-5,
        "disk_centroid_error_px": 0.25,
        "disk_energy_relative_error": 0.10,
        "disk_normalized_l1_error": 0.15,
        "disk_normalized_profile_rmse": 0.15,
        "disk_leakage_outside_radius_plus_1": 0.02,
        "foreground_disk_coverage_rmse": 0.12,
        "alpha_range": [0.0, 1.0],
        "opaque_alpha_max_abs_error": 1e-5,
        "transparent_black_rgb_max": 2e-5,
        "transport_compensated_motion_mae_rgba": 0.03,
        "note": "Initial thresholds for small analytic CoC; tighten after calibrated renderer results. Profile norms are relative to expected disk mass/RMS, unlike absolute RMSE.",
    })
}
