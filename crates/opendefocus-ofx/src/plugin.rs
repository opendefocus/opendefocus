//! `OpenFX` entry points: the exported plugin symbols, and the dispatch of host actions (Load,
//! Describe, Render and the rest) to the plugin's handlers.

use core::ffi::{CStr, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::ofx_sys::{
    OFX_ACTION_CREATE_INSTANCE, OFX_ACTION_DESCRIBE, OFX_ACTION_DESTROY_INSTANCE,
    OFX_ACTION_INSTANCE_CHANGED, OFX_ACTION_LOAD, OFX_ACTION_UNLOAD, OFX_BIT_DEPTH_FLOAT,
    OFX_IMAGE_CLIP_PROP_COMPONENTS_OUTPUT, OFX_IMAGE_CLIP_PROP_DEPTH_OUTPUT,
    OFX_IMAGE_CLIP_PROP_OPTIONAL, OFX_IMAGE_COMPONENT_ALPHA, OFX_IMAGE_COMPONENT_RGBA,
    OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT, OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES,
    OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST, OFX_IMAGE_EFFECT_ACTION_IS_IDENTITY,
    OFX_IMAGE_EFFECT_ACTION_RENDER, OFX_IMAGE_EFFECT_CONTEXT_FILTER,
    OFX_IMAGE_EFFECT_CONTEXT_GENERAL, OFX_IMAGE_EFFECT_PLUGIN_API,
    OFX_IMAGE_EFFECT_PLUGIN_API_VERSION, OFX_IMAGE_EFFECT_PLUGIN_PROP_FIELD_RENDER_TWICE_ALWAYS,
    OFX_IMAGE_EFFECT_PLUGIN_PROP_GROUPING, OFX_IMAGE_EFFECT_PLUGIN_PROP_HOST_FRAME_THREADING,
    OFX_IMAGE_EFFECT_PLUGIN_RENDER_THREAD_SAFETY, OFX_IMAGE_EFFECT_PROP_PRE_MULTIPLICATION,
    OFX_IMAGE_EFFECT_PROP_SUPPORTED_COMPONENTS, OFX_IMAGE_EFFECT_PROP_SUPPORTED_CONTEXTS,
    OFX_IMAGE_EFFECT_PROP_SUPPORTED_PIXEL_DEPTHS, OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTI_RESOLUTION,
    OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTIPLE_CLIP_DEPTHS,
    OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTIPLE_CLIP_PARS, OFX_IMAGE_EFFECT_PROP_SUPPORTS_TILES,
    OFX_IMAGE_EFFECT_PROP_TEMPORAL_CLIP_ACCESS, OFX_IMAGE_EFFECT_RENDER_INSTANCE_SAFE,
    OFX_IMAGE_PRE_MULTIPLIED, OFX_PROP_LABEL, OFX_PROP_PLUGIN_DESCRIPTION, OFX_PROP_TYPE,
    OFX_STAT_ERR_BAD_HANDLE, OFX_STAT_FAILED, OFX_STAT_OK, OFX_STAT_REPLY_DEFAULT,
    OFX_TYPE_PARAMETER, OfxHost, OfxImageEffectHandle, OfxPlugin, OfxPropertySetHandle,
    OfxPropertySetStruct, OfxStatus,
};
use crate::params::{define_params, set_page_order, sync_enabled};
use crate::render;
use crate::suites::{
    HostSuites, Props, Result, check, load_suites, set_host as store_host, stored_suites,
};

/// Plugin identifier, as required by the OFX plugin descriptor.
const PLUGIN_IDENTIFIER: &CStr = c"org.opendefocus.OpenDefocus";
/// Name the host shows for the plugin.
const PLUGIN_LABEL: &CStr = c"OpenDefocus";
/// Short description shown in the host's plugin information.
const PLUGIN_DESCRIPTION: &CStr = c"Physically based defocus blur (WGPU).";
/// Menu group the host files the plugin under.
const PLUGIN_GROUPING: &CStr = c"OpenDefocus";
/// Name of the input clip that carries the image to defocus.
const CLIP_SOURCE: &CStr = c"Source";
/// Name of the optional clip that carries the depth map.
const CLIP_DEPTH: &CStr = c"Depth";
/// Name of the output clip the render writes into.
const CLIP_OUTPUT: &CStr = c"Output";

/// Wrapper that lets the plugin descriptor live in a `static`. The raw pointers in it
/// point to `'static` C strings and functions, so sharing it across threads is sound.
struct SyncPlugin(OfxPlugin);
// SAFETY: the descriptor only holds pointers to 'static C string literals and to functions
// in this binary, and it is never mutated after initialization.
unsafe impl Sync for SyncPlugin {}

/// The plugin descriptor the host reads through [`OfxGetPlugin`]: API, version, identifier and
/// the `setHost` and `mainEntry` entry points.
static PLUGIN: SyncPlugin = SyncPlugin(OfxPlugin {
    pluginApi: OFX_IMAGE_EFFECT_PLUGIN_API.as_ptr(),
    apiVersion: OFX_IMAGE_EFFECT_PLUGIN_API_VERSION,
    pluginIdentifier: PLUGIN_IDENTIFIER.as_ptr(),
    pluginVersionMajor: 0,
    pluginVersionMinor: 2,
    setHost: set_host,
    mainEntry: main_entry,
});

/// Returns the number of plugins in this binary.
#[allow(
    non_snake_case,
    reason = "the exported symbol name is fixed by the OFX C ABI"
)]
#[unsafe(no_mangle)]
pub extern "C" fn OfxGetNumberOfPlugins() -> c_int {
    1
}

