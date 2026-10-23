//! Background render workers for the adaptive renderer and the preview.
#[cfg(feature = "wgpu")]
/// Preview rendering on the adaptive pipeline.
pub(crate) mod adaptive_preview;
#[cfg(feature = "wgpu")]
/// Adaptive block rendering.
pub(crate) mod adaptive_render;
