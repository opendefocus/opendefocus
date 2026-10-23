//! Lens-variant gates: focus identity, spatial response against the plain render, spot shape,
//! and the per-channel comparison with the Legacy renderer, which is diagnostic only.
use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

use super::{
    case::Row,
    context::Context,
    fields::{field, pair},
    gates::Gate,
    image::Rgba,
    metrics::{
        delta::image_delta,
        spot::{Spot, SpotPoint, bokeh_spot_metrics},
    },
};

/// Note stored with the Legacy comparison: these per-channel moments are diagnostic and do not
/// decide pass or fail.
const LEGACY_ROLE: &str =
    "diagnostic only; absolute moments include renderer differences and clipped frame support";

/// Lens-variant checks: focus identity, spatial response against the plain render, and spot shape.
pub fn lens_gates(
    ctx: &Context,
    case: &Value,
    lens_name: &str,
    adaptive: &Rgba,
    legacy: Option<&Rgba>,
    plain: &Rgba,
    dimensions: (usize, usize),
    row: &mut Row,
) -> Result<()> {
    let lens_effect = image_delta(plain, adaptive)?;
    let points = spot_points(case)?;
    let (width, height) = dimensions;
    let center = ctx
        .center_override()
        .unwrap_or((width as f64 * 0.5, height as f64 * 0.5));
    let off_axis_points: Vec<&SpotPoint> = points
        .iter()
        .filter(|point| point.sign != "focus")
        .collect();
    let off_axis = off_axis_points.is_empty()
        || off_axis_points.iter().any(|point| {
            let spread_x = (point.xy.0 - center.0).abs() / (width as f64 * 0.5).max(1.0);
            let spread_y = (point.xy.1 - center.1).abs() / (height as f64 * 0.5).max(1.0);
            spread_x.max(spread_y) > 0.25
        });
    let require_spatial_response = lens_name == "axial_aberration" || off_axis;
    row.fields.insert(
        "spatial_response_requires_off_axis_point".to_owned(),
        json!(off_axis),
    );
    row.fields
        .insert("lens_effect_vs_plain".to_owned(), lens_effect.to_value());
    for point in points.iter().filter(|point| point.sign == "focus") {
        let focus_error = focus_patch_error(adaptive, plain, point.xy);
        row.gates.push(Gate::at_most(
            "lens_focused_point_identity_max_abs",
            focus_error,
            1e-5,
        ));
    }
    if !points.is_empty() {
        spot_shape_gates(lens_name, off_axis, &points, adaptive, legacy, plain, row)?;
    }
    if case.get("gate").and_then(Value::as_str) != Some("focused_identity")
        && require_spatial_response
    {
        row.gates.push(Gate::new(
            "lens_effect_nonzero",
            json!(lens_effect.mae_rgb),
            json!("> 1e-7"),
            lens_effect.mae_rgb > 1e-7,
        ));
    }
    Ok(())
}

/// Parse the case's `bright_points` list; an absent list is empty.
pub fn spot_points(case: &Value) -> Result<Vec<SpotPoint>> {
    let Some(points) = case.get("bright_points").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    points
        .iter()
        .map(|point| {
            let sign = field(point, "sign")?
                .as_str()
                .unwrap_or_default()
                .to_owned();
            Ok(SpotPoint {
                xy: pair(field(point, "xy")?)?,
                sign,
            })
        })
        .collect()
}

/// Largest absolute difference between the adaptive and plain renders over all RGBA channels
/// of a small patch around `(x, y)`: two pixels before and three after, clipped to the image.
/// It checks that a focused point is left unchanged by the lens.
fn focus_patch_error(adaptive: &Rgba, plain: &Rgba, (x, y): (f64, f64)) -> f64 {
    let rows = (y.floor() - 2.0).max(0.0) as usize
        ..((y.ceil() + 3.0).max(0.0) as usize).min(adaptive.height);
    let columns = (x.floor() - 2.0).max(0.0) as usize
        ..((x.ceil() + 3.0).max(0.0) as usize).min(adaptive.width);
    let mut worst = f64::NEG_INFINITY;
    for py in rows {
        for px in columns.clone() {
            for channel in 0..4 {
                let difference =
                    f64::from(adaptive.at(px, py, channel)) - f64::from(plain.at(px, py, channel));
                worst = worst.max(difference.abs());
            }
        }
    }
    worst
}

