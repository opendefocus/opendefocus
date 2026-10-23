//! Self-checks of the reference metrics and the analytic gates. They run before any GPU
//! render, so a broken reference fails the QA run early.
use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::{Gate, analytic::analytic_gate, channels::channel_grid_metrics};
use crate::fixtures::quality::{
    image::Rgba,
    metrics::{
        reference::border_normalized_disk_reference,
        seam::zero_depth_seam_metrics,
        spot::{SpotPoint, bokeh_spot_metrics},
    },
};

/// `numpy.isclose` with its default relative tolerance.
fn isclose(actual: f64, expected: f64, atol: f64) -> bool {
    (actual - expected).abs() <= atol + 1e-5 * expected.abs()
}

/// Returns the names of the gates that did not pass.
fn failed_names(gates: &[Gate]) -> Vec<String> {
    gates
        .iter()
        .filter(|gate| !gate.pass)
        .map(|gate| gate.name.clone())
        .collect()
}

/// Fails with `message` and the names of the failing gates unless every gate passed.
fn expect_all_pass(gates: &[Gate], message: &str) -> Result<()> {
    let failed = failed_names(gates);
    if !failed.is_empty() {
        bail!("{message}: {failed:?}");
    }
    Ok(())
}

/// Builds an opaque grey image of `width` by `height` from `values`, one per pixel. Panics
/// when `values` is shorter than the image.
fn gray_rgba(values: &[f64], width: usize, height: usize) -> Rgba {
    Rgba::from_fn(width, height, |x, y| {
        let value = values[y * width + x] as f32;
        [value, value, value, 1.0]
    })
}

/// Catch mistakes in the reference metrics before spending time on GPU renders.
pub fn run() -> Result<()> {
    fractional_spot_is_measured()?;
    disk_references_pass_their_own_gates()?;
    clipped_corner_reference_is_stable()?;
    large_shifted_reference_passes()?;
    spot_channels_are_centered()?;
    seam_metric_detects_synthetic_edge()?;
    opaque_lens_alpha_passes()?;
    hdr_energy_profile_passes()?;
    channel_profiles_pass()?;
    alpha_regression_is_rejected()
}