/// Returns the `nth` plugin descriptor, or null when `nth` is out of range.
#[allow(
    non_snake_case,
    reason = "the exported symbol name is fixed by the OFX C ABI"
)]
#[unsafe(no_mangle)]
pub extern "C" fn OfxGetPlugin(nth: c_int) -> *mut crate::ofx_sys::OfxPlugin {
    if nth != 0 {
        return core::ptr::null_mut();
    }
    core::ptr::addr_of!(PLUGIN.0).cast_mut()
}

/// OFX `setHost`: records the host pointer. Must not call any host function.
///
/// # Safety
///
/// Called by the host with a pointer valid for the lifetime of the loaded plugin.
unsafe extern "C" fn set_host(host: *mut OfxHost) {
    // A panic payload cannot cross the C boundary and the host has no channel to report it.
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: forwarded from this function's contract.
        unsafe { store_host(host) }
    }))
    .ok();
}

/// OFX `mainEntry`: dispatches one action. Never unwinds across the FFI boundary.
///
/// # Safety
///
/// The host passes a valid NUL-terminated `action` and valid property set handles for
/// the duration of the call, as the OFX specification requires.
unsafe extern "C" fn main_entry(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    catch_unwind(AssertUnwindSafe(|| {
        if action.is_null() {
            return OFX_STAT_FAILED;
        }
        // SAFETY: the host passes a valid NUL-terminated action string.
        let action = unsafe { CStr::from_ptr(action) };
        dispatch(action, handle as OfxImageEffectHandle, in_args, out_args)
    }))
    .unwrap_or_else(|_| {
        crate::suites::diagnostic("panic in OFX action dispatch");
        OFX_STAT_FAILED
    })
}

/// Actions whose handle is an effect or instance handle. A null handle is an error for these.
const HANDLE_ACTIONS: &[&CStr] = &[
    OFX_ACTION_DESCRIBE,
    OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT,
    OFX_ACTION_CREATE_INSTANCE,
    OFX_ACTION_DESTROY_INSTANCE,
    OFX_IMAGE_EFFECT_ACTION_RENDER,
    OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST,
    OFX_IMAGE_EFFECT_ACTION_IS_IDENTITY,
    OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES,
    OFX_ACTION_INSTANCE_CHANGED,
];