/// Adds the spot-shape section to `row`: plain, lens and Legacy spot metrics and the
/// per-channel comparison with Legacy. Then adds the lens response gate: anisotropy change
/// for off-axis catseye, barndoors and astigmatism, or support-radius change for axial
/// aberration. Fails when the axial support cannot be computed.
fn spot_shape_gates(
    lens_name: &str,
    off_axis: bool,
    points: &[SpotPoint],
    adaptive: &Rgba,
    legacy: Option<&Rgba>,
    plain: &Rgba,
    row: &mut Row,
) -> Result<()> {
    let plain_spots = bokeh_spot_metrics(plain, points);
    let lens_spots = bokeh_spot_metrics(adaptive, points);
    let legacy_spots = legacy.map(|legacy| bokeh_spot_metrics(legacy, points));
    let channel_errors = legacy_spots
        .as_deref()
        .map(|legacy| adaptive_vs_legacy(&lens_spots, legacy))
        .unwrap_or_default();
    let mut section = Map::new();
    section.insert("plain".to_owned(), spots_value(&plain_spots));
    section.insert("lens".to_owned(), spots_value(&lens_spots));
    section.insert(
        "legacy_lens".to_owned(),
        legacy_spots.as_deref().map_or(Value::Null, spots_value),
    );
    section.insert(
        "adaptive_vs_legacy_per_channel".to_owned(),
        Value::Array(channel_errors.iter().map(ChannelError::to_value).collect()),
    );
    section.insert("legacy_comparison_role".to_owned(), json!(LEGACY_ROLE));
    if !channel_errors.is_empty() {
        section.insert(
            "adaptive_vs_legacy_max".to_owned(),
            channel_maximums(&channel_errors),
        );
    }
    row.fields.insert(
        "lens_spot_shape_vs_plain".to_owned(),
        Value::Object(section),
    );
    if matches!(lens_name, "catseye" | "barndoors" | "astigmatism") && off_axis {
        let shape_change = plain_spots
            .iter()
            .zip(&lens_spots)
            .filter(|(base, effected)| {
                !base.is_empty() && !effected.is_empty() && base.sign != "focus"
            })
            .map(|(base, effected)| {
                let (base, effected) = (base.shape.as_ref(), effected.shape.as_ref());
                match (base, effected) {
                    (Some(base), Some(effected)) => (base.anisotropy - effected.anisotropy).abs(),
                    _ => 0.0,
                }
            })
            .fold(0.0, f64::max);
        row.gates.push(Gate::new(
            "lens_spot_shape_response",
            json!(shape_change),
            json!("> 0.02 anisotropy change"),
            shape_change > 0.02,
        ));
    } else if lens_name == "axial_aberration" {
        let max_change = axial_support_change(&plain_spots, &lens_spots)?;
        row.gates.push(Gate::new(
            "axial_channel_support_response",
            json!(max_change),
            json!("> 0.1 px channel support change"),
            max_change > 0.1,
        ));
    }
    Ok(())
}

/// Converts spot metrics to a JSON array.
fn spots_value(spots: &[Spot]) -> Value {
    Value::Array(spots.iter().map(Spot::to_value).collect())
}

