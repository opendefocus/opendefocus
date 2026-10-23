//! Test session for the mock host: installs the host and plugin for one test, runs actions, and exposes clips, parameters and results for assertions.

use super::*;

/// One mock host session. Holds the test serialization lock for its lifetime, resets the
/// mock state, passes a host to the plugin, and calls Unload on drop.
pub struct Session {
    _serial: MutexGuard<'static, ()>,
    /// Kept alive because the plugin holds a pointer to it until Unload.
    #[allow(
        dead_code,
        reason = "the host is never read, only kept alive for the plugin pointer"
    )]
    host: Box<OfxHost>,
}

impl Session {
    /// Starts a session whose host does not provide the suites named in `omitted`.
    pub fn new(omitted: &[&str]) -> Self {
        let serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        *STATE.lock().unwrap_or_else(PoisonError::into_inner) = Some(State::fresh());
        with(|s| {
            s.omitted = omitted
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
        });
        let mut host = Box::new(new_host());
        // SAFETY: the host lives in this session until Unload, which the drop performs.
        unsafe { (plugin().setHost)(&raw mut *host) };
        Self {
            _serial: serial,
            host,
        }
    }

    /// Calls one plugin action with the given handle and in-args.
    #[allow(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "test host: handles are only passed to the plugin"
    )]
    /// Runs one OFX action on the plugin with the given handle and arguments and returns its status.
    pub fn action(
        &self,
        action: &CStr,
        handle: *mut c_void,
        in_args: OfxPropertySetHandle,
    ) -> OfxStatus {
        // SAFETY: the action string is NUL terminated and outlives the call; the property
        // set arguments are null or valid for this call.
        unsafe { (plugin().mainEntry)(action.as_ptr(), handle, in_args, ptr::null_mut()) }
    }

    /// Runs the Load action and returns its status.
    #[must_use]
    pub fn load(&self) -> OfxStatus {
        self.action(OFX_ACTION_LOAD, ptr::null_mut(), ptr::null_mut())
    }

    /// Calls `GetClipPreferences` with a fresh out-args property set. Returns the status and
    /// the out-args property set, which `prop_string` and friends read.
    #[must_use]
    pub fn clip_preferences(&self) -> (OfxStatus, usize) {
        let out = with(State::alloc);
        let status = Self::action_with_out(
            OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES,
            self.effect(),
            ptr::null_mut(),
            out as OfxPropertySetHandle,
        );
        (status, out)
    }

    /// The parameter set handle the host returns from `getParamSet` for the effect.
    #[must_use]
    pub fn param_set(&self) -> OfxParamSetHandle {
        with(|s| s.param_set) as OfxParamSetHandle
    }

    /// Runs an action with an out-args property set and returns its status.
    pub(super) fn action_with_out(
        action: &CStr,
        handle: *mut c_void,
        in_args: OfxPropertySetHandle,
        out_args: OfxPropertySetHandle,
    ) -> OfxStatus {
        // SAFETY: as for `action`; `out_args` is a property set owned by the mock.
        unsafe { (plugin().mainEntry)(action.as_ptr(), handle, in_args, out_args) }
    }

    /// Returns the effect handle of this session as an opaque pointer.
    #[must_use]
    pub fn effect(&self) -> *mut c_void {
        with(|s| s.effect) as *mut c_void
    }

    /// Runs the Describe action on the effect and returns its status.
    #[must_use]
    pub fn describe(&self) -> OfxStatus {
        self.action(OFX_ACTION_DESCRIBE, self.effect(), ptr::null_mut())
    }

    /// Runs `DescribeInContext` for the given context name and returns its status.
    #[must_use]
    pub fn describe_in_context(&self, context: &CStr) -> OfxStatus {
        let in_args = in_args_with_context(context);
        self.action(
            OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT,
            self.effect(),
            in_args,
        )
    }

    /// Returns the property set handle of the effect.
    #[must_use]
    pub fn effect_props(&self) -> usize {
        with(|s| s.effect_props)
    }

    /// Returns the handle of the named parameter, or `None` if no such parameter was defined.
    #[must_use]
    pub fn param_handle(&self, name: &str) -> Option<OfxParamHandle> {
        param_by_name(name).map(|(handle, _)| handle as OfxParamHandle)
    }

    /// Returns the property set of the named parameter, or `None` if it was not defined.
    #[must_use]
    pub fn param_props(&self, name: &str) -> Option<usize> {
        param_by_name(name).map(|(_, props)| props)
    }

    /// The kind string passed to `paramDefine`, e.g. `OfxParamTypeDouble`.
    #[must_use]
    pub fn param_kind(&self, name: &str) -> Option<String> {
        with(|s| {
            s.params
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.kind.clone())
        })
    }

    /// Returns the property set of the named clip, or `None` if it was not defined.
    #[must_use]
    pub fn clip_props(&self, name: &str) -> Option<usize> {
        find_clip_by_name(name).map(|(_, props)| props)
    }

    /// Stores positive-rowBytes RGBA pixels for a clip at bounds `(0, 0, width, height)`.
    /// `data` holds `width * height * 4` floats, row 0 (the bottom row) first.
    /// Returns the address of the stored buffer, which `clipGetImage` hands to the plugin.
    #[must_use]
    pub fn set_clip_pixels(&self, name: &str, width: c_int, height: c_int, data: &[f32]) -> usize {
        self.set_clip_image(name, [0, 0, width, height], 4, data, false)
    }

    /// Stores an image for a clip and connects it.
    ///
    /// `logical` holds `channels` floats per pixel, row 0 being the bottom row (OFX order).
    /// With `negative_row_bytes` the memory is laid out top-down and `rowBytes` is negative.
    /// Returns the address of the physical buffer, the value `clipGetImage` will hand out.
    #[allow(
        clippy::must_use_candidate,
        reason = "the setters are called for their effect; the returned address is read only where needed"
    )]
    /// Stores a float image on the named clip with the bounds and region of definition the test provides.
    pub fn set_clip_image(
        &self,
        name: &str,
        bounds: [c_int; 4],
        channels: usize,
        logical: &[f32],
        negative_row_bytes: bool,
    ) -> usize {
        self.set_clip_image_with_rod(name, bounds, bounds, channels, logical, negative_row_bytes)
    }

    /// Like [`Session::set_clip_image`], but the region of definition is given separately
    /// from the bounds. The host reports `rod` in `clipGetImage`, as a host can do for a clip
    /// whose pixels extend beyond its region of definition.
    #[allow(
        clippy::must_use_candidate,
        reason = "the setters are called for their effect; the returned address is read only where needed"
    )]
    /// Stores a float image on the named clip with an explicit region of definition, separate from its bounds.
    pub fn set_clip_image_with_rod(
        &self,
        name: &str,
        bounds: [c_int; 4],
        rod: [c_int; 4],
        channels: usize,
        logical: &[f32],
        negative_row_bytes: bool,
    ) -> usize {
        self.set_clip_image_strided(name, bounds, rod, channels, logical, negative_row_bytes, 0)
    }

    /// Like [`Session::set_clip_image_with_rod`], with `pad` NaN floats after every row in
    /// memory, so `rowBytes` is larger than the row. The plugin must address rows by rowBytes.
    ///
    /// # Panics
    ///
    /// Panics if `logical` does not hold `width * height * channels` floats for `bounds`.
    #[allow(
        clippy::must_use_candidate,
        reason = "the setters are called for their effect; the returned address is read only where needed"
    )]
    /// Stores a float image on the named clip with a row stride, so tests can check padded rows.
    pub fn set_clip_image_strided(
        &self,
        name: &str,
        bounds: [c_int; 4],
        rod: [c_int; 4],
        channels: usize,
        logical: &[f32],
        negative_row_bytes: bool,
        pad: usize,
    ) -> usize {
        let width = (bounds[2] - bounds[0]) as usize;
        let height = (bounds[3] - bounds[1]) as usize;
        assert_eq!(logical.len(), width * height * channels);
        let row_floats = width * channels;
        let stride = row_floats + pad;
        // Memory row m holds OFX row height-1-m when rowBytes is negative, else OFX row m.
        let mut physical = vec![f32::NAN; stride * height];
        for m in 0..height {
            let ofx_row = if negative_row_bytes {
                height - 1 - m
            } else {
                m
            };
            physical[m * stride..m * stride + row_floats]
                .copy_from_slice(&logical[ofx_row * row_floats..(ofx_row + 1) * row_floats]);
        }
        let address = physical.as_ptr() as usize;
        let props = with(|s| {
            let clip = s
                .clips
                .iter_mut()
                .find(|c| c.name == name)
                .expect("clip is defined by Describe in context");
            clip.image = Some(ClipImage {
                physical,
                bounds,
                rod,
                channels,
                negative_row_bytes,
                stride,
            });
            clip.props
        });
        store(
            (
                props,
                OFX_IMAGE_CLIP_PROP_CONNECTED.to_string_lossy().into_owned(),
            ),
            0,
            Value::Int(1),
        );
        address
    }

    /// The clip's pixels in OFX order (row 0 bottom), read back from the host memory.
    ///
    /// # Panics
    ///
    /// Panics if no clip named `name` was defined by `DescribeInContext`.
    #[must_use]
    pub fn clip_logical(&self, name: &str) -> Vec<f32> {
        with(|s| {
            let clip = s
                .clips
                .iter()
                .find(|c| c.name == name)
                .expect("clip is defined by Describe in context");
            let image = clip.image.as_ref().expect("clip has an image");
            let h = image.height();
            let row_floats = image.row_floats();
            let mut out = Vec::with_capacity(row_floats * h);
            for ofx_row in 0..h {
                let m = if image.negative_row_bytes {
                    h - 1 - ofx_row
                } else {
                    ofx_row
                };
                out.extend_from_slice(
                    &image.physical[m * image.stride..m * image.stride + row_floats],
                );
            }
            out
        })
    }

    /// Sets the render scale and the clips' pixel aspect ratio for later actions, as a host
    /// does when it renders a proxy (scale below 1) or a non-square-pixel clip.
    pub fn set_render_scale(&self, scale: f64, par: f64) {
        let clip_props: Vec<usize> = with(|s| {
            s.render_scale = [scale, scale];
            s.pixel_aspect = par;
            s.clips.iter().map(|c| c.props).collect()
        });
        for props in clip_props {
            store(
                (
                    props,
                    OFX_IMAGE_PROP_PIXEL_ASPECT_RATIO
                        .to_string_lossy()
                        .into_owned(),
                ),
                0,
                Value::Double(par),
            );
        }
    }

    /// Overrides a parameter's value for later reads. The type must match the parameter.
    ///
    /// # Panics
    ///
    /// Panics if the parameter is not defined by the session.
    pub fn set_param_value(&self, name: &str, value: Value) {
        let props = self.param_props(name).expect("parameter exists");
        store((props, MOCK_VALUE.to_string()), 0, value);
    }

    /// Calls Render with a render window (x1, y1, x2, y2) and a time.
    #[must_use]
    pub fn render(&self, window: [c_int; 4], time: f64) -> OfxStatus {
        self.action(
            OFX_IMAGE_EFFECT_ACTION_RENDER,
            self.effect(),
            render_args(window, time),
        )
    }

    /// Calls `GetRegionsOfInterest`. `roi` is the requested canonical region (None sends null
    /// in-args). Returns the status and the out-args property set, which `prop_double` reads
    /// (`OfxImageClipPropRoI_<clip>`).
    #[must_use]
    pub fn regions_of_interest(&self, roi: Option<[f64; 4]>, time: f64) -> (OfxStatus, usize) {
        let out = with(State::alloc);
        let in_args = match roi {
            Some(roi) => roi_args(roi, time),
            None => ptr::null_mut(),
        };
        let status = Self::action_with_out(
            OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST,
            self.effect(),
            in_args,
            out as OfxPropertySetHandle,
        );
        (status, out)
    }

    /// Calls Render with a null in-args property set.
    #[must_use]
    pub fn render_without_args(&self) -> OfxStatus {
        self.action(
            OFX_IMAGE_EFFECT_ACTION_RENDER,
            self.effect(),
            ptr::null_mut(),
        )
    }

    /// Calls `CreateInstance` on the effect.
    #[must_use]
    pub fn create_instance(&self) -> OfxStatus {
        self.action(OFX_ACTION_CREATE_INSTANCE, self.effect(), ptr::null_mut())
    }

    /// Calls `DestroyInstance` on the effect.
    #[must_use]
    pub fn destroy_instance(&self) -> OfxStatus {
        self.action(OFX_ACTION_DESTROY_INSTANCE, self.effect(), ptr::null_mut())
    }

    /// The instance data pointer stored on the effect property set, if any.
    #[must_use]
    pub fn instance_data(&self) -> Option<usize> {
        prop_pointer(self.effect_props(), OFX_PROP_INSTANCE_DATA, 0)
    }

    /// Starts a session whose host provides the Nuke camera suite, with `camera` connected or not.
    #[must_use]
    pub fn with_nuke_camera(camera: MockCamera) -> Self {
        let session = Session::new(&[]);
        let props = with(|s| {
            let handle = s.alloc();
            let props = s.alloc();
            s.nuke_camera = Some(CameraRec {
                handle,
                props,
                connected: camera.connected,
                focal: camera.focal,
                haperture: camera.haperture,
                vaperture: camera.vaperture,
            });
            props
        });
        // The host reports the connection in the camera's property set.
        store(
            (
                props,
                OFX_IMAGE_CLIP_PROP_CONNECTED.to_string_lossy().into_owned(),
            ),
            0,
            Value::Int(c_int::from(camera.connected)),
        );
        session
    }

    /// The mock camera's property set, where the optional flag and the connection are stored.
    #[must_use]
    pub fn camera_props(&self) -> Option<usize> {
        with(|s| s.nuke_camera.as_ref().map(|c| c.props))
    }

    /// Names of the camera inputs the plugin defined in `DescribeInContext`.
    #[must_use]
    pub fn camera_inputs(&self) -> Vec<String> {
        with(|s| s.camera_inputs.clone())
    }

    /// The render settings the plugin reads for this session at `time`, through the same code
    /// Render uses. Needs Load and `DescribeInContext` first; no GPU is involved.
    ///
    /// # Errors
    ///
    /// Returns the plugin's non-OK status as an error when the settings cannot be read.
    pub fn settings(&self, time: f64) -> opendefocus_ofx::suites::Result<Settings> {
        let args = render_args([0; 4], time);
        // SAFETY: the effect and in-args are this session's handles, and the plugin is loaded.
        unsafe { effect_settings(self.effect().cast(), args) }
    }

    /// Number of images handed out by clipGetImage so far.
    #[must_use]
    pub fn images_fetched(&self) -> usize {
        with(|s| s.images_fetched)
    }

    /// Returns the last message the plugin sent through the message suite as its (type, text) pair.
    #[must_use]
    pub fn last_message(&self) -> Option<(String, String)> {
        with(|s| s.last_message.clone())
    }

    /// Returns how many clip images the plugin has released in this session.
    #[must_use]
    pub fn images_released(&self) -> usize {
        with(|s| s.images_released)
    }
}

