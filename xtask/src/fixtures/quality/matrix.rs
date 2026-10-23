//! Runs the static QA matrix: every case at every setting, with the lens variants where
//! requested, plus the gates that compare the variants with each other.
use anyhow::Result;
use serde_json::{Value, json};

use super::{
    cache::CachedRows,
    case::{LensChoice, PlainCache, Row, Setting, run_case},
    context::Context,
    fields::{field, flatten_numbers, opt_str, truthy},
    gates::Gate,
    lens,
};

/// A render that failed: its case, its lens variant and the error text.
pub struct Failure {
    /// Name of the case.
    pub case: String,
    /// Lens variant name, or `plain` when no variant is applied.
    pub lens: String,
    /// Error text of the failed render.
    pub error: String,
}

/// Result of the static matrix: the rows that rendered and the renders that failed.
pub struct MatrixRun {
    /// Rows of the matrix, including the cross-variant gates.
    pub rows: Vec<Row>,
    /// Renders that failed. They are reported with the run instead of being dropped.
    pub execution_failures: Vec<Failure>,
}

/// Run every static case and setting, then add the cross-variant lens gates.
pub fn run(ctx: &Context, cases: &[Value], cached: Option<&CachedRows>) -> Result<MatrixRun> {
    let max_sizes = match &ctx.args.max_sizes {
        Some(sizes) if !sizes.is_empty() => sizes.clone(),
        _ => vec![ctx.args.max_size],
    };
    let variants = lens::variants();
    let mut plain_cache = PlainCache::default();
    let mut rows = Vec::new();
    let mut execution_failures = Vec::new();
    for case in cases {
        let name = opt_str(case, "name").unwrap_or_default().to_owned();
        for setting in settings_for(case, ctx, &max_sizes)? {
            match run_case(ctx, case, setting, None, cached, &mut plain_cache) {
                Ok(row) => rows.push(row),
                Err(error) => execution_failures.push(Failure {
                    case: name.clone(),
                    lens: "plain".to_owned(),
                    error: format!("{error:#}"),
                }),
            }
            let sweep = ctx.args.lens_sweep
                && (truthy(case.get("lens_sweep")) || name == "direct-hdr_points");
            if !sweep {
                continue;
            }
            for (lens_name, settings) in &variants {
                let choice = LensChoice {
                    name: lens_name,
                    settings,
                };
                match run_case(ctx, case, setting, Some(&choice), cached, &mut plain_cache) {
                    Ok(row) => rows.push(row),
                    Err(error) => execution_failures.push(Failure {
                        case: name.clone(),
                        lens: (*lens_name).to_owned(),
                        error: format!("{error:#}"),
                    }),
                }
            }
        }
    }
    add_fingerprint_gates(&mut rows, cases);
    add_inverse_gates(&mut rows);
    Ok(MatrixRun {
        rows,
        execution_failures,
    })
}

/// Setting combinations for one case: focal planes by sizes by max sizes, or one focal for direct CoC.
fn settings_for(case: &Value, ctx: &Context, max_sizes: &[f64]) -> Result<Vec<Setting>> {
    let focal_default = case
        .get("focus_plane")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let focal: Vec<f64> = match &ctx.args.focal_planes {
        Some(values) if !values.is_empty() => values.clone(),
        _ => match case.get("focus_planes") {
            Some(list) => flatten_numbers(list)?,
            None => vec![focal_default],
        },
    };
    let sizes: Vec<f64> = match &ctx.args.sizes {
        Some(values) if !values.is_empty() => values.clone(),
        _ => match case.get("sizes") {
            Some(list) => flatten_numbers(list)?,
            None => vec![24.0],
        },
    };
    let mode = field(case, "mode")?.as_str().unwrap_or_default();
    let mut settings = Vec::new();
    if mode == "direct_coc" {
        for &max_size in max_sizes {
            settings.push(Setting {
                focal: focal[0],
                size: sizes[0],
                max_size,
            });
        }
        return Ok(settings);
    }
    for &focal in &focal {
        for &size in &sizes {
            for &max_size in max_sizes {
                settings.push(Setting {
                    focal,
                    size,
                    max_size,
                });
            }
        }
    }
    Ok(settings)
}

/// Returns the case name and the focal plane, size and maximum size of a row, which identify
/// the setting it was rendered at. A missing number becomes NaN.
fn row_key(row: &Row) -> (String, f64, f64, f64) {
    let text = |key: &str| {
        row.fields
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let number = |key: &str| {
        row.fields
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN)
    };
    (
        text("case"),
        number("focal_plane"),
        number("size"),
        number("max_size"),
    )
}

