//! Hardware tests of the far occlusion: a far surface hides the more blurred far background behind it, while a
//! receding far surface keeps the full reach of its farther parts.
use ndarray::Array3;

use crate::runners::adaptive::PreparedLayers;

use super::references::{assert_close, far_reference};
use super::scenes::{effect_settings, far_pixel, lens_for, uniform_layers};
use super::{feature, high, runner};
use crate::AdaptiveSampleBudget;
use crate::datamodel::render::Quality;

/// Frame size (width, height) of the far occlusion scenes.
const SIZE: (usize, usize) = (96, 32);
/// Column where the far surface of the occluder scene ends and the background starts.
const SILHOUETTE: usize = 48;
/// Far `CoC` of the occluding surface, in pixels.
const SURFACE_COC: f32 = 2.0;
/// Far `CoC` of the background behind it, and the largest radius of every scene.
const BACKGROUND_COC: f32 = 24.0;
/// Opaque surface colour.
const SURFACE: [f32; 4] = [0.1, 0.2, 0.3, 1.0];
/// Bright HDR background colour, so that any leak is large.
const BACKGROUND: [f32; 4] = [4.0, 2.0, 1.0, 1.0];
/// Distance in pixels from the silhouette beyond which rays reaching a surface pixel cross only the surface: twice
/// its probe radius.
const HIDDEN_DISTANCE: usize = 8;

/// Index of the axial aberration effect (amount 1.5) in `effect_settings`: its channel radii differ by about a
/// quarter.
const AXIAL_EFFECT: usize = 4;

/// Renders opaque far-only `color` with far `coc` through the exact and the sampled gather.
async fn render_far_layer(
    color: impl Fn(usize, usize) -> [f32; 4],
    coc: impl Fn(usize, usize) -> f32,
) -> (Array3<f32>, [Array3<f32>; 2]) {
    render_far_layer_with_lens(color, coc, None).await
}

/// Renders opaque far-only `color` with far `coc` through the exact and the sampled gather, with the lens `lens`.
async fn render_far_layer_with_lens(
    color: impl Fn(usize, usize) -> [f32; 4],
    coc: impl Fn(usize, usize) -> f32,
    lens: Option<&crate::convolve_settings::ConvolveSettings>,
) -> (Array3<f32>, [Array3<f32>; 2]) {
    let runner = runner().await;
    let (width, height) = SIZE;
    let source = Array3::from_shape_fn((height, width, 4), |(y, x, c)| color(x, y)[c]);
    let prepared: PreparedLayers = uniform_layers(SIZE, |x, y| far_pixel(color(x, y), -coc(x, y)));
    let features =
        vec![feature(-BACKGROUND_COC, 0.0, 2.0); width.div_ceil(16) * height.div_ceil(16)];
    let mut outputs = Vec::new();
    for budget in [high(), AdaptiveSampleBudget::for_quality(Quality::Fast)] {
        outputs.push(
            runner
                .gather_adaptive_frame_with_lens(
                    &source,
                    &prepared,
                    &features,
                    BACKGROUND_COC,
                    budget,
                    lens,
                )
                .await
                .unwrap(),
        );
    }
    let sampled = outputs.pop().unwrap();
    let exact = outputs.pop().unwrap();
    (source, [exact, sampled])
}

/// A slightly blurred far surface in front of a strongly blurred far background (the right edge of the glossy
/// sphere in the bundled `render3d` scene). The rays that reach a surface pixel cross the surface's depth inside a
/// disc of its own `CoC`, so away from the silhouette the background must not show through. A single normalised far
/// gather lets every background disc that reaches a surface pixel bleed into it; in `render3d` that washed out the
/// sphere's rim, and the hand-over from the sharp source to the far gather drew a hard line across it.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_far_surface_hides_the_background_behind_it() {
    let (_, outputs) = render_far_layer(
        |x, _| if x < SILHOUETTE { SURFACE } else { BACKGROUND },
        |x, _| {
            if x < SILHOUETTE {
                SURFACE_COC
            } else {
                BACKGROUND_COC
            }
        },
    )
    .await;
    assert_surface_hides_background(&outputs, SILHOUETTE - HIDDEN_DISTANCE);
}

