//! Process-wide state of the mock host: stored property values, parameter records, clip images, and the lock that serialises tests.

use super::*;

/// A stored property value. `Unset` fills holes left by a set at a higher index.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unset,
    Str(CString),
    Int(c_int),
    Double(f64),
    Ptr(usize),
    /// A string property whose host pointer is null, which the plugin reads as an empty string.
    NullStr,
}

pub(super) struct ParamRec {
    pub(super) name: String,
    pub(super) kind: String,
    pub(super) handle: usize,
    pub(super) props: usize,
}

pub(super) struct Clip {
    pub(super) name: String,
    pub(super) handle: usize,
    pub(super) props: usize,
    pub(super) image: Option<ClipImage>,
}

/// An image as the mock host stores it. `physical` is the memory the plugin reads and writes.
pub(super) struct ClipImage {
    pub(super) physical: Vec<f32>,
    pub(super) bounds: [c_int; 4],
    /// Region of definition reported by `clipGetImage`. It may differ from `bounds`.
    pub(super) rod: [c_int; 4],
    pub(super) channels: usize,
    /// When true the memory is top-down: `data` addresses the bottom row and `rowBytes` is
    /// negative, which is the layout the spec allows for hosts with a top-down native order.
    pub(super) negative_row_bytes: bool,
    /// Floats from one row start to the next in memory. Larger than `row_floats` when the host
    /// pads its rows; the padding holds NaN, so a plugin that reads it produces non-finite output.
    pub(super) stride: usize,
}

impl ClipImage {
    pub(super) fn width(&self) -> usize {
        (self.bounds[2] - self.bounds[0]) as usize
    }

    pub(super) fn height(&self) -> usize {
        (self.bounds[3] - self.bounds[1]) as usize
    }

    pub(super) fn row_floats(&self) -> usize {
        self.width() * self.channels
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) next: usize,
    pub(super) props: HashMap<(usize, String), Vec<Value>>,
    pub(super) effect: usize,
    pub(super) effect_props: usize,
    pub(super) param_set: usize,
    pub(super) param_set_props: usize,
    pub(super) params: Vec<ParamRec>,
    pub(super) clips: Vec<Clip>,
    pub(super) omitted: Vec<String>,
    pub(super) last_message: Option<(String, String)>,
    pub(super) images_released: usize,
    pub(super) images_fetched: usize,
    /// The mock Nuke camera, when the session was started with one. Its suite is offered only then.
    pub(super) nuke_camera: Option<CameraRec>,
    /// Names passed to `cameraDefine`, in order.
    pub(super) camera_inputs: Vec<String>,
    /// Render scale (x, y) in the in-args of Render and `GetRegionsOfInterest`.
    pub(super) render_scale: [f64; 2],
    /// Pixel aspect ratio of the clips. Canonical x = pixel x * par / scale x.
    pub(super) pixel_aspect: f64,
    /// Host call index that fails, and the status it returns. Counted from the last arming.
    pub(super) fault: Option<(usize, OfxStatus)>,
    /// Calls to fault-aware host functions since the last arming.
    pub(super) calls: usize,
    /// When set, `abort` asks the effect to stop rendering.
    pub(super) abort_requested: bool,
    /// When set, clip images report `OfxBitDepthByte` instead of float.
    pub(super) byte_depth: bool,
}

impl State {
    pub(super) fn fresh() -> Self {
        let mut s = Self {
            render_scale: [1.0; 2],
            pixel_aspect: 1.0,
            ..Self::default()
        };
        s.effect = s.alloc();
        s.effect_props = s.alloc();
        s.param_set = s.alloc();
        s.param_set_props = s.alloc();
        s
    }

    pub(super) fn alloc(&mut self) -> usize {
        self.next += 16;
        self.next
    }
}

pub(super) static STATE: Mutex<Option<State>> = Mutex::new(None);
pub(super) static SERIAL: Mutex<()> = Mutex::new(());

/// Counts one call to a fault-aware host function and returns the status the test injected for
/// this call, if it is the one chosen.
pub(super) fn injected_fault() -> Option<OfxStatus> {
    with(|s| {
        let index = s.calls;
        s.calls += 1;
        s.fault
            .filter(|(at, _)| *at == index)
            .map(|(_, status)| status)
    })
}

pub(super) fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(State::fresh))
}

pub(super) fn plugin() -> &'static OfxPlugin {
    // SAFETY: `OfxGetPlugin(0)` returns the static descriptor, which lives for the program.
    unsafe { &*OfxGetPlugin(0) }
}

/// Builds a property key from a handle and a NUL-terminated name.
///
/// # Safety
///
/// `name` must be a valid NUL-terminated string.
pub(super) unsafe fn key_of(props: usize, name: *const c_char) -> (usize, String) {
    // SAFETY: forwarded from this function's contract.
    let name = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    (props, name)
}

pub(super) fn store(key: (usize, String), index: c_int, value: Value) -> OfxStatus {
    if key.0 == 0 {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    let Ok(i) = usize::try_from(index) else {
        return OFX_STAT_ERR_BAD_INDEX;
    };
    with(|s| {
        let values = s.props.entry(key).or_default();
        if values.len() <= i {
            values.resize(i + 1, Value::Unset);
        }
        values[i] = value;
    });
    OFX_STAT_OK
}

pub(super) fn read<T>(
    key: &(usize, String),
    index: c_int,
    pick: impl FnOnce(&Value) -> Option<T>,
) -> Result<T, OfxStatus> {
    let Ok(i) = usize::try_from(index) else {
        return Err(OFX_STAT_ERR_BAD_INDEX);
    };
    with(|s| {
        let values = s.props.get(key).ok_or(OFX_STAT_ERR_UNKNOWN)?;
        let value = values
            .get(i)
            .filter(|v| !matches!(v, Value::Unset))
            .ok_or(OFX_STAT_ERR_BAD_INDEX)?;
        pick(value).ok_or(OFX_STAT_ERR_VALUE)
    })
}

/// Writes `value` through a caller's out pointer.
///
/// # Safety
///
/// A non-null `out` must be valid for writes of `T`.
pub(super) unsafe fn put<T>(out: *mut T, value: T) -> OfxStatus {
    if out.is_null() {
        return OFX_STAT_ERR_VALUE;
    }
    // SAFETY: the caller passes a valid out pointer for a `T`.
    unsafe { out.write(value) };
    OFX_STAT_OK
}
