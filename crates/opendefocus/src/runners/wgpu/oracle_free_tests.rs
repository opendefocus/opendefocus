//! GPU-only regression tests that do not call the CPU reference renderer.
//!
//! They replace the GPU-versus-CPU comparisons in `wgpu.rs` so the CPU
//! reference can be retired. Three kinds of expectation are used:
//! analytic or brute-force references computed in this file, invariants of the
//! algorithm (energy, symmetry, continuity), and frozen probe values (see
//! [`frozen`]), which are regression guards and not oracles.
//!
//! Shared scene builders live here so that the one-off capture test and the
//! guard tests render exactly the same inputs.

use crate::AdaptiveSampleBudget;

use super::{TileFeatures, WgpuRunner};

/// Hardware tests for the far occlusion of far surfaces.
mod far_occlusion_tests;
/// Hardware tests for focus reconstruction.
mod focus_tests;
/// Frozen probe values for the lens and layer paths.
mod frozen;
/// Hardware tests for the gather against analytic references.
mod gather_tests;
/// Hardware tests for the Accurate layer paths.
mod layer_tests;
/// Hardware tests for lens effects against analytic references.
mod lens_tests;
/// Analytic CPU reference helpers for the oracle-free tests.
mod references;
/// Analytic test scenes and their probe pixels.
mod scenes;

/// Creates the adaptive WGPU runner used by the hardware tests. Panics when no hardware GPU is available.
async fn runner() -> WgpuRunner {
    WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required")
}

/// Builds a tile feature with the given minimum and maximum CoC and class.
fn feature(min_coc: f32, max_coc: f32, class: f32) -> TileFeatures {
    let mut feature = TileFeatures { values: [0.0; 11] };
    feature.values[0] = min_coc;
    feature.values[1] = max_coc;
    feature.values[9] = class;
    feature
}

/// Returns the sample budget for the Accurate quality tier.
fn high() -> AdaptiveSampleBudget {
    AdaptiveSampleBudget::for_quality(crate::datamodel::render::Quality::Accurate)
}
