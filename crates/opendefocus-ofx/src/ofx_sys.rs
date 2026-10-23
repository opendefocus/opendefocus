//! Hand-written FFI declarations for the subset of the `OpenFX` API used by this plugin.
//!
//! Every layout, function-pointer signature and constant string is copied from the
//! official headers in the `OpenFX` distribution (BSD-3-Clause), not from memory.
//! Line numbers in the comments refer to those headers.
//!
//! Variadic C functions (`...`) are declared with C-variadic function pointer types such as
//! `unsafe extern "C" fn(OfxParamHandle, ...) -> OfxStatus`. Stable Rust can declare and call
//! those types; it cannot define variadic functions, which is why the test mock host needs
//! its own workaround.

#![allow(
    non_snake_case,
    reason = "OFX C API names are kept exactly as the header defines them"
)]
#![allow(
    non_camel_case_types,
    reason = "OFX C API names are kept exactly as the header defines them"
)]

mod core_types;
pub use core_types::*;
mod status;
pub use status::*;
mod actions;
pub use actions::*;
mod suite_names;
pub use suite_names::*;
mod properties;
pub use properties::*;
mod suite_structs;
pub use suite_structs::*;
mod nuke_camera;
pub use nuke_camera::*;