impl Session {
    /// Arms the host to fail one call: the call with index `fault.0`, counted from this call,
    /// returns `fault.1`. `None` disarms it. Counting restarts at zero.
    pub fn arm_fault(&self, fault: Option<(usize, OfxStatus)>) {
        with(|s| {
            s.fault = fault;
            s.calls = 0;
        });
    }

    /// Number of fault-aware host calls since the last [`Session::arm_fault`].
    #[must_use]
    pub fn host_calls(&self) -> usize {
        with(|s| s.calls)
    }

    /// Makes the host ask the effect to abort from now on.
    pub fn request_abort(&self) {
        with(|s| s.abort_requested = true);
    }

    /// Makes every clip image report `OfxBitDepthByte` from now on.
    pub fn use_byte_depth(&self) {
        with(|s| s.byte_depth = true);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Unload clears the plugin's host and suites; the host pointer is then dropped.
        self.action(OFX_ACTION_UNLOAD, ptr::null_mut(), ptr::null_mut());
    }
}

/// Stores the render scale (two doubles) in an in-args property set.
pub(super) fn store_render_scale(props: usize) {
    let scale = with(|s| s.render_scale);
    for (i, value) in scale.iter().enumerate() {
        store(
            (
                props,
                OFX_IMAGE_EFFECT_PROP_RENDER_SCALE
                    .to_string_lossy()
                    .into_owned(),
            ),
            i as c_int,
            Value::Double(*value),
        );
    }
}