/// Asserts that the exact and the sampled output of the occluder scene show the surface colour in every channel
/// left of column `hidden_end`, away from the frame edge.
fn assert_surface_hides_background(outputs: &[Array3<f32>; 2], hidden_end: usize) {
    // The sampled gather reads filtered layers and a sampled probe disc.
    for ((label, tolerance), output) in [("exact", 1e-4), ("sampled", 1e-2)].iter().zip(outputs) {
        for y in 8..SIZE.1 - 8 {
            for x in 8..hidden_end {
                for (channel, expected) in SURFACE.iter().enumerate() {
                    let actual = output[[y, x, channel]];
                    assert!(
                        (actual - expected).abs() <= *tolerance,
                        "{label}: background leaks into the surface at ({x}, {y}) channel {channel}: {actual}"
                    );
                }
            }
        }
    }
}

/// The occluder scene through axial aberration, whose channel radii differ: the hidden background is subtracted
/// with each channel's own aperture weights, so no channel keeps a residue of the background. One weight for every
/// channel (that of alpha) left the other channels' background partly in the gather.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_far_surface_hides_the_background_in_every_channel_with_axial_aberration() {
    let center = (SIZE.0 as f32 * 0.5, SIZE.1 as f32 * 0.5);
    let lens = lens_for(&effect_settings(AXIAL_EFFECT, SIZE, center), SIZE, None);
    let (_, outputs) = render_far_layer_with_lens(
        |x, _| if x < SILHOUETTE { SURFACE } else { BACKGROUND },
        |x, _| {
            if x < SILHOUETTE {
                SURFACE_COC
            } else {
                BACKGROUND_COC
            }
        },
        Some(&lens),
    )
    .await;
    assert_surface_hides_background(&outputs, SILHOUETTE - HIDDEN_DISTANCE);
}

/// A far surface that recedes steeply (0.75 px of `CoC` per row) is not an occluder: its farther rows lie on the
/// open side of every pixel's probe disc, so the exact gather must still match the unoccluded brute-force far
/// reference. Tolerance 1e-5 relative, as for the other brute-force layer references.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_receding_far_surface_keeps_its_far_reach() {
    let coc = |_: usize, y: usize| (1.5 + 0.75 * (SIZE.1 - 1 - y) as f32).min(BACKGROUND_COC);
    let color = |x: usize, y: usize| {
        let checker = if (x / 4 + y / 2).is_multiple_of(2) {
            0.8
        } else {
            0.2
        };
        [checker, 0.5 * checker, 0.25, 1.0]
    };
    let (_, [exact, _]) = render_far_layer(color, coc).await;
    assert_close(
        &exact,
        &far_reference(SIZE, color, coc),
        1e-5,
        "receding far surface",
    );
}

/// Far `CoC` of the ring between the surface and the background in the ring scene: above the surface's probe radius
/// (twice [`RING_SURFACE_COC`]) but below the `CoC` at which a source is wholly behind it.
const RING_COC: f32 = 9.4;
/// Far `CoC` of the surface in front of the ring.
const RING_SURFACE_COC: f32 = 4.0;
/// Columns of the ring in the ring scene; the background starts at its end.
const RING: std::ops::Range<usize> = 48..52;
/// Colour of the ring: no red, so that channel 0 measures the background alone.
const RING_COLOR: [f32; 4] = [0.0, 0.0, 4.0, 1.0];
/// Largest step in channel 0 (the background alone) between neighbouring surface pixels of the ring scene. The
/// background fades in over the probe radius (8 px) in steps of up to 0.2; a probe that switched from hiding it to
/// hiding nothing as it reached the ring drew a step of about 0.6 at 8 px from the ring.
const RING_MAX_STEP: f32 = 0.3;

/// A far surface in front of a moderately blurred ring and a strongly blurred background behind it (the blue sphere
/// of the bundled `render3d` scene at focal plane 5.44, with the 13.3-deep ring around it). Every occlusion weight is
/// continuous in space, so the surface shows no hard arc where its probe disc first reaches the ring.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_far_occlusion_has_no_step_where_the_probe_reaches_a_ring() {
    let (_, outputs) = render_far_layer(
        |x, _| {
            if x < RING.start {
                SURFACE
            } else if x < RING.end {
                RING_COLOR
            } else {
                BACKGROUND
            }
        },
        |x, _| {
            if x < RING.start {
                RING_SURFACE_COC
            } else if x < RING.end {
                RING_COC
            } else {
                BACKGROUND_COC
            }
        },
    )
    .await;
    for (label, output) in ["exact", "sampled"].iter().zip(&outputs) {
        for y in 8..SIZE.1 - 8 {
            for x in 8..RING.start - 4 {
                let step = (output[[y, x + 1, 0]] - output[[y, x, 0]]).abs();
                assert!(
                    step <= RING_MAX_STEP,
                    "{label}: step {step} between ({x}, {y}) and ({}, {y})",
                    x + 1
                );
            }
        }
    }
}
