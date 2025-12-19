#![warn(unused_extern_crates)]
#![cfg_attr(target_arch = "spirv", no_std)]
#![cfg_attr(
    target_arch = "spirv",
    allow(internal_features),
    feature(asm_experimental_arch, core_intrinsics, lang_items, repr_simd)
)]
use glam::UVec2;
mod internal_settings;
pub use crate::internal_settings::{
    AxialAberration, AxialAberrationType, ConvolveSettings, GlobalFlags, NonUniformFlags,
};
pub mod math;

#[cfg(not(any(target_arch = "spirv")))]
pub mod cpu_image;

pub const WORKGROUP_SIZE: u32 = 16;
pub const OUTPUT_CHANNELS: usize = 5;

#[derive(Copy, Clone, Debug)]
pub struct ThreadId {
    x: u32,
    y: u32,
}

impl ThreadId {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// Calculates 2D coordinates from a thread ID based on the resolution.
    pub fn get_coordinates(&self) -> UVec2 {
        UVec2::new(
            self.x,
            self.y,
        )
    }
}