/// In-args for Render: the render window as four ints and the time as a double.
pub(super) fn render_args(window: [c_int; 4], time: f64) -> OfxPropertySetHandle {
    let props = with(State::alloc);
    store_render_scale(props);
    for (i, value) in window.iter().enumerate() {
        store(
            (
                props,
                OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW
                    .to_string_lossy()
                    .into_owned(),
            ),
            i as c_int,
            Value::Int(*value),
        );
    }
    store(
        (props, OFX_PROP_TIME.to_string_lossy().into_owned()),
        0,
        Value::Double(time),
    );
    props as OfxPropertySetHandle
}

/// In-args for `GetRegionsOfInterest`: the requested region (four doubles) and the time.
pub(super) fn roi_args(roi: [f64; 4], time: f64) -> OfxPropertySetHandle {
    let props = with(State::alloc);
    store_render_scale(props);
    for (i, value) in roi.iter().enumerate() {
        store(
            (
                props,
                OFX_IMAGE_EFFECT_PROP_REGION_OF_INTEREST
                    .to_string_lossy()
                    .into_owned(),
            ),
            i as c_int,
            Value::Double(*value),
        );
    }
    store(
        (props, OFX_PROP_TIME.to_string_lossy().into_owned()),
        0,
        Value::Double(time),
    );
    props as OfxPropertySetHandle
}