/// Returns the lens variant of a row, or an empty string when it has none.
fn row_lens(row: &Row) -> &str {
    row.fields
        .get("lens")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Distinct lens variants must produce distinct output images for the same case and setting.
fn add_fingerprint_gates(rows: &mut [Row], cases: &[Value]) {
    let mut seen: Vec<((String, f64, f64, f64), Vec<(String, String)>)> = Vec::new();
    for row in rows.iter_mut() {
        let lens = row_lens(row).to_owned();
        let case_name = row_key(row).0;
        let focused = cases
            .iter()
            .find(|case| opt_str(case, "name") == Some(case_name.as_str()))
            .and_then(|case| opt_str(case, "gate"))
            == Some("focused_identity");
        if lens == "plain" || lens == "inverse_foreground_off" || focused {
            continue;
        }
        let Some(report) = row.fields.get("adaptive_report") else {
            continue;
        };
        let fingerprint = report
            .get("qa")
            .and_then(|qa| qa.get("output_sha256"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let key = row_key(row);
        let index = if let Some(index) = seen.iter().position(|(existing, _)| *existing == key) {
            index
        } else {
            seen.push((key, Vec::new()));
            seen.len() - 1
        };
        let variants = &mut seen[index].1;
        let duplicate = fingerprint.as_ref().and_then(|digest| {
            variants
                .iter()
                .find(|(_, existing)| existing == digest)
                .map(|(name, _)| name.clone())
        });
        row.gates.push(Gate::new(
            "lens_variant_unique_output",
            json!({"sha256": fingerprint, "duplicates_variant": duplicate}),
            json!("distinct finite-image result per distinct lens variant"),
            fingerprint.is_some() && duplicate.is_none(),
        ));
        if let Some(digest) = fingerprint {
            variants.push((lens, digest));
        }
    }
}

/// Foreground inversion must move near spots without moving far spots.
fn add_inverse_gates(rows: &mut [Row]) {
    let mut additions: Vec<(usize, Gate)> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row_lens(row) != "inverse_foreground_off" {
            continue;
        }
        let Some(spots) = spot_array(row) else {
            continue;
        };
        let key = row_key(row);
        let default = rows
            .iter()
            .rev()
            .find(|candidate| row_key(candidate) == key && row_lens(candidate) == "catseye");
        let Some(default_spots) = default.and_then(spot_array) else {
            continue;
        };
        let (near_delta, far_deltas) = spot_deltas(default_spots, spots);
        let max_far = far_deltas.iter().copied().fold(0.0, f64::max);
        let has_near = default_spots.iter().any(|spot| sign_is(spot, "near"));
        let has_far = default_spots.iter().any(|spot| sign_is(spot, "far"));
        if !(has_near && has_far) {
            if !far_deltas.is_empty() {
                additions.push((
                    index,
                    Gate::at_most("foreground_inverse_preserves_far", max_far, 0.05),
                ));
            }
            continue;
        }
        additions.push((
            index,
            Gate::new(
                "foreground_inverse_directionality",
                json!({"near_centroid_delta_px": near_delta, "max_far_centroid_delta_px": max_far}),
                json!({"near_min_px": 0.25, "far_max_px": 0.05}),
                near_delta > 0.25 && max_far <= 0.05,
            ),
        ));
    }
    for (index, gate) in additions {
        rows[index].gates.push(gate);
    }
}

/// Returns the lens spot list of a row, if it has one.
fn spot_array(row: &Row) -> Option<&Vec<Value>> {
    row.fields
        .get("lens_spot_shape_vs_plain")?
        .get("lens")?
        .as_array()
}

/// Whether the `sign` field of a spot equals `sign`.
fn sign_is(spot: &Value, sign: &str) -> bool {
    spot.get("sign").and_then(Value::as_str) == Some(sign)
}

/// Near and far centroid displacements between the default and inverted spots.
fn spot_deltas(default: &[Value], inverted: &[Value]) -> (f64, Vec<f64>) {
    let mut near = 0.0_f64;
    let mut far = Vec::new();
    for (base, other) in default.iter().zip(inverted) {
        if is_empty(base) || is_empty(other) {
            continue;
        }
        let (Some(a), Some(b)) = (centroid(base), centroid(other)) else {
            continue;
        };
        let delta = (a.0 - b.0).hypot(a.1 - b.1);
        if sign_is(base, "near") {
            near = near.max(delta);
        } else if sign_is(base, "far") {
            far.push(delta);
        }
    }
    (near, far)
}

/// Whether a spot record is marked as empty, meaning it holds no measurable content.
fn is_empty(spot: &Value) -> bool {
    spot.get("empty").and_then(Value::as_bool).unwrap_or(false)
}

/// Returns the centroid `(x, y)` of a spot record, or `None` when it is missing.
fn centroid(spot: &Value) -> Option<(f64, f64)> {
    let values = spot.get("centroid_xy")?.as_array()?;
    Some((values.first()?.as_f64()?, values.get(1)?.as_f64()?))
}