/// Routes one OFX action to its handler. Load and Unload are handled first, unknown actions get
/// the host default, and instance actions require a non-null handle. Returns the status to report.
fn dispatch(
    action: &CStr,
    handle: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if action == OFX_ACTION_LOAD {
        // SAFETY: setHost stored a valid host pointer before Load, per the OFX lifecycle.
        return status(unsafe { load() });
    }
    if action == OFX_ACTION_UNLOAD {
        crate::suites::clear();
        return OFX_STAT_OK;
    }
    if !HANDLE_ACTIONS.contains(&action) {
        return OFX_STAT_REPLY_DEFAULT;
    }
    if handle.is_null() {
        return OFX_STAT_ERR_BAD_HANDLE;
    }
    if action == OFX_IMAGE_EFFECT_ACTION_RENDER {
        return render::render(handle, in_args);
    }
    if action == OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST {
        return render::regions_of_interest(handle, in_args, out_args);
    }
    if action == OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES {
        return status(clip_preferences(out_args));
    }
    let result = if action == OFX_ACTION_DESCRIBE {
        describe(handle)
    } else if action == OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT {
        describe_in_context(handle)
    } else if action == OFX_ACTION_CREATE_INSTANCE {
        // Best effort: the enabled state is cosmetic and must not stop the instance being created.
        sync_enabled_states(handle).ok();
        render::create_instance(handle)
    } else if action == OFX_ACTION_DESTROY_INSTANCE {
        render::destroy_instance(handle)
    } else if action == OFX_ACTION_INSTANCE_CHANGED {
        instance_changed(handle, in_args)
    } else {
        return OFX_STAT_REPLY_DEFAULT;
    };
    status(result)
}

/// `InstanceChanged`: only a changed parameter can flip an Enable toggle, so other changes are
/// ignored.
fn instance_changed(effect: OfxImageEffectHandle, in_args: OfxPropertySetHandle) -> Result<()> {
    let suites = stored_suites()?;
    let props = Props::new(in_args, suites.property);
    // A host may omit the type; then there is nothing to sync.
    match props.get_string(OFX_PROP_TYPE, 0) {
        Ok(t) if t == OFX_TYPE_PARAMETER.to_string_lossy() => {}
        _ => return Ok(()),
    }
    // Cosmetic: a failure must not make the host see the edit as failed.
    sync_enabled_states(effect).ok();
    Ok(())
}

/// Greys out the sub-parameters of each Enable toggle to match its value. Run on create as well
/// as on change, since a host may not send `InstanceChanged` when it loads a project.
fn sync_enabled_states(effect: OfxImageEffectHandle) -> Result<()> {
    let suites = stored_suites()?;
    let mut param_set = core::ptr::null_mut();
    // SAFETY: `effect` is the handle the host passed to this action; `param_set` is an out pointer.
    check(unsafe { (suites.image_effect.getParamSet)(effect, &raw mut param_set) })?;
    // SAFETY: `param_set` was just returned by the host for this effect.
    unsafe { sync_enabled(param_set) }
}

/// Converts a handler result to the OFX status it reports: OK, or the status carried by the error.
fn status(result: Result<()>) -> OfxStatus {
    match result {
        Ok(()) => OFX_STAT_OK,
        Err(error) => error.0,
    }
}

/// Fetches the host suites and stores them for later actions.
///
/// # Safety
///
/// The host must have called `setHost` and keep its pointers valid until Unload.
unsafe fn load() -> Result<()> {
    // SAFETY: forwarded from this function's contract.
    let suites = unsafe { load_suites()? };
    crate::suites::store_suites(suites);
    Ok(())
}

/// Declares the Output clip preferences: float RGBA, premultiplied.
///
/// The Source premultiplication is not declared here: the OFX headers only define
/// `OfxImageEffectPropPreMultiplication` as an output clip preference, with no clip-level
/// request for the input. The plugin therefore assumes a premultiplied Source, as the CLI does.
fn clip_preferences(out_args: OfxPropertySetHandle) -> Result<()> {
    if out_args.is_null() {
        return Err(crate::suites::OfxError(OFX_STAT_FAILED));
    }
    let suites = crate::suites::stored_suites()?;
    let props = Props::new(out_args, suites.property);
    props.set_string(
        OFX_IMAGE_CLIP_PROP_COMPONENTS_OUTPUT,
        0,
        OFX_IMAGE_COMPONENT_RGBA,
    )?;
    props.set_string(OFX_IMAGE_CLIP_PROP_DEPTH_OUTPUT, 0, OFX_BIT_DEPTH_FLOAT)?;
    props.set_string(
        OFX_IMAGE_EFFECT_PROP_PRE_MULTIPLICATION,
        0,
        OFX_IMAGE_PRE_MULTIPLIED,
    )?;
    Ok(())
}