/// Builds an in-args property set that names the given image-effect context.
pub(super) fn in_args_with_context(context: &CStr) -> OfxPropertySetHandle {
    let props = with(State::alloc);
    let context = context.to_owned();
    store(
        (
            props,
            OFX_IMAGE_EFFECT_PROP_CONTEXT.to_string_lossy().into_owned(),
        ),
        0,
        Value::Str(context),
    );
    props as OfxPropertySetHandle
}

/// Returns the key the mock stores a named property under for a property set.
pub(super) fn key_for(props: usize, name: &CStr) -> (usize, String) {
    (props, name.to_string_lossy().into_owned())
}

/// Reads a string property value, if set at `index`.
#[must_use]
pub fn prop_string(props: usize, name: &CStr, index: c_int) -> Option<String> {
    read(&key_for(props, name), index, |v| match v {
        Value::Str(s) => Some(s.to_string_lossy().into_owned()),
        _ => None,
    })
    .ok()
}

/// Reads an int property value, if set at `index`.
#[must_use]
pub fn prop_int(props: usize, name: &CStr, index: c_int) -> Option<c_int> {
    read(&key_for(props, name), index, |v| match v {
        Value::Int(i) => Some(*i),
        _ => None,
    })
    .ok()
}

/// Reads a double property value, if set at `index`.
#[must_use]
pub fn prop_double(props: usize, name: &CStr, index: c_int) -> Option<f64> {
    read(&key_for(props, name), index, |v| match v {
        Value::Double(d) => Some(*d),
        _ => None,
    })
    .ok()
}

