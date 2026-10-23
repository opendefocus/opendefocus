//! Bokeh kernel rendering: settings and an optional renderer that evaluates the
//! kernel per pixel, with the `image` and `noise` features adding array and noise support.

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

/// Settings types describing the bokeh shape, filter and noise.
mod datamodel;
/// Per-pixel bokeh renderer, available with the `renderer` feature.
#[cfg(feature = "renderer")]
mod renderer;

pub use crate::datamodel::*;
#[cfg(feature = "renderer")]
pub use renderer::Renderer;
