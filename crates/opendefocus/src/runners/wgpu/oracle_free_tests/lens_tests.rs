//! Hardware tests of lens effects against analytic probes and point-spread checks.
use crate::convolve_settings::ConvolveSettings;
use bytemuck::Zeroable;
use ndarray::Array3;

use crate::AdaptiveSampleBudget;
use crate::runners::adaptive::AdaptiveLayerPixel;

use super::frozen;
use super::references::{assert_close, assert_probes, disc_coverage, max_abs_diff, moments};
use super::scenes::{
    Case, effect_settings, far_pixel, high_lens_cases, high_lens_probe_pixels, lens_effect_cases,
    lens_effect_probe_pixels, lens_for, plain_lens_variant, uniform_layers,
};
use super::{feature, high, runner};

/// Regression guard (frozen at 9fe44e0, see [`frozen`]): the six lens effects.
/// Tolerance 1e-5 absolute: the captured GPU output was bit-identical across
/// two runs and the constants are rounded to 1e-6.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_frozen_lens_effect_probes() {
    let runner = runner().await;
    let cases = lens_effect_cases();
    assert_eq!(cases.len(), frozen::LENS_EFFECT.len());
    for case in &cases {
        let output = case.gpu(&runner).await;
        let pixels = lens_effect_probe_pixels();
        assert_probes(
            "lens-effect",
            case,
            &output,
            &pixels,
            frozen::LENS_EFFECT,
            1e-5,
        );
    }
}

/// Regression guard (frozen at 9fe44e0): the eleven Accurate lens-integral cases.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_frozen_high_lens_integral_probes() {
    let runner = runner().await;
    let cases = high_lens_cases();
    assert_eq!(cases.len(), frozen::HIGH_LENS.len());
    for case in &cases {
        let output = case.gpu(&runner).await;
        let pixels = high_lens_probe_pixels();
        assert_probes("high-lens", case, &output, &pixels, frozen::HIGH_LENS, 1e-5);
    }
}

/// Plan item 5(b), the feasible subset. A lens with every effect off and the
/// inverse-foreground shift disabled is a plain circle and must equal the
/// no-lens render exactly. A catseye of amount 0 must not depend on the lens
/// centre. The plan's other identities are false and are therefore not tested:
/// catseye amount 0 is a soft-rimmed disc (it differs from `none` by 1.63 at
/// the 1.25 px column) and barndoors have no neutral amount (amount 0 still
/// cuts the aperture on the far side of the screen).
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_high_none_lens_is_plain_disc_and_catseye_zero_ignores_center() {
    let runner = runner().await;
    let base = high_lens_cases().remove(0);
    let with = |lens: Option<ConvolveSettings>| Case {
        name: "identity".into(),
        source: base.source.clone(),
        prepared: base.prepared.clone(),
        features: base.features.clone(),
        max_radius: base.max_radius,
        lens,
    };
    let plain = with(None).gpu(&runner).await;
    let none = with(Some(plain_lens_variant(|_| {}))).gpu(&runner).await;
    assert!(
        max_abs_diff(&none, &plain) <= 1e-6,
        "lens with all effects off differs from the plain disc by {}",
        max_abs_diff(&none, &plain)
    );
    let catseye_zero = |center: (f32, f32)| {
        plain_lens_variant(|s| {
            s.non_uniform.catseye.enable = true;
            s.non_uniform.catseye.amount = 0.0;
            s.render.center.x = center.0;
            s.render.center.y = center.1;
        })
    };
    let a = with(Some(catseye_zero((16.0, 12.0)))).gpu(&runner).await;
    let b = with(Some(catseye_zero((3.0, 20.0)))).gpu(&runner).await;
    assert!(
        max_abs_diff(&a, &b) <= 1e-6,
        "catseye amount 0 depends on the lens centre: {}",
        max_abs_diff(&a, &b)
    );
    // Guard against a vacuous pass: the catseye mask must still do something.
    assert!(max_abs_diff(&a, &none) > 0.1);
}

