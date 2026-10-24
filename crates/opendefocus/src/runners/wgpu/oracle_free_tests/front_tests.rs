//! Hardware tests of the front layer's normalisation: the hand-over from its coverage at the silhouette to its own
//! validity inside the surface must be continuous.
use crate::runners::adaptive::{AdaptiveLayerPixel, PreparedLayers};
use ndarray::Array3;

use super::scenes::uniform_layers;
use super::{feature, high, runner};

/// Frame size (width, height) of the front scene.
const SIZE: (usize, usize) = (128, 32);
/// Near `CoC` of the blurred foreground surface, and the largest radius of the scene.
const SURFACE_COC: f32 = 24.0;
/// Near `CoC` of the less blurred foreground ring next to it, whose discs do not reach the surface interior.
const RING_COC: f32 = 8.0;
/// Columns of the ring; the surface lies left of it and the far background right of it.
const RING: std::ops::Range<usize> = 64..70;
/// Far `CoC` of the background.
const BACKGROUND_COC: f32 = -24.0;
/// Opaque surface colour.
const SURFACE: [f32; 4] = [0.2, 0.3, 0.4, 1.0];
/// Opaque ring colour.
const RING_COLOR: [f32; 4] = [0.9, 0.5, 0.1, 1.0];
/// Opaque background colour.
const BACKGROUND: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
/// Largest change of slope (second difference) in channel 0 along a row of the surface. The front layer's alpha
/// falls from one inside the surface to its coverage over the outer quarter of the potential radius, which bends the
/// row by at most 0.011 per pixel, as much as the blurred silhouette itself; switching between the two normalisers
/// within the last percent of potential occupancy bent it by 0.038 where the potential disc first reached the
/// background behind the ring, a visible line.
const MAX_BEND: f32 = 0.02;

/// Returns the prepared layer pixel of the front scene at column `x`.
fn front_scene_pixel(x: usize) -> AdaptiveLayerPixel {
    let near = |color: [f32; 4], coc: f32| AdaptiveLayerPixel {
        near_rgba: color,
        far_rgba: BACKGROUND,
        near_coc: coc,
        far_coc: BACKGROUND_COC,
        near_valid: 1.0,
        far_valid: 1.0,
    };
    if x < RING.start {
        near(SURFACE, SURFACE_COC)
    } else if x < RING.end {
        near(RING_COLOR, RING_COC)
    } else {
        AdaptiveLayerPixel {
            near_rgba: [0.0; 4],
            far_rgba: BACKGROUND,
            near_coc: 0.0,
            far_coc: BACKGROUND_COC,
            near_valid: 0.0,
            far_valid: 1.0,
        }
    }
}

/// A blurred foreground surface next to a less blurred foreground ring with the far background behind it (the red
/// sphere of the bundled `render3d` scene at focal plane 5.44 and the ring of farther geometry around it). The ring
/// pixels fill the potential disc but do not reach the surface interior, so the front layer's two normalisers differ
/// there by several percent; the surface must still show no ring where the potential disc reaches the background.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_front_interior_has_no_ring_where_the_potential_disc_reaches_the_background() {
    let runner = runner().await;
    let (width, height) = SIZE;
    let color = |x: usize| {
        if x < RING.start {
            SURFACE
        } else if x < RING.end {
            RING_COLOR
        } else {
            BACKGROUND
        }
    };
    let source = Array3::from_shape_fn((height, width, 4), |(_, x, c)| color(x)[c]);
    let prepared: PreparedLayers = uniform_layers(SIZE, |x, _| front_scene_pixel(x));
    let features =
        vec![feature(BACKGROUND_COC, SURFACE_COC, 2.0); width.div_ceil(16) * height.div_ceil(16)];
    let output = runner
        .gather_adaptive_frame_with_budget(&source, &prepared, &features, SURFACE_COC, high())
        .await
        .unwrap();
    for y in 8..height - 8 {
        for x in 8..RING.start - 2 {
            let bend =
                (output[[y, x + 1, 0]] - 2.0 * output[[y, x, 0]] + output[[y, x - 1, 0]]).abs();
            assert!(bend <= MAX_BEND, "bend {bend} at ({x}, {y})");
        }
    }
}
