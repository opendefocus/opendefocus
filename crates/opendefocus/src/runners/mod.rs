//! Render backends. The adaptive modules and the WGPU runner exist only with the `wgpu` feature.
#[cfg(feature = "wgpu")]
/// Adaptive renderer types and stages.
pub mod adaptive;
#[cfg(feature = "wgpu")]
/// Internal streaming of large source frames for the adaptive renderer.
pub(crate) mod adaptive_streaming;
#[cfg(feature = "wgpu")]
/// WGPU device, pipelines and dispatch.
mod wgpu;
#[cfg(feature = "wgpu")]
pub(crate) use wgpu::{StreamGather, StreamSource};
#[cfg(feature = "wgpu")]
pub use wgpu::{TileFeatures, WgpuRunner};
