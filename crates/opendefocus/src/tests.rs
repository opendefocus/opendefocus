//! Test module root for the crate, grouping the budget, CoC, GPU and settings checks.
/// Tests for the sample budgets.
mod budgets;
/// Tests for CoC preparation.
mod coc;
/// Hardware tests for the GPU renderer.
mod gpu;
/// Tests for adaptive settings validation.
mod settings;
/// Tests that each adaptive settings rejection reports its exact reason.
#[cfg(feature = "wgpu")]
mod validation;

use super::*;
