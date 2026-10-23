//! Failure type for render actions. Separates an abort requested by the host from an error message
//! that is reported back to the host.

use super::{CString, OfxError, OfxImageEffectHandle, message_error};

/// Why a render stopped. Only `Message` is reported to the host.
pub(super) enum Failure {
    /// The host asked the render to abort; returned without a message.
    Aborted,
    /// The message to report to the host, describing what failed.
    Message(String),
}

impl From<OfxError> for Failure {
    fn from(error: OfxError) -> Self {
        Failure::Message(format!("OFX host call failed: {error}"))
    }
}

/// Result of a render step; the error side says whether the render failed or was aborted.
pub(super) type Outcome<T> = Result<T, Failure>;

/// Returns an `Err` carrying `message`. Used where a condition is an error, not a host abort.
pub(super) fn fail<T>(message: impl Into<String>) -> Outcome<T> {
    Err(Failure::Message(message.into()))
}
/// Reports a render failure through the message suite and stderr.
pub(super) fn report(effect: OfxImageEffectHandle, text: &str) {
    crate::suites::diagnostic(text);
    if let Ok(text) = CString::new(text.replace('\0', " ")) {
        // The message suite is optional; without it the stderr line above is the report.
        message_error(effect, &text).ok();
    }
}
/// Adds the render's diagnostics to a message failure.
pub(super) fn with_context(failure: Failure, context: &str) -> Failure {
    match failure {
        Failure::Message(text) => Failure::Message(format!("{text} {context}")),
        Failure::Aborted => Failure::Aborted,
    }
}