/// Largest per-channel support-radius change between the plain and lens spots.
fn axial_support_change(plain: &[Spot], lens: &[Spot]) -> Result<f64> {
    let mut spreads = Vec::new();
    for (base, effected) in plain.iter().zip(lens) {
        let (Some(base), Some(effected)) = (&base.shape, &effected.shape) else {
            continue;
        };
        let changes: Vec<f64> = base
            .channels
            .iter()
            .zip(&effected.channels)
            .filter(|(base, effected)| base.centroid.is_some() && effected.centroid.is_some())
            .map(|(base, effected)| (effected.support_radius_px - base.support_radius_px).abs())
            .collect();
        if changes.is_empty() {
            bail!("max() arg is an empty sequence");
        }
        spreads.push(changes.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    }
    Ok(spreads.into_iter().fold(0.0, f64::max))
}

/// Difference between the adaptive and Legacy spots of one bright point in one colour channel.
struct ChannelError {
    /// Position of the bright point in pixels.
    point: (f64, f64),
    /// Colour channel name: `r`, `g` or `b`.
    channel: &'static str,
    /// Distance between the two centroids, in pixels.
    centroid_error_px: f64,
    /// Energy difference relative to the Legacy energy.
    energy_relative_error: f64,
    /// Difference between the two support radii, in pixels.
    support_radius_error_px: f64,
    /// Relative L1 difference of the covariance eigenvalues.
    eigenvalue_error: f64,
}

impl ChannelError {
    /// Converts the error to a JSON object using the metric names of the report.
    fn to_value(&self) -> Value {
        json!({
            "point_xy": [self.point.0, self.point.1],
            "channel": self.channel,
            "centroid_error_px": self.centroid_error_px,
            "energy_relative_error": self.energy_relative_error,
            "support_radius_error_px": self.support_radius_error_px,
            "covariance_eigenvalue_l1_relative_error": self.eigenvalue_error,
        })
    }
}

/// Per-channel moment differences between the adaptive and Legacy spots (diagnostic only).
fn adaptive_vs_legacy(lens: &[Spot], legacy: &[Spot]) -> Vec<ChannelError> {
    let mut errors = Vec::new();
    for (actual_spot, reference_spot) in lens.iter().zip(legacy) {
        let (Some(actual_shape), Some(reference_shape)) =
            (&actual_spot.shape, &reference_spot.shape)
        else {
            continue;
        };
        for (channel, (actual, reference)) in ["r", "g", "b"]
            .into_iter()
            .zip(actual_shape.channels.iter().zip(&reference_shape.channels))
        {
            let (Some(actual_centroid), Some(reference_centroid)) =
                (actual.centroid, reference.centroid)
            else {
                continue;
            };
            let centroid_error = (actual_centroid.0 - reference_centroid.0)
                .hypot(actual_centroid.1 - reference_centroid.1);
            let energy_error =
                (actual.energy - reference.energy).abs() / reference.energy.max(1e-12);
            let eigen_error = match (actual.eigenvalues, reference.eigenvalues) {
                (Some(a), Some(r)) => {
                    ((a[0] - r[0]).abs() + (a[1] - r[1]).abs())
                        / (r[0].abs() + r[1].abs()).max(1e-12)
                }
                _ => 0.0,
            };
            errors.push(ChannelError {
                point: actual_spot.xy,
                channel,
                centroid_error_px: centroid_error,
                energy_relative_error: energy_error,
                support_radius_error_px: (actual.support_radius_px - reference.support_radius_px)
                    .abs(),
                eigenvalue_error: eigen_error,
            });
        }
    }
    errors
}

/// Largest value of each error metric over all channels, as a JSON object.
fn channel_maximums(errors: &[ChannelError]) -> Value {
    let max_of = |value: fn(&ChannelError) -> f64| {
        errors.iter().map(value).fold(f64::NEG_INFINITY, f64::max)
    };
    let mut map = Map::new();
    map.insert(
        "centroid_error_px".to_owned(),
        json!(max_of(|e| e.centroid_error_px)),
    );
    map.insert(
        "energy_relative_error".to_owned(),
        json!(max_of(|e| e.energy_relative_error)),
    );
    map.insert(
        "support_radius_error_px".to_owned(),
        json!(max_of(|e| e.support_radius_error_px)),
    );
    map.insert(
        "covariance_eigenvalue_l1_relative_error".to_owned(),
        json!(max_of(|e| e.eigenvalue_error)),
    );
    Value::Object(map)
}
