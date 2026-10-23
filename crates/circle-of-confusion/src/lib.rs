//! Circle of confusion (CoC) calculation: the public settings types and the
//! [`Calculator`] that maps depth values to signed CoC sizes.

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

/// Per-value CoC evaluation from precomputed settings.
mod calculator;
/// Public settings, camera and enum types.
mod datamodel;
/// PyO3 bindings that expose the calculator to Python.
#[cfg(feature = "python-bindings")]
mod python;

pub use crate::datamodel::*;

/// Precomputes a [`Calculator`] for `settings`.
///
/// Create one calculator per settings set and reuse it for every depth value.
#[must_use]
pub fn initialize_calculator(settings: Settings) -> Calculator {
    Calculator::new(settings)
}

/// Returns the signed circle of confusion size for a depth `value`.
///
/// The value is interpreted with the calculator's [`Math`] mode. The result is
/// clamped to the settings' `max_size`, and is positive or negative depending on
/// whether the depth lies in front of or behind the focal plane.
#[must_use]
pub fn calculate(calculator: &Calculator, value: f32) -> f32 {
    calculator.calculate(value)
}
