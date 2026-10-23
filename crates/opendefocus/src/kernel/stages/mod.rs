//! Kernel stages: aperture bound construction and packed integral tree building, used by WGPU dispatch.
/// Aperture bound and layout construction for range queries.
pub mod adaptive_aperture_integral;
/// Packed integral trees for the exact Accurate gather.
pub mod adaptive_integral;
