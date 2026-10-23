//! Conversions between `image` buffers and `ndarray` arrays, with normalized
//! float conversion for integer and float channel types.

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![warn(clippy::print_stdout, clippy::print_stderr)]
/// Error type returned by fallible conversions.
mod error;
/// Trait implementations for image-array conversion and normalized floats.
mod traits;

pub use error::Error;
/// Commonly used traits, re-exported for `use image_ndarray::prelude::*`.
pub mod prelude;