/// Plan item 4, replacement for the CPU comparison. A single bright pixel on
/// black, blurred with a 12 px `CoC`, reveals each lens effect as a change of the
/// point-spread function (PSF) relative to a plain circle. Expected behaviour,
/// read off the shader formulas and checked at two mirrored screen positions:
/// - catseye clips the aperture toward the lens centre: the PSF centroid moves
///   toward the centre and mass drops (the aperture is cut);
/// - barndoors cut light from the outer sides: the centroid moves away from the
///   centre and the (renormalised) mass rises;
/// - astigmatism squeezes the PSF (second moments fall) and shifts it outward;
/// - axial aberration shrinks one colour channel radius by `0.25*|amount|`
///   (red when `polarity*amount > 0`, blue when `< 0`, green by half) so the
///   PSF variance of that channel scales with the square of the radius.
/// Foreground and background (inverse foreground off) must agree for the
/// geometric effects and swap the axial channels. The centroid-shift bands are
/// +-15% around the values measured at 9fe44e0 (catseye 1.03..1.09 px, barndoors
/// -1.33..-1.53, astigmatism -0.48..-0.50): regression bands, not derived
/// constants. Weak spot: wrong gamma/softness curves are not distinguished.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_lens_effects_reshape_point_spread_as_designed() {
    let runner = runner().await;
    let (w, h) = (96usize, 80usize);
    let center = (48.0f32, 40.0f32);
    for (px, py) in [(70usize, 20usize), (30, 60)] {
        let to_center = ((center.0 as f64 - px as f64), (center.1 as f64 - py as f64));
        let norm = to_center.0.hypot(to_center.1);
        let unit = (to_center.0 / norm, to_center.1 / norm);
        let color = |x: usize, y: usize| {
            if (x, y) == (px, py) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        };
        let source = Array3::from_shape_fn((h, w, 4), |(y, x, c)| color(x, y)[c]);
        let features = vec![feature(-12.0, 12.0, 2.0); w.div_ceil(16) * h.div_ceil(16)];
        let mut results = Vec::new();
        for foreground in [false, true] {
            let prepared = uniform_layers((w, h), |x, y| {
                let c = color(x, y);
                if foreground {
                    AdaptiveLayerPixel {
                        near_rgba: c,
                        near_coc: 12.0,
                        near_valid: 1.0,
                        ..AdaptiveLayerPixel::zeroed()
                    }
                } else {
                    far_pixel(c, -12.0)
                }
            });
            let mut per_effect = Vec::new();
            for effect in [6usize, 0, 1, 2, 3, 4, 5] {
                let lens = lens_for(&effect_settings(effect, (w, h), center), (w, h), None);
                let output = runner
                    .gather_adaptive_frame_with_lens(
                        &source,
                        &prepared,
                        &features,
                        12.0,
                        high(),
                        Some(&lens),
                    )
                    .await
                    .unwrap();
                assert!(output.iter().all(|v| v.is_finite()));
                per_effect.push([
                    moments(&output, 0),
                    moments(&output, 1),
                    moments(&output, 2),
                ]);
            }
            results.push(per_effect);
        }
        let shift_along = |m: &(f64, f64, f64, f64, f64), plain: &(f64, f64, f64, f64, f64)| {
            (m.1 - plain.1) * unit.0 + (m.2 - plain.2) * unit.1
        };
        for (foreground, per_effect) in results.iter().enumerate() {
            let label = format!("source ({px},{py}) foreground={}", foreground == 1);
            let plain = per_effect[0][0];
            // catseye
            let m = per_effect[1][0];
            assert!(
                (0.9..1.25).contains(&shift_along(&m, &plain)),
                "{label} catseye shift {}",
                shift_along(&m, &plain)
            );
            assert!(
                m.0 < plain.0 * 0.98,
                "{label} catseye mass {} vs {}",
                m.0,
                plain.0
            );
            assert!(
                m.3 < plain.3 * 0.95 && m.4 < plain.4 * 0.95,
                "{label} catseye spread"
            );
            // barndoors
            let m = per_effect[2][0];
            assert!(
                (-1.75..-1.15).contains(&shift_along(&m, &plain)),
                "{label} barndoors shift {}",
                shift_along(&m, &plain)
            );
            assert!(
                m.0 > plain.0 * 1.15,
                "{label} barndoors mass {} vs {}",
                m.0,
                plain.0
            );
            // astigmatism
            let m = per_effect[3][0];
            assert!(
                (-0.6..-0.38).contains(&shift_along(&m, &plain)),
                "{label} astigmatism shift {}",
                shift_along(&m, &plain)
            );
            assert!(
                m.3 < plain.3 * 0.92 && m.4 < plain.4 * 0.92,
                "{label} astigmatism spread"
            );
            // axial: colour channel c shrinks to (1 - k)^2 of the plain variance
            let near = foreground == 1;
            let variance = |m: &(f64, f64, f64, f64, f64)| f64::midpoint(m.3, m.4);
            let plain_variance = variance(&plain);
            for (effect_index, amount) in [(5usize, 1.5f64), (6, -1.5)] {
                let polarity = if near { -1.0 } else { 1.0 };
                let signed = polarity * amount;
                let corrected = amount.abs() * 0.25;
                let expected = [
                    if signed > 0.0 { corrected } else { 0.0 },
                    corrected * 0.5,
                    if signed < 0.0 { corrected } else { 0.0 },
                ];
                for channel in 0..3 {
                    let measured = variance(&per_effect[effect_index][channel]) / plain_variance;
                    let target = (1.0 - expected[channel]).powi(2);
                    assert!(
                        (measured - target).abs() < 0.03,
                        "{label} axial amount {amount} channel {channel}: variance ratio {measured}, expected {target}"
                    );
                }
            }
        }
        // Geometric effects do not distinguish foreground from background.
        for effect in 1..=3 {
            for channel in 0..3 {
                let (bg, fg) = (results[0][effect][channel], results[1][effect][channel]);
                assert!((bg.1 - fg.1).abs() < 1e-3 && (bg.2 - fg.2).abs() < 1e-3);
                assert!((bg.3 - fg.3).abs() < 1e-2 && (bg.0 - fg.0).abs() < 1e-3);
            }
        }
    }
}

