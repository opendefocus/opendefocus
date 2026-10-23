//! Render preparation helpers: filter images (`filter`) and image resizing and mip chains (`resize`).
//! The focal plane setup module is compiled only for tests.
/// Filter image preparation for the adaptive renderer.
pub(crate) mod filter;
#[cfg(test)]
pub mod focal_plane_setup;
/// Image resizing and mip-chain generation.
pub mod resize;