/// Checks that a spot at a fractional position is measured and yields a shape.
fn fractional_spot_is_measured() -> Result<()> {
    let image = Rgba::from_fn(11, 9, |x, y| {
        if (x, y) == (5, 4) {
            [2.0, 1.0, 0.5, 1.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    });
    let spots = bokeh_spot_metrics(
        &image,
        &[SpotPoint {
            xy: (5.25, 4.5),
            sign: "near".to_owned(),
        }],
    );
    if spots.len() != 1 || spots[0].is_empty() {
        bail!("fractional spot self-check produced no shape");
    }
    Ok(())
}

/// Checks that disk references for radii from 2 to 98 pass their own PSF gates.
fn disk_references_pass_their_own_gates() -> Result<()> {
    for radius in [2.0_f64, 4.0, 8.0, 12.0, 16.0, 32.0, 64.0, 98.0] {
        let dimension = (radius * 2.0 + 8.0).ceil() as usize;
        let center = (dimension / 2, dimension / 2);
        let target = border_normalized_disk_reference(
            (dimension, dimension),
            (center.0 as f64, center.1 as f64),
            radius,
            (0.0, 0.0),
        );
        let rgba = Rgba::from_fn(dimension, dimension, |x, y| {
            let value = target[y * dimension + x] as f32;
            [value, value, value, value]
        });
        let case = json!({
            "gate": "disk_psf",
            "radius": radius,
            "expected_centroid_xy": [center.0, center.1],
            "transparent_background": true,
        });
        let analysis = analytic_gate(&case, &rgba, 102.0)?;
        expect_all_pass(
            &analysis.gates,
            &format!("Analytic disk reference self-check failed at r={radius}"),
        )?;
    }
    Ok(())
}

/// Checks that a corner-clipped aperture reference keeps its energy and centroid and passes
/// its gates.
fn clipped_corner_reference_is_stable() -> Result<()> {
    let clipped = border_normalized_disk_reference((33, 33), (2.0, 2.0), 8.0, (0.0, 0.0));
    let mass: f64 = clipped.iter().sum();
    let (mut centroid_x, mut centroid_y) = (0.0, 0.0);
    for y in 0..33 {
        for x in 0..33 {
            centroid_x += clipped[y * 33 + x] * x as f64;
            centroid_y += clipped[y * 33 + x] * y as f64;
        }
    }
    let centroid = (centroid_x / mass, centroid_y / mass);
    if !isclose(mass, 0.778_900_376_3, 1e-8) {
        bail!("Per-destination corner aperture reference energy changed: {mass}");
    }
    if !(isclose(centroid.0, 3.754_507_956, 1e-8) && isclose(centroid.1, 3.754_507_956, 1e-8)) {
        bail!("Per-destination corner aperture centroid changed: {centroid:?}");
    }
    let rgba = Rgba::from_fn(33, 33, |x, y| {
        let value = clipped[y * 33 + x] as f32;
        [value, value, value, 1.0]
    });
    let case = json!({
        "gate": "disk_psf",
        "radius": 8.0,
        "expected_centroid_xy": [2, 2],
        "edge_clipped": true,
    });
    let analysis = analytic_gate(&case, &rgba, 102.0)?;
    expect_all_pass(
        &analysis.gates,
        "Truncated edge-support reference self-check failed",
    )
}

/// Checks that a large, shifted reference with clipped support keeps its energy and passes
/// the PSF gates.
fn large_shifted_reference_passes() -> Result<()> {
    let shift = (2.0 * 98.0 / 256.0, 2.0 * 98.0 / 256.0);
    let center = (160.0, 128.0);
    let large = border_normalized_disk_reference((256, 320), center, 98.0, shift);
    let mass: f64 = large.iter().sum();
    if !isclose(mass, 1.085_126_460_9, 1e-8) {
        bail!("Large-radius clipped-support reference energy changed: {mass}");
    }
    let rgba = gray_rgba(&large, 320, 256);
    let case = json!({
        "gate": "disk_psf",
        "radius": 98.0,
        "expected_centroid_xy": [center.0 + shift.0, center.1 + shift.1],
        "expected_kernel_shift_xy": [shift.0, shift.1],
    });
    let analysis = analytic_gate(&case, &rgba, 102.0)?;
    expect_all_pass(
        &analysis.gates,
        "Shifted large-radius clipped-support self-check failed",
    )
}

/// Checks that per-channel spot metrics find a centred impulse whose support is no larger
/// than that of an r8 PSF.
fn spot_channels_are_centered() -> Result<()> {
    let profile = border_normalized_disk_reference((65, 65), (32.0, 32.0), 8.0, (0.0, 0.0));
    let rgba = Rgba::from_fn(65, 65, |x, y| {
        let value = profile[y * 65 + x];
        [
            (value * 2.0) as f32,
            (value * 3.0) as f32,
            (value * 4.0) as f32,
            1.0,
        ]
    });
    let spot = bokeh_spot_metrics(
        &rgba,
        &[SpotPoint {
            xy: (32.0, 32.0),
            sign: "near".to_owned(),
        }],
    )
    .remove(0);
    let Some(shape) = spot.shape else {
        bail!("Per-channel spot metrics lost the centered impulse");
    };
    for channel in &shape.channels {
        if channel.centroid != Some((32.0, 32.0)) {
            bail!(
                "Per-channel spot metrics lost centered impulse: {:?}",
                channel.centroid
            );
        }
        if channel.support_radius_px > 9.0 {
            bail!(
                "Per-channel support threshold exceeds an r8 PSF: {}",
                channel.support_radius_px
            );
        }
    }
    Ok(())
}

/// Checks that the zero-depth seam metric detects a synthetic vertical edge.
fn seam_metric_detects_synthetic_edge() -> Result<()> {
    let image = Rgba::from_fn(32, 16, |x, _| {
        let value = if x == 16 { 0.75 } else { 0.5 };
        [value, value, value, 1.0]
    });
    if zero_depth_seam_metrics(&image, 16)?.gradient_ratio < 3.0 {
        bail!("Zero-depth seam metric failed its synthetic vertical-edge self-check");
    }
    Ok(())
}

/// Checks that an opaque lens PSF passes its alpha gate.
fn opaque_lens_alpha_passes() -> Result<()> {
    let profile = border_normalized_disk_reference((65, 65), (32.0, 32.0), 8.0, (0.0, 0.0));
    let rgba = Rgba::from_fn(65, 65, |x, y| {
        let value = profile[y * 65 + x];
        [
            (value * 2.0) as f32,
            (value * 3.0) as f32,
            (value * 4.0) as f32,
            1.0,
        ]
    });
    let analysis = analytic_gate(&json!({"gate": "lens_psf"}), &rgba, 102.0)?;
    if analysis.gates.iter().any(|gate| !gate.pass) {
        bail!("Opaque lens PSF alpha self-check failed");
    }
    Ok(())
}

/// Returns an opaque 64 by 64 image of a bright disk: the reference disk of radius 8 at
/// (32, 32), scaled by `energy`.
fn opaque_bright_disk(energy: f64) -> Rgba {
    let profile = border_normalized_disk_reference((64, 64), (32.0, 32.0), 8.0, (0.0, 0.0));
    Rgba::from_fn(64, 64, |x, y| {
        let value = (profile[y * 64 + x] * energy) as f32;
        [value, value, value, 1.0]
    })
}

/// Checks that a bright disk with the expected signal energy passes the PSF gates.
fn hdr_energy_profile_passes() -> Result<()> {
    let opaque = opaque_bright_disk(14.244);
    let case = json!({
        "gate": "disk_psf",
        "radius": 8.0,
        "expected_centroid_xy": [32, 32],
        "expected_signal_energy": 14.244,
    });
    let analysis = analytic_gate(&case, &opaque, 102.0)?;
    expect_all_pass(
        &analysis.gates,
        "HDR input-energy profile self-check failed",
    )
}

/// Checks that three independent RGB disks, each with its own centre, radius and energy,
/// pass the channel gates.
fn channel_profiles_pass() -> Result<()> {
    let channels = [
        ("red", 0, [20.0, 20.0], 8.0, 10.0),
        ("green", 1, [64.0, 64.0], 12.0, 20.0),
        ("blue", 2, [108.0, 108.0], 16.0, 30.0),
    ];
    let mut image = Rgba::new(128, 128, vec![0.0; 128 * 128 * 4]);
    for (_, index, center, radius, energy) in channels {
        let profile = border_normalized_disk_reference(
            (128, 128),
            (center[0], center[1]),
            radius,
            (0.0, 0.0),
        );
        for y in 0..128 {
            for x in 0..128 {
                image.data[(y * 128 + x) * 4 + index] = (profile[y * 128 + x] * energy) as f32;
            }
        }
    }
    for pixel in image.data.as_chunks_mut::<4>().0 {
        pixel[3] = 1.0;
    }
    let case: Value = json!({"channels": channels.iter().map(|(name, index, center, radius, energy)| json!({
        "name": name,
        "index": index,
        "center_xy": center,
        "radius": radius,
        "expected_signal_energy": energy,
    })).collect::<Vec<_>>()});
    let (gates, _) = channel_grid_metrics(&case, &image)?;
    expect_all_pass(&gates, "Independent RGB channel profile self-check failed")
}

/// Checks that fractional alpha on an opaque disk is rejected by the opaque alpha gate.
fn alpha_regression_is_rejected() -> Result<()> {
    let mut opaque = opaque_bright_disk(14.244);
    opaque.data[3] = 0.99;
    let case = json!({
        "gate": "disk_psf",
        "radius": 8.0,
        "expected_centroid_xy": [32, 32],
        "expected_signal_energy": 14.244,
    });
    let analysis = analytic_gate(&case, &opaque, 102.0)?;
    let rejected = analysis
        .gates
        .iter()
        .any(|gate| gate.name == "opaque_alpha_preserved" && !gate.pass);
    if !rejected {
        bail!("Opaque alpha regression self-check did not reject fractional alpha");
    }
    Ok(())
}