/// Describes the plugin: label, grouping, contexts, pixel depths and render safety.
///
/// Label, contexts and pixel depths are required. The rest are hints, and a host that
/// rejects one (Resolve answers `kOfxStatErrUnknown`) must not lose the whole plugin.
fn describe(effect: OfxImageEffectHandle) -> Result<()> {
    let suites = crate::suites::stored_suites()?;
    let props = effect_props(&suites, effect)?;
    props.set_string(OFX_PROP_LABEL, 0, PLUGIN_LABEL)?;
    props.set_string(
        OFX_IMAGE_EFFECT_PROP_SUPPORTED_CONTEXTS,
        0,
        OFX_IMAGE_EFFECT_CONTEXT_FILTER,
    )?;
    props.set_string(
        OFX_IMAGE_EFFECT_PROP_SUPPORTED_CONTEXTS,
        1,
        OFX_IMAGE_EFFECT_CONTEXT_GENERAL,
    )?;
    props.set_string(
        OFX_IMAGE_EFFECT_PROP_SUPPORTED_PIXEL_DEPTHS,
        0,
        OFX_BIT_DEPTH_FLOAT,
    )?;
    let hint = |name: &CStr, result: Result<()>| {
        if let Err(error) = result {
            crate::suites::diagnostic(&format!("host rejected {name:?} in describe: {}", error.0));
        }
    };
    hint(
        OFX_PROP_PLUGIN_DESCRIPTION,
        props.set_string(OFX_PROP_PLUGIN_DESCRIPTION, 0, PLUGIN_DESCRIPTION),
    );
    hint(
        OFX_IMAGE_EFFECT_PLUGIN_PROP_GROUPING,
        props.set_string(OFX_IMAGE_EFFECT_PLUGIN_PROP_GROUPING, 0, PLUGIN_GROUPING),
    );
    hint(
        OFX_IMAGE_EFFECT_PLUGIN_RENDER_THREAD_SAFETY,
        props.set_string(
            OFX_IMAGE_EFFECT_PLUGIN_RENDER_THREAD_SAFETY,
            0,
            OFX_IMAGE_EFFECT_RENDER_INSTANCE_SAFE,
        ),
    );
    for (name, value) in [
        (OFX_IMAGE_EFFECT_PROP_SUPPORTS_TILES, 1),
        (OFX_IMAGE_EFFECT_PROP_TEMPORAL_CLIP_ACCESS, 0),
        (OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTIPLE_CLIP_DEPTHS, 0),
        (OFX_IMAGE_EFFECT_PLUGIN_PROP_FIELD_RENDER_TWICE_ALWAYS, 0),
        (OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTI_RESOLUTION, 0),
        (OFX_IMAGE_EFFECT_PLUGIN_PROP_HOST_FRAME_THREADING, 0),
        (OFX_IMAGE_EFFECT_PROP_SUPPORTS_MULTIPLE_CLIP_PARS, 0),
    ] {
        hint(name, props.set_int(name, 0, value));
    }
    Ok(())
}

/// Declares the clips and parameters for one context.
fn describe_in_context(effect: OfxImageEffectHandle) -> Result<()> {
    let suites = crate::suites::stored_suites()?;

    define_clip(
        &suites,
        effect,
        CLIP_SOURCE,
        false,
        &[OFX_IMAGE_COMPONENT_RGBA],
    )?;
    define_clip(
        &suites,
        effect,
        CLIP_DEPTH,
        true,
        &[OFX_IMAGE_COMPONENT_RGBA, OFX_IMAGE_COMPONENT_ALPHA],
    )?;
    define_clip(
        &suites,
        effect,
        CLIP_OUTPUT,
        false,
        &[OFX_IMAGE_COMPONENT_RGBA],
    )?;
    // SAFETY: `effect` is the descriptor handle this DescribeInContext action received.
    unsafe { crate::camera::define_camera_input(&suites, effect) }?;

    let mut param_set = core::ptr::null_mut();
    // SAFETY: `effect` is the descriptor handle the host passed to this action.
    check(unsafe { (suites.image_effect.getParamSet)(effect, &raw mut param_set) })?;
    // SAFETY: `param_set` was just returned by the host for this action.
    unsafe { define_params(&suites, param_set) }?;
    // Best effort: Natron rejects kOfxPluginPropParamPageOrder (kOfxStatErrUnknown), on the
    // descriptor in Describe and in DescribeInContext alike. Without the property the host uses
    // the declaration order, which is PAGES order, so the failure is ignored.
    set_page_order(&effect_props(&suites, effect)?).ok();
    Ok(())
}

