//! Pass/fail gates of the QA matrix: the `Gate` record and its constructors, and the profile
//! gates shared by the disk PSF checks.
pub mod analytic;
pub mod channels;
pub mod selfcheck;

use serde::Serialize;
use serde_json::{Value, json};

use crate::fixtures::quality::metrics::disk::DiskProfile;

/// One named pass/fail check, serialized with the `name`, `metric`, `tolerance`, `pass` keys.
#[derive(Clone, Debug, Serialize)]
pub struct Gate {
    /// Gate name, unique within a row.
    pub name: String,
    /// Measured value, a number or a JSON array.
    pub metric: Value,
    /// Limit the measurement is compared with, in the same form as `metric`.
    pub tolerance: Value,
    /// Whether the measurement met the tolerance.
    pub pass: bool,
}

impl Gate {
    /// Creates a gate from its name, measured value, tolerance description and pass result.
    pub fn new(name: impl Into<String>, metric: Value, tolerance: Value, pass: bool) -> Self {
        Self {
            name: name.into(),
            metric,
            tolerance,
            pass,
        }
    }

    /// A scalar check that passes when `value <= limit`.
    pub fn at_most(name: impl Into<String>, value: f64, limit: f64) -> Self {
        Self::new(name, json!(value), json!(limit), value <= limit)
    }

    /// Serialises the gate to a JSON object. Every field of a gate is JSON-serialisable, so the
    /// `null` fallback is never produced.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// The five profile gates shared by disk PSF checks, named `PREFIX_<check>`.
pub fn profile_gates(prefix: &str, profile: &DiskProfile) -> Vec<Gate> {
    [
        ("centroid_error_px", 0.25, "centroid"),
        ("energy_relative_error", 0.10, "energy"),
        ("normalized_disk_l1", 0.15, "normalized_l1"),
        ("normalized_disk_rmse", 0.15, "normalized_rmse"),
        ("leakage_beyond_radius_plus_1", 0.02, "leakage"),
    ]
    .into_iter()
    .map(|(metric, limit, label)| {
        Gate::at_most(
            format!("{prefix}_{label}"),
            profile.gated_value(metric),
            limit,
        )
    })
    .collect()
}
