//! Stored properties, clips and placeholder entries of the parity-scene host, used by the parity harness.

use super::*;

/// A property value as the mock stores it.
#[derive(Clone)]
pub(super) enum Stored {
    Int(c_int),
    Double(f64),
    Ptr(usize),
    Str(CString),
}

/// One clip of the effect. `pixels` is bottom-up (OFX row 0 first) and empty when unconnected.
pub(super) struct Clip {
    pub(super) name: &'static str,
    pub(super) handle: usize,
    pub(super) props: usize,
    pub(super) connected: bool,
    pub(super) pixels: Vec<f32>,
    pub(super) width: c_int,
    pub(super) height: c_int,
}

#[derive(Default)]
pub(super) struct Host {
    pub(super) next: usize,
    pub(super) props: HashMap<(usize, String, c_int), Stored>,
    pub(super) effect: usize,
    pub(super) effect_props: usize,
    pub(super) param_set: usize,
    pub(super) param_handles: HashMap<usize, &'static str>,
    pub(super) values: HashMap<&'static str, ParamValue>,
    pub(super) clips: Vec<Clip>,
}

pub(super) static HOST: Mutex<Option<Host>> = Mutex::new(None);

pub(super) fn with<R>(f: impl FnOnce(&mut Host) -> R) -> R {
    let mut guard = HOST.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(Host::default))
}

impl Host {
    pub(super) fn alloc(&mut self) -> usize {
        self.next += 16;
        self.next
    }

    pub(super) fn store(
        &mut self,
        props: usize,
        name: String,
        index: c_int,
        value: Stored,
    ) -> OfxStatus {
        if props == 0 {
            return OFX_STAT_ERR_BAD_HANDLE;
        }
        self.props.insert((props, name, index), value);
        OFX_STAT_OK
    }

    pub(super) fn fetch(&self, props: usize, name: &str, index: c_int) -> Option<&Stored> {
        self.props.get(&(props, name.to_owned(), index))
    }

    pub(super) fn clip_named(&self, name: &str) -> &Clip {
        self.clips
            .iter()
            .find(|clip| clip.name == name)
            .expect("clip exists")
    }
}

/// Copies a host-provided property name.
///
/// # Safety
///
/// `name` must be null or a NUL-terminated string that lives for the call.
pub(super) unsafe fn name_of(name: *const c_char) -> String {
    if name.is_null() {
        return String::new();
    }
    // SAFETY: the plugin passes NUL-terminated property names, per this function's contract.
    unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned()
}

/// Stores an empty placeholder for a suite entry this host does not use. Calling it returns
/// `kOfxStatErrUnknown`, and the plugin never calls the entries that are stubbed here.
pub(super) fn stub<F: Copy>() -> F {
    unsafe extern "C" fn unimplemented() -> OfxStatus {
        OFX_STAT_ERR_UNKNOWN
    }
    let f: unsafe extern "C" fn() -> OfxStatus = unimplemented;
    // SAFETY: function pointers share one size, and the stub reads no arguments.
    unsafe { std::mem::transmute_copy::<unsafe extern "C" fn() -> OfxStatus, F>(&f) }
}