/// Port of the point-spread-function stage tests. For a lone bright far pixel
/// in the middle of a black frame the gather output is `cov(d) / L` where `L`
/// is the lattice sum of the coverage (translation invariant away from the
/// frame edges). Therefore, at Accurate and for several radii:
/// - the PSF sums to exactly 1 (energy conservation);
/// - its support is the pixel-centre disc `|d| < R + 0.5`, no more, no less;
/// - every pixel with `|d| <= R - 0.5` carries the same plateau value 1/L;
/// - it has the eight-fold symmetry of the pixel grid;
/// - sweeping R in 0.05 steps the peak (1/L) falls monotonically and each step
///   changes it by 1.0..2.5 x the relative area change `dR / R` (continuity
///   across the small-radius grid cut-offs; measured 2.7% per 0.05 at R=3).
/// The sampled Fast path is also checked for energy conservation (1e-4).
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_point_source_psf_has_unit_energy_and_pixel_centre_disc_support() {
    let runner = runner().await;
    let n = 41usize;
    let center = 20usize;
    let render = |radius: f32, budget: AdaptiveSampleBudget| {
        let runner = &runner;
        async move {
            let color = |x: usize, y: usize| {
                if (x, y) == (center, center) {
                    [1.0, 0.5, 0.25, 1.0]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                }
            };
            let source = Array3::from_shape_fn((n, n, 4), |(y, x, c)| color(x, y)[c]);
            let prepared = uniform_layers((n, n), |x, y| far_pixel(color(x, y), -radius));
            runner
                .gather_adaptive_frame_with_budget(
                    &source,
                    &prepared,
                    &[feature(-radius, 0.0, 2.0); 9],
                    radius,
                    budget,
                )
                .await
                .unwrap()
        }
    };
    let at = |radius: f32, x: usize, y: usize| {
        disc_coverage(radius, x as f32 - center as f32, y as f32 - center as f32)
    };
    let mut previous: Option<(f32, f32)> = None;
    for radius in [2.3f32, 3.0, 3.05, 3.1, 3.4, 5.3, 8.25]
        .into_iter()
        .chain((0..=20).map(|k| 3.0 + k as f32 * 0.05))
    {
        let output = render(radius, high()).await;
        for (channel, scale) in [1.0f64, 0.5, 0.25].into_iter().enumerate() {
            let sum: f64 = output
                .slice(ndarray::s![.., .., channel])
                .iter()
                .map(|&v| v as f64)
                .sum();
            assert!(
                (sum - scale).abs() < 1e-5,
                "R={radius} channel {channel}: energy {sum}"
            );
        }
        let lattice: f64 = (0..n)
            .flat_map(|y| (0..n).map(move |x| (x, y)))
            .map(|(x, y)| at(radius, x, y) as f64)
            .sum();
        let peak = output[[center, center, 0]];
        assert!(
            (peak as f64 * lattice - 1.0).abs() < 1e-5,
            "R={radius}: peak {peak} vs 1/{lattice}"
        );
        for y in 0..n {
            for x in 0..n {
                let distance = (x as f32 - center as f32).hypot(y as f32 - center as f32);
                let value = output[[y, x, 0]];
                assert_eq!(
                    value > 1e-7,
                    distance < radius + 0.5,
                    "R={radius}: support at ({x},{y})"
                );
                if distance <= radius - 0.5 {
                    assert!(
                        (value - peak).abs() < 1e-6,
                        "R={radius}: plateau at ({x},{y})"
                    );
                }
                let (dx, dy) = (x as i32 - center as i32, y as i32 - center as i32);
                let mirrored = output[[
                    (center as i32 + dx) as usize,
                    (center as i32 + dy) as usize,
                    0,
                ]];
                let transposed = output[[
                    (center as i32 + dy) as usize,
                    (center as i32 + dx) as usize,
                    0,
                ]];
                let flipped = output[[
                    (center as i32 - dy) as usize,
                    (center as i32 - dx) as usize,
                    0,
                ]];
                assert!((mirrored - transposed).abs() < 1e-7 && (mirrored - flipped).abs() < 1e-7);
            }
        }
        if let Some((previous_radius, previous_peak)) =
            previous.filter(|(r, _)| (radius - r - 0.05).abs() < 1e-4)
        {
            let change = (previous_peak - peak) / previous_peak;
            let area = 0.05 / previous_radius;
            assert!(
                change > area * 1.0 && change < area * 2.5,
                "R={radius}: peak changed by {change}, expected {area}..{}",
                area * 2.5
            );
        }
        previous = Some((radius, peak));
        let low = render(
            radius,
            AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Fast),
        )
        .await;
        let sum: f64 = low
            .slice(ndarray::s![.., .., 0])
            .iter()
            .map(|&v| v as f64)
            .sum();
        assert!((sum - 1.0).abs() < 1e-4, "R={radius}: low energy {sum}");
    }
}

