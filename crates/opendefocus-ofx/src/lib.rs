//! `OpenFX` plugin for `OpenDefocus`.
//!
//! The plugin is a thin host adapter around the `opendefocus` renderer. The OFX API is
//! hand-declared in [`ofx_sys`]; no bindgen or C++ code is involved.

#![warn(clippy::print_stdout, clippy::print_stderr)]

pub mod camera;
pub mod ofx_sys;
pub mod params;
pub mod plugin;
pub mod render;
pub mod suites;
