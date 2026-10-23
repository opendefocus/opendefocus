//! Harness for the parity comparison: installs the mock host, renders one scene through the plugin, and compares the result with its baseline.

use super::*;

pub(super) unsafe extern "C" fn fetch_suite(
    _host: OfxPropertySetHandle,
    name: *const c_char,
    version: c_int,
) -> *const c_void {
    if name.is_null() || version != 1 {
        return ptr::null();
    }
    // SAFETY: the plugin passes a NUL-terminated suite name.
    let name = unsafe { CStr::from_ptr(name) };
    if name == OFX_PROPERTY_SUITE {
        std::ptr::from_ref::<OfxPropertySuiteV1>(property_suite()).cast()
    } else if name == OFX_IMAGE_EFFECT_SUITE {
        std::ptr::from_ref::<OfxImageEffectSuiteV1>(image_effect_suite()).cast()
    } else if name == OFX_PARAMETER_SUITE {
        std::ptr::from_ref::<OfxParameterSuiteV1>(parameter_suite()).cast()
    } else {
        // Memory, multi-thread and message suites are optional; the host does not offer them.
        ptr::null()
    }
}

/// Builds the host state for one entry: effect, parameter values, clips and in-args.
/// Returns the effect handle and the in-args property set.
pub(super) fn install_host(
    values: HashMap<&'static str, ParamValue>,
    color: &Frame,
    depth: Option<&Frame>,
) -> (usize, usize) {
    with(|h| {
        *h = Host::default();
        h.effect = h.alloc();
        h.effect_props = h.alloc();
        h.param_set = h.alloc();
        h.values = values;
        let mut clip = |name: &'static str, frame: Option<&Frame>, out: bool| {
            let handle = h.alloc();
            let props = h.alloc();
            let (pixels, width, height, connected) = match frame {
                Some(frame) => (frame.bottom_up(), frame.width, frame.height, true),
                None if out => (vec![0.0; color.rows.len()], color.width, color.height, true),
                None => (Vec::new(), color.width, color.height, false),
            };
            h.clips.push(Clip {
                name,
                handle,
                props,
                connected,
                pixels,
                width,
                height,
            });
            h.store(
                props,
                OFX_IMAGE_CLIP_PROP_CONNECTED.to_string_lossy().into_owned(),
                0,
                Stored::Int(c_int::from(connected)),
            );
        };
        clip("Source", Some(color), false);
        clip("Depth", depth, false);
        clip("Output", None, true);
        let in_args = h.alloc();
        h.store(
            in_args,
            OFX_PROP_TIME.to_string_lossy().into_owned(),
            0,
            Stored::Double(0.0),
        );
        for (i, value) in [0, 0, color.width, color.height].into_iter().enumerate() {
            h.store(
                in_args,
                OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW
                    .to_string_lossy()
                    .into_owned(),
                c_int::try_from(i).expect("index"),
                Stored::Int(value),
            );
        }
        (h.effect, in_args)
    })
}

/// Calls one action on the plugin's main entry point.
pub(super) fn action(plugin: &OfxPlugin, name: &CStr, handle: usize, in_args: usize) -> OfxStatus {
    // SAFETY: `mainEntry` takes a NUL-terminated action name and handles the plugin accepts;
    // the host state these handles refer to is installed before the call.
    unsafe {
        (plugin.mainEntry)(
            name.as_ptr(),
            handle as *const c_void,
            in_args as OfxPropertySetHandle,
            ptr::null_mut(),
        )
    }
}

pub(super) fn check(status: OfxStatus, what: &str) -> Result<(), String> {
    if status == OFX_STAT_OK {
        Ok(())
    } else {
        Err(format!("{what} returned status {status}"))
    }
}

/// An image as the test sees it: top-down rows of RGBA floats.
pub(super) struct Frame {
    pub(super) width: c_int,
    pub(super) height: c_int,
    pub(super) rows: Vec<f32>,
}

impl Frame {
    /// The rows in OFX order (row 0 is the bottom row), so the plugin sees a bottom-up host.
    pub(super) fn bottom_up(&self) -> Vec<f32> {
        flip_rows(&self.rows, self.width as usize * 4)
    }
}

/// Reverses the row order. Applying it twice gives the input back.
pub(super) fn flip_rows(values: &[f32], row_floats: usize) -> Vec<f32> {
    values
        .chunks_exact(row_floats)
        .rev()
        .flatten()
        .copied()
        .collect()
}

/// Loads an image the same way the CLI does: `image::open` followed by `to_rgba32f`.
pub(super) fn load_frame(path: &Path) -> Result<Frame, String> {
    let image = image::open(path)
        .map_err(|e| format!("loading {}: {e}", path.display()))?
        .to_rgba32f();
    Ok(Frame {
        width: c_int::try_from(image.width()).map_err(|e| e.to_string())?,
        height: c_int::try_from(image.height()).map_err(|e| e.to_string())?,
        rows: image.into_raw(),
    })
}

/// Renders one frame through the plugin's full lifecycle and returns the top-down output rows.
pub(super) fn render_through_plugin(
    values: HashMap<&'static str, ParamValue>,
    color: &Frame,
    depth: Option<&Frame>,
) -> Result<Vec<f32>, String> {
    let (effect, in_args) = install_host(values, color, depth);
    let plugin = OfxGetPlugin(0);
    if plugin.is_null() {
        return Err("plugin exports no descriptor".to_owned());
    }
    // SAFETY: the descriptor for index 0 is static and lives for the process.
    let plugin = unsafe { &*plugin };
    let mut host = OfxHost {
        host: ptr::null_mut(),
        fetchSuite: fetch_suite,
    };
    // SAFETY: `host` lives on this stack frame until Unload below, which the plugin needs.
    unsafe { (plugin.setHost)(&raw mut host) };

    let outcome: Result<(), String> = (|| {
        check(action(plugin, OFX_ACTION_LOAD, 0, 0), "Load")?;
        check(
            action(plugin, OFX_ACTION_CREATE_INSTANCE, effect, 0),
            "CreateInstance",
        )?;
        let render = action(plugin, OFX_IMAGE_EFFECT_ACTION_RENDER, effect, in_args);
        // Destroy the instance even when the render failed.
        let destroy = action(plugin, OFX_ACTION_DESTROY_INSTANCE, effect, 0);
        check(render, "Render")?;
        check(destroy, "DestroyInstance")?;
        Ok(())
    })();
    check(action(plugin, OFX_ACTION_UNLOAD, 0, 0), "Unload")?;
    outcome?;

    let output = with(|h| {
        let clip = h.clip_named("Output");
        flip_rows(&clip.pixels, clip.width as usize * 4)
    });
    Ok(output)
}

/// Max absolute difference between two RGBA arrays, counting non-finite values as failures.
pub(super) fn compare(rendered: &[f32], baseline: &[f32]) -> (f32, usize) {
    let mut max = 0.0f32;
    let mut non_finite = 0;
    for (a, b) in rendered.iter().zip(baseline) {
        let d = (a - b).abs();
        if d.is_finite() {
            max = max.max(d);
        } else {
            non_finite += 1;
        }
    }
    (max, non_finite)
}
