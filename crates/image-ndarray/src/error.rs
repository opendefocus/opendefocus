//! Error type shared by the image-ndarray conversions.

use thiserror::Error;

/// Global error object for the image-ndarray crate.
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    /// The array shape is inconsistent with its element count (wraps `ndarray::ShapeError`).
    #[cfg(feature = "image")]
    #[error("NDArray had an error during initialization of shape: {0}")]
    NDArray(#[from] ndarray::ShapeError),
    /// The image container rejected the converted buffer, for example because its length is wrong.
    #[error("Image could not be constructed from ndarray")]
    ImageConstructFailed,
    /// The array's last axis length differs from the image pixel's channel count.
    #[error(
        "Image could not be constructed from ndarray because output does not match input channel count"
    )]
    ChannelMismatch,
    /// The array is not 2D or 3D, or its width or height does not fit in `u32`.
    #[error("Only 2d and 3d arrays are supported")]
    Dimensions,
}

/// Result alias for conversions that can fail with [`Error`].
#[cfg(feature = "image")]
pub type Result<T> = core::result::Result<T, Error>;
