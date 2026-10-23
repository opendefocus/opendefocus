//! Mock memory, multi-thread, mutex and message suites, and the recorder that keeps error messages for the tests to read.

use super::*;

/// Memory suite stub: allocation always fails, so plugin code must not rely on it.
pub(super) unsafe extern "C" fn memory_alloc_stub(
    _handle: *mut c_void,
    _n: usize,
    out: *mut *mut c_void,
) -> OfxStatus {
    // SAFETY: a valid out pointer per the host contract.
    unsafe { put(out, ptr::null_mut()) };
    OFX_STAT_ERR_MEMORY
}

/// Stub for the memory free entry point; it accepts the call and does nothing.
pub(super) unsafe extern "C" fn memory_free_stub(_data: *mut c_void) -> OfxStatus {
    OFX_STAT_OK
}

/// Returns the mock memory suite table, built once and shared.
pub(super) fn memory_suite() -> &'static OfxMemorySuiteV1 {
    static SUITE: OnceLock<OfxMemorySuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxMemorySuiteV1 {
        memoryAlloc: memory_alloc_stub,
        memoryFree: memory_free_stub,
    })
}

/// Runs the thread function once on the calling thread.
pub(super) unsafe extern "C" fn multi_thread(
    func: unsafe extern "C" fn(c_uint, c_uint, *mut c_void),
    n_threads: c_uint,
    custom: *mut c_void,
) -> OfxStatus {
    if n_threads > 0 {
        // SAFETY: the plugin passes a valid thread function and argument for this call.
        unsafe { func(0, 1, custom) };
    }
    OFX_STAT_OK
}

/// Writes a fixed CPU count for the mock host.
pub(super) unsafe extern "C" fn multi_thread_num_cpus(out: *mut c_uint) -> OfxStatus {
    // SAFETY: a valid out pointer per the host contract.
    unsafe { put(out, 1) }
}

/// Writes thread index zero, as the calling thread is not a spawned worker.
pub(super) unsafe extern "C" fn multi_thread_index(out: *mut c_uint) -> OfxStatus {
    // SAFETY: a valid out pointer per the host contract.
    unsafe { put(out, 0) }
}

/// Reports that the calling thread is not a spawned worker.
pub(super) unsafe extern "C" fn multi_thread_is_spawned() -> c_int {
    0
}

/// Creates a mock mutex and stores its handle for the caller.
pub(super) unsafe extern "C" fn mutex_create(
    out: *mut *mut OfxMutex,
    _lock_count: c_int,
) -> OfxStatus {
    // SAFETY: a valid out pointer per the host contract. The mutex is never dereferenced.
    unsafe { put(out, NonNull::<OfxMutex>::dangling().as_ptr()) }
}

/// Returns the mock multi-thread suite table, built once and shared.
pub(super) fn multi_thread_suite() -> &'static OfxMultiThreadSuiteV1 {
    static SUITE: OnceLock<OfxMultiThreadSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxMultiThreadSuiteV1 {
        multiThread: multi_thread,
        multiThreadNumCPUs: multi_thread_num_cpus,
        multiThreadIndex: multi_thread_index,
        multiThreadIsSpawnedThread: multi_thread_is_spawned,
        mutexCreate: mutex_create,
        mutexDestroy: mutex_destroy,
        mutexLock: mutex_lock,
        mutexUnLock: mutex_unlock,
        mutexTryLock: mutex_try_lock,
    })
}

/// Records one message: the format is what the caller passed, and for `%s` the text.
///
/// # Safety
///
/// `format` must be a valid NUL-terminated string; `text` a valid string when the format is `%s`.
pub(super) unsafe fn record_message(
    kind: *const c_char,
    format: *const c_char,
    text: *const c_char,
) -> OfxStatus {
    // SAFETY: forwarded from this function's contract.
    let (kind, format) = unsafe {
        (
            CStr::from_ptr(kind).to_string_lossy().into_owned(),
            CStr::from_ptr(format).to_string_lossy().into_owned(),
        )
    };
    let body = if format == "%s" && !text.is_null() {
        // SAFETY: the caller's `%s` consumes one NUL-terminated string.
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    } else {
        format
    };
    with(|s| s.last_message = Some((kind, body)));
    OFX_STAT_OK
}

/// Message receiver. The variadic text is the fifth argument.
#[cfg(not(all(target_vendor = "apple", target_arch = "aarch64")))]
pub(super) unsafe extern "C" fn message_receiver(
    _handle: *mut c_void,
    kind: *const c_char,
    _id: *const c_char,
    format: *const c_char,
    text: *const c_char,
) -> OfxStatus {
    // SAFETY: forwarded from the host contract.
    unsafe { record_message(kind, format, text) }
}

/// Apple arm64 receiver: `text` is the first stack argument after `x0..x3` and `x4..x7`.
#[cfg(all(target_vendor = "apple", target_arch = "aarch64"))]
pub(super) unsafe extern "C" fn message_receiver(
    _handle: *mut c_void,
    kind: *const c_char,
    _id: *const c_char,
    format: *const c_char,
    _x4: usize,
    _x5: usize,
    _x6: usize,
    _x7: usize,
    text: *const c_char,
) -> OfxStatus {
    // SAFETY: forwarded from the host contract.
    unsafe { record_message(kind, format, text) }
}

/// Returns the mock message suite table, built once and shared.
pub(super) fn message_suite() -> &'static OfxMessageSuiteV1 {
    static SUITE: OnceLock<OfxMessageSuiteV1> = OnceLock::new();
    SUITE.get_or_init(|| OfxMessageSuiteV1 {
        // SAFETY: the receiver matches the variadic slot's trailing argument, see the
        // module documentation; `transmute` changes only the declared variadic-ness.
        message: unsafe { transmute::<*const (), _>(message_receiver as *const ()) },
    })
}