/// Reads a pointer property value, if set at `index`.
#[must_use]
pub fn prop_pointer(props: usize, name: &CStr, index: c_int) -> Option<usize> {
    read(&key_for(props, name), index, |v| match v {
        Value::Ptr(p) => Some(*p),
        _ => None,
    })
    .ok()
}

/// Number of values stored under a property name.
#[must_use]
pub fn prop_count(props: usize, name: &CStr) -> usize {
    with(|s| s.props.get(&key_for(props, name)).map_or(0, Vec::len))
}

/// In-args for `InstanceChanged` whose `kOfxPropType` is `kind`, as the host sends them.
pub(super) fn change_args(kind: &str) -> OfxPropertySetHandle {
    let props = with(State::alloc);
    store(
        (props, OFX_PROP_TYPE.to_string_lossy().into_owned()),
        0,
        Value::Str(CString::new(kind).expect("no NUL")),
    );
    props as OfxPropertySetHandle
}

/// In-args carrying only the render time, so a request can fail on a later property read.
pub(super) fn time_args(time: f64) -> OfxPropertySetHandle {
    let props = with(State::alloc);
    store(
        (props, OFX_PROP_TIME.to_string_lossy().into_owned()),
        0,
        Value::Double(time),
    );
    props as OfxPropertySetHandle
}
