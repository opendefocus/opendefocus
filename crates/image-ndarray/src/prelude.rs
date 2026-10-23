//! Re-exports of the traits needed for image-array conversions.

pub use crate::traits::NormalizedFloat;

#[cfg(all(feature = "image", feature = "std"))]
pub use crate::traits::ImageArray;