/// Port of the mip-blend continuity tests. The sampled Fast path
/// switches mip levels as the footprint grows; sweeping the `CoC` of a point
/// source from 2 to 28.5 px in 0.45 px steps must not show a step. For each
/// radius the PSF has unit energy (0.03) and a variance close to the uniform
/// disc value R^2/4 per axis (within 4% for R >= 3.5; measured 0.98..1.00), and
/// the variance ratio changes by at most 1.5% between neighbouring radii
/// (measured 1%). Weak spot: only the second moment is smooth-checked, not the
/// shape of the profile.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_sampled_psf_varies_smoothly_across_mip_blend_radii() {
    let runner = runner().await;
    let (n, c) = (101usize, 50usize);
    for quality in [crate::datamodel::render::Quality::Fast] {
        let budget = AdaptiveSampleBudget::for_quality(quality);
        let mut previous: Option<f64> = None;
        for k in 0..60 {
            let radius = 2.0 + k as f32 * 0.45;
            let color = |x: usize, y: usize| {
                if (x, y) == (c, c) {
                    [1.0, 0.0, 0.0, 1.0]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                }
            };
            let source = Array3::from_shape_fn((n, n, 4), |(y, x, ch)| color(x, y)[ch]);
            let prepared = uniform_layers((n, n), |x, y| far_pixel(color(x, y), -radius));
            let tiles = n.div_ceil(16) * n.div_ceil(16);
            let output = runner
                .gather_adaptive_frame_with_budget(
                    &source,
                    &prepared,
                    &vec![feature(-radius, 0.0, 2.0); tiles],
                    radius,
                    budget,
                )
                .await
                .unwrap();
            let m = moments(&output, 0);
            assert!(
                (m.0 - 1.0).abs() < 0.03,
                "{quality:?} R={radius}: energy {}",
                m.0
            );
            let ratio = f64::midpoint(m.3, m.4) / (radius as f64 * radius as f64 / 4.0);
            if radius >= 3.5 {
                assert!(
                    (ratio - 1.0).abs() < 0.04,
                    "{quality:?} R={radius}: variance ratio {ratio}"
                );
                if let Some(previous) = previous {
                    assert!(
                        (ratio - previous).abs() < 0.015,
                        "{quality:?} R={radius}: variance ratio jumped from {previous} to {ratio}"
                    );
                }
                previous = Some(ratio);
            }
        }
    }
}

