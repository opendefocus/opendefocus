//! Executable entry point for the OpenDefocus viewer; it starts the native window and does nothing on wasm.

/// Starts the native viewer window and returns its result.
///
/// # Errors
///
/// Returns the error reported by the windowing system when the window cannot be started.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    opendefocus_ui::run_native()
}

/// Empty entry point for wasm builds, which start from the library's web entry point instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
