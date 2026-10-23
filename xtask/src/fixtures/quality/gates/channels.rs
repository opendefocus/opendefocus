//! Per-channel PSF gates for multi-channel fixtures, checking each colour channel's disk
//! independently, including leakage into the other channels and alpha preservation.

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use super::Gate;
use crate::fixtures::quality::{
    fields::{field, opt_str, pair, truthy},
    image::Rgba,
    metrics::{disk::disk_signal_metrics, reference::border_normalized_disk_reference},
};

/// Per-channel limits of the disk PSF metrics, in the order of `profile_gates`.
const LIMITS: [(&str, f64); 5] = [
    ("centroid_error_px", 0.25),
    ("energy_relative_error", 0.10),
    ("normalized_disk_l1", 0.15),
    ("normalized_disk_rmse", 0.15),
    ("leakage_beyond_radius_plus_1", 0.02),
];

/// Gate each channel's PSF independently, including cross-channel leakage and opaque alpha.
pub fn channel_grid_metrics(case: &Value, image: &Rgba) -> Result<(Vec<Gate>, Map<String, Value>)> {
    let mut gates = Vec::new();
    let mut channel_metrics = Map::new();
    let channels = field(case, "channels")?
        .as_array()
        .context("`channels` must be an array")?;
    let independent = case
        .get("independent_channel_sources")
        .is_none_or(|flag| truthy(Some(flag)));
    for channel in channels {
        let index = field(channel, "index")?
            .as_f64()
            .context("channel index must be a number")? as usize;
        let expected_energy = field(channel, "expected_signal_energy")?
            .as_f64()
            .context("expected_signal_energy must be a number")?;
        let radius = field(channel, "radius")?
            .as_f64()
            .context("radius must be a number")?;
        let center = pair(field(channel, "center_xy")?)?;
        let shift = match channel.get("kernel_shift_xy") {
            Some(value) => pair(value)?,
            None => (0.0, 0.0),
        };
        let mut reference = border_normalized_disk_reference(image.shape(), center, radius, shift);
        for value in &mut reference {
            *value *= expected_energy;
        }
        let profile = disk_signal_metrics(
            &image.channel(index),
            radius,
            Some(center),
            Some(&reference),
        );
        let name = opt_str(channel, "name").context("channel name must be a string")?;
        let mut entry = profile.to_map();
        for (metric, limit) in LIMITS {
            gates.push(Gate::at_most(
                format!("{name}_{metric}"),
                profile.gated_value(metric),
                limit,
            ));
        }
        if independent {
            let cross = cross_channel_energy(image, index, center, radius);
            let cross_limit = expected_energy * 1e-5;
            entry.insert("cross_channel_energy".to_owned(), json!(cross));
            gates.push(Gate::at_most(
                format!("{name}_cross_channel_energy"),
                cross,
                cross_limit,
            ));
        }
        channel_metrics.insert(name.to_owned(), Value::Object(entry));
    }
    let alpha_error = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| (f64::from(pixel[3]) - 1.0).abs())
        .fold(f64::NEG_INFINITY, f64::max);
    gates.push(Gate::at_most("opaque_alpha_preserved", alpha_error, 1e-5));
    Ok((gates, channel_metrics))
}

/// Absolute energy in the other two channels near one channel's PSF support.
fn cross_channel_energy(image: &Rgba, index: usize, center: (f64, f64), radius: f64) -> f64 {
    let others: Vec<usize> = (0..3).filter(|channel| *channel != index).collect();
    let mut total = 0.0;
    for y in 0..image.height {
        for x in 0..image.width {
            let distance = (x as f64 - center.0).hypot(y as f64 - center.1);
            if distance <= radius + 1.0 {
                total += others
                    .iter()
                    .map(|channel| f64::from(image.at(x, y, *channel)).abs())
                    .sum::<f64>();
            }
        }
    }
    total
}