/// Port of `non_uniform_weights_are_finite_at_exact_lens_center` and the
/// singular-settings stage tests. With gamma 0 (a singular exponent), zero
/// softness and the lens centre exactly on a pixel, every effect enabled, the
/// blurred constant frame (far plate on the left, near on the right) must come
/// back as that constant (1e-3) at every pixel. Weak spot: a constant frame
/// passes even if the weights are all masked (the shader falls back to the
/// source), so this guards against NaN/inf and holes in coverage, not against
/// wrong mask shapes.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_singular_lens_settings_and_exact_center_keep_constant_frame() {
    let runner = runner().await;
    let (w, h) = (33usize, 25usize);
    let color = [3.0f32, 0.5, 0.25, 1.0];
    let source = Array3::from_shape_fn((h, w, 4), |(_, _, c)| color[c]);
    let prepared = uniform_layers((w, h), |x, _| {
        if x < 16 {
            far_pixel(color, -6.0)
        } else {
            AdaptiveLayerPixel {
                near_rgba: color,
                near_coc: 6.0,
                near_valid: 1.0,
                ..AdaptiveLayerPixel::zeroed()
            }
        }
    });
    let features = vec![feature(-6.0, 6.0, 2.0); 6];
    for center in [(16.0f32, 12.0f32), (0.0, 0.0), (32.0, 24.0)] {
        let mut settings = effect_settings(99, (w, h), center);
        settings.non_uniform.catseye.enable = true;
        settings.non_uniform.catseye.gamma = 0.0;
        settings.non_uniform.catseye.softness = 0.0;
        settings.non_uniform.barndoors.enable = true;
        settings.non_uniform.barndoors.gamma = 0.0;
        settings.non_uniform.astigmatism.enable = true;
        settings.non_uniform.astigmatism.gamma = 0.0;
        let lens = lens_for(&settings, (w, h), None);
        let output = runner
            .gather_adaptive_frame_with_lens(
                &source,
                &prepared,
                &features,
                6.0,
                high(),
                Some(&lens),
            )
            .await
            .unwrap();
        assert_close(
            &output,
            &source,
            1e-3,
            &format!("singular lens, centre {center:?}"),
        );
    }
}
