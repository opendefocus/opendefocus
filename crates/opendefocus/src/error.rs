//! Error type returned by the public API and the `Result` alias built on it.
use thiserror::Error;

/// Errors that can occur during `OpenDefocus` operations.
///
/// Each variant names one specific failure, so callers can match on the cause
/// instead of parsing messages.
#[derive(Debug, Error)]
pub enum Error {
    /// The array to resize has an empty height, width or channel axis.
    #[error("Resize source must have non-zero height, width and channels")]
    ResizeEmptySource,
    /// The requested resize output has a zero width or height.
    #[error("Resize target resolution must be non-zero")]
    ResizeEmptyTarget,
    /// No compatible GPU adapter could be selected.
    #[error("No suitable GPU adapter found")]
    NoAdapter,
    /// Adaptive rendering needs the `wgpu` feature, which this build does not provide.
    #[error("The adaptive renderer requires the wgpu feature")]
    AdaptiveWgpuRequired,
    /// The adaptive classifier was used before it was initialized.
    #[cfg(feature = "wgpu")]
    #[error("Adaptive classifier was not initialized; construct an adaptive renderer first")]
    AdaptiveClassifierUnavailable,
    /// A CoC value fed to the classifier is non-finite or outside the representable range.
    #[cfg(feature = "wgpu")]
    #[error("Adaptive classifier input contains a non-finite or out-of-range CoC value")]
    AdaptiveCocOutOfRange,
    /// The adaptive GPU operation was cancelled before completion.
    #[cfg(feature = "wgpu")]
    #[error("Adaptive GPU operation was cancelled")]
    AdaptiveCancelled,
    /// The adaptive settings combination is not supported; the payload names the rejected setting.
    #[cfg(feature = "wgpu")]
    #[error("Unsupported adaptive settings: {0}")]
    AdaptiveSettingsUnsupported(&'static str),
    /// No filter image was supplied although the `image` filter mode is selected.
    #[error("No filter image provided but the image filter mode is selected")]
    NoFilterProvided,
    /// Focal plane setup is only defined for depth-based rendering, not 2D.
    #[error("Focal plane setup is only available for depth-based operations, not 2D")]
    FocalPlaneOverlayWhile2D,
    /// A depth map is required for the selected defocus mode but none was given.
    #[error("Depth is required for this defocus mode but none was provided")]
    DepthNotFound,
    /// The selected render mode needs an aperture image but none was given.
    #[error("Aperture image is required for this render but none was provided")]
    MissingAperture,
    /// The selected render mode needs lens settings but none were given.
    #[error("Lens settings are required for this render but none were provided")]
    MissingLensSettings,
    /// A render parameter is non-finite, negative or zero where a positive value is required.
    #[error("Render parameter is non-finite, negative or zero where a positive value is required")]
    InvalidParameter,
    /// The image, depth or region dimensions are zero, mismatched or outside the full region.
    #[error("Image, depth or region dimensions are invalid or inconsistent")]
    InvalidDimensions,
    /// The image or buffer size exceeds what the GPU or the index types support.
    #[error("Image or buffer size exceeds what the GPU or the index types support")]
    SizeLimitExceeded,
    /// The array is not in standard contiguous layout, so its data cannot be uploaded directly.
    #[error("Array must be in standard contiguous layout")]
    NonContiguousArray,
    /// The mipmap buffer is empty, so no pyramid can be built.
    #[error("Mipmap buffer is empty and cannot be constructed")]
    MipmapEmpty,
    /// The input channel count is outside the range the renderer supports.
    #[error("Input channel count is outside the supported range")]
    InvalidChannelCount,
    /// An I/O error, converted from `std::io::Error`.
    #[error(transparent)]
    #[allow(
        clippy::enum_variant_names,
        reason = "renaming the public variant would change the API"
    )]
    IOError(#[from] std::io::Error),
    /// An ndarray shape error, converted from `ndarray::ShapeError`.
    #[error(transparent)]
    Shape(#[from] ndarray::ShapeError),
    /// A cancelled oneshot channel, converted from `futures_channel::oneshot::Canceled`.
    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    Canceled(#[from] futures_channel::oneshot::Canceled),
    /// A GPU buffer mapping failed, converted from `wgpu::BufferAsyncError`.
    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    BufferAsync(#[from] wgpu::BufferAsyncError),
    /// A GPU buffer map range was invalid, converted from `wgpu::MapRangeError`.
    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    MapRange(#[from] wgpu::MapRangeError),
    /// Waiting for submitted GPU work failed, converted from `wgpu::PollError`.
    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuPoll(#[from] wgpu::PollError),
    /// The wgpu device request failed, converted from `wgpu::RequestDeviceError`.
    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuRequestDevice(#[from] wgpu::RequestDeviceError),
}

/// Crate result type with [`Error`] as the error.
pub type Result<T> = core::result::Result<T, Error>;