/// Declares one clip on the descriptor with the components it accepts. `optional` marks the clip
/// as one the host may leave unconnected.
fn define_clip(
    suites: &HostSuites,
    effect: OfxImageEffectHandle,
    name: &CStr,
    optional: bool,
    components: &[&CStr],
) -> Result<()> {
    let mut clip_props: OfxPropertySetHandle = core::ptr::null_mut();
    // SAFETY: `effect` is valid for this action; `name` outlives the call.
    check(unsafe { (suites.image_effect.clipDefine)(effect, name.as_ptr(), &raw mut clip_props) })?;
    let props = Props::new(clip_props, suites.property);
    for (i, component) in components.iter().enumerate() {
        props.set_string(
            OFX_IMAGE_EFFECT_PROP_SUPPORTED_COMPONENTS,
            i as c_int,
            component,
        )?;
    }
    props.set_int(OFX_IMAGE_CLIP_PROP_OPTIONAL, 0, optional as c_int)?;
    Ok(())
}

/// Property set of an effect handle (the descriptor or an instance).
pub(crate) fn effect_props(suites: &HostSuites, effect: OfxImageEffectHandle) -> Result<Props<'_>> {
    let mut handle: *mut OfxPropertySetStruct = core::ptr::null_mut();
    // SAFETY: `effect` is a handle the host passed to this action; `handle` is an out pointer.
    check(unsafe { (suites.image_effect.getPropertySet)(effect, &raw mut handle) })?;
    Ok(Props::new(handle, suites.property))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ofx_sys::{OFX_STAT_ERR_MISSING_HOST_FEATURE, OFX_STAT_REPLY_DEFAULT};
    use core::ptr::NonNull;

    /// Checks that the binary exports exactly one plugin with the expected identifier and API version.
    #[test]
    fn exports_one_plugin_with_identifier() {
        assert_eq!(OfxGetNumberOfPlugins(), 1);
        // SAFETY: the pointer is null or refers to the static descriptor, which is always valid.
        let plugin = unsafe { OfxGetPlugin(0).as_ref() }.expect("plugin 0 is exported");
        assert_eq!(plugin.apiVersion, 1);
        // SAFETY: `pluginIdentifier` is a NUL-terminated string literal inside the static
        // descriptor, so it is valid for reads for the rest of the program.
        let identifier = unsafe { CStr::from_ptr(plugin.pluginIdentifier) };
        assert_eq!(identifier, c"org.opendefocus.OpenDefocus");
        assert!(OfxGetPlugin(1).is_null());
    }

    /// Checks that an action the plugin does not implement is passed back to the host as default.
    #[test]
    fn unknown_action_is_ignored() {
        let status = dispatch_str(c"OfxActionGetRegionOfDefinition");
        assert_eq!(status, OFX_STAT_REPLY_DEFAULT);
    }

    /// Checks that Describe before Load reports the missing host feature instead of crashing.
    #[test]
    fn describe_before_load_reports_missing_host_feature() {
        let status = dispatch(
            OFX_ACTION_DESCRIBE,
            NonNull::dangling().as_ptr(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, OFX_STAT_ERR_MISSING_HOST_FEATURE);
    }

    /// Checks that every instance action rejects a null handle with a bad-handle status.
    #[test]
    fn null_handle_is_bad_handle_for_instance_actions() {
        for action in HANDLE_ACTIONS {
            assert_eq!(
                dispatch(
                    action,
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                    core::ptr::null_mut()
                ),
                OFX_STAT_ERR_BAD_HANDLE,
                "{action:?}"
            );
        }
    }

    /// Dispatches an action with no handle or arguments, for checks that need neither.
    fn dispatch_str(action: &CStr) -> OfxStatus {
        dispatch(
            action,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    }
}
