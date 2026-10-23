//! Process-wide abort flag shared by all render workers. Long-running GPU work polls it to stop early.
use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, Ordering},
};

/// Process-wide cancellation flag shared by every render worker.
/// Stored as an `Arc` so each thread can hold a handle to the same flag.
static ABORTED: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

/// Gets the current abort status.
///
/// This can be read across threads to allow for early exits if application has abort status set
pub fn get_aborted() -> bool {
    ABORTED.load(Ordering::SeqCst)
}

/// Sets the abort status.
pub fn set_aborted(status: bool) {
    ABORTED.store(status, Ordering::SeqCst);
}
