//! Hardware tests of the far occlusion: a far surface hides the more blurred far background behind it, while a
//! receding far surface keeps the full reach of its farther parts.
use ndarray::Array3;

use crate::runners::adaptive::PreparedLayers;

use super::references::{assert_close, far_reference};
use super::scenes::{far_pixel, uniform_layers};
use super::{feature, high, runner};

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

/// Renders opaque far-only `color` with far `coc` through the exact and the sampled gather.
async fn render_far_layer(
    color: impl Fn(usize, usize) -> [f32; 4],
    coc: impl Fn(usize, usize) -> f32,
) -> (Array3<f32>, [Array3<f32>; 2]) {
    let runner = runner().await;
    let (width, height) = SIZE;
    let source = Array3::from_shape_fn((height, width, 4), |(y, x, c)| color(x, y)[c]);
    let prepared: PreparedLayers = uniform_layers(SIZE, |x, y| far_pixel(color(x, y), -coc(x, y)));
    let features =
        vec![feature(-BACKGROUND_COC, 0.0, 2.0); width.div_ceil(16) * height.div_ceil(16)];
    let exact = runner
        .gather_adaptive_frame_with_budget(&source, &prepared, &features, BACKGROUND_COC, high())
        .await
        .unwrap();
    let sampled = runner
        .gather_adaptive_frame(&source, &prepared, &features, BACKGROUND_COC)
        .await
        .unwrap();
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
    // The sampled gather reads filtered layers and a sampled probe disc.
    for ((label, tolerance), output) in [("exact", 1e-4), ("sampled", 1e-2)].iter().zip(&outputs) {
        for y in 8..SIZE.1 - 8 {
            for x in 8..SILHOUETTE - HIDDEN_DISTANCE {
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
