//! Lifecycle tests through the mock host: Load, Describe, instance creation and destruction, Unload, and the handle and suite round trips.

use super::*;

/// Reads a string property at `index` from a property set, or an empty string if it is unset.
pub(super) fn string_at(props: usize, key: &CStr, index: c_int) -> String {
    prop_string(props, key, index).unwrap_or_else(|| panic!("missing string {key:?}[{index}]"))
}

/// Checks that Describe declares the plugin's clips, parameters and pages with the expected properties.
#[test]
fn describe_declares_plugin_clips_and_params() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    assert_plugin_descriptor();
    assert_effect_properties(&session);
    assert_clips(&session);
    assert_groups_and_pages(&session);
    for def in PARAMS {
        assert_param(&session, def);
    }
}

/// Checks the static plugin descriptor fields.
fn assert_plugin_descriptor() {
    // Plugin descriptor fields.
    let plugin = plugin();
    assert_eq!(plugin.apiVersion, 1);
    assert_eq!(plugin.pluginVersionMajor, 0);
    assert_eq!(plugin.pluginVersionMinor, 2);
    // SAFETY: the descriptor strings are static C literals.
    unsafe {
        assert_eq!(
            CStr::from_ptr(plugin.pluginIdentifier),
            c"org.opendefocus.OpenDefocus"
        );
    }
}

/// Checks the effect-level properties set by Describe.
fn assert_effect_properties(session: &Session) {
    // Effect properties.
    let fx = session.effect_props();
    assert_eq!(string_at(fx, OFX_PROP_LABEL, 0), "OpenDefocus");
    assert_eq!(
        string_at(fx, c"OfxImageEffectPluginPropGrouping", 0),
        "OpenDefocus"
    );
    assert_eq!(prop_count(fx, c"OfxImageEffectPropSupportedContexts"), 2);
    assert_eq!(
        string_at(fx, c"OfxImageEffectPropSupportedContexts", 0),
        "OfxImageEffectContextFilter"
    );
    assert_eq!(
        string_at(fx, c"OfxImageEffectPropSupportedContexts", 1),
        "OfxImageEffectContextGeneral"
    );
    assert_eq!(prop_count(fx, c"OfxImageEffectPropSupportedPixelDepths"), 1);
    assert_eq!(
        string_at(fx, c"OfxImageEffectPropSupportedPixelDepths", 0),
        "OfxBitDepthFloat"
    );
    assert_eq!(
        string_at(fx, c"OfxImageEffectPluginRenderThreadSafety", 0),
        "OfxImageEffectRenderInstanceSafe"
    );
    assert_eq!(prop_int(fx, c"OfxImageEffectPropSupportsTiles", 0), Some(1));
    for flag in [
        c"OfxImageEffectPropTemporalClipAccess",
        c"OfxImageEffectPropMultipleClipDepths",
        c"OfxImageEffectPluginPropFieldRenderTwiceAlways",
        c"OfxImageEffectPropSupportsMultiResolution",
        c"OfxImageEffectPluginPropHostFrameThreading",
        c"OfxImageEffectPropSupportsMultipleClipPARs",
    ] {
        assert_eq!(prop_int(fx, flag, 0), Some(0), "{flag:?}");
    }
}

/// Checks that Source and Output are required RGBA clips and Depth is optional RGBA or Alpha.
fn assert_clips(session: &Session) {
    // Clips: Source and Output are required RGBA, Depth is optional RGBA or Alpha.
    let source = session.clip_props("Source").expect("Source clip");
    assert_eq!(
        prop_count(source, c"OfxImageEffectPropSupportedComponents"),
        1
    );
    assert_eq!(
        string_at(source, c"OfxImageEffectPropSupportedComponents", 0),
        "OfxImageComponentRGBA"
    );
    assert_eq!(prop_int(source, OFX_IMAGE_CLIP_PROP_OPTIONAL, 0), Some(0));

    let depth = session.clip_props("Depth").expect("Depth clip");
    assert_eq!(
        prop_count(depth, c"OfxImageEffectPropSupportedComponents"),
        2
    );
    assert_eq!(
        string_at(depth, c"OfxImageEffectPropSupportedComponents", 0),
        "OfxImageComponentRGBA"
    );
    assert_eq!(
        string_at(depth, c"OfxImageEffectPropSupportedComponents", 1),
        "OfxImageComponentAlpha"
    );
    assert_eq!(prop_int(depth, OFX_IMAGE_CLIP_PROP_OPTIONAL, 0), Some(1));

    let output = session.clip_props("Output").expect("Output clip");
    assert_eq!(
        string_at(output, c"OfxImageEffectPropSupportedComponents", 0),
        "OfxImageComponentRGBA"
    );
}

/// Checks that the top-level group and every page are declared.
fn assert_groups_and_pages(session: &Session) {
    // Groups, pages and every parameter with the type, default and range from the table.
    assert_eq!(
        session.param_kind("defocus").as_deref(),
        Some("OfxParamTypeGroup")
    );
    for page in PAGES {
        assert_eq!(
            session.param_kind(page.name).as_deref(),
            Some("OfxParamTypePage"),
            "{}",
            page.name
        );
    }
}

/// Checks the type, label, group, default and choice options of one parameter from the table.
fn assert_param(session: &Session, def: &opendefocus_ofx::params::ParamDef) {
    let props = session
        .param_props(def.name)
        .unwrap_or_else(|| panic!("parameter {} not defined", def.name));
    let expected_kind = match def.kind {
        opendefocus_ofx::params::ParamKind::Double { .. } => "OfxParamTypeDouble",
        opendefocus_ofx::params::ParamKind::Int { .. } => "OfxParamTypeInteger",
        opendefocus_ofx::params::ParamKind::Bool { .. } => "OfxParamTypeBoolean",
        opendefocus_ofx::params::ParamKind::Choice { .. } => "OfxParamTypeChoice",
    };
    assert_eq!(
        session.param_kind(def.name).as_deref(),
        Some(expected_kind),
        "{}",
        def.name
    );
    assert_eq!(string_at(props, OFX_PROP_LABEL, 0), def.label);
    assert_eq!(string_at(props, OFX_PARAM_PROP_PARENT, 0), def.group);
    match def.default_value() {
        opendefocus_ofx::params::ParamValue::Double(d) => {
            assert_eq!(
                prop_double(props, OFX_PARAM_PROP_DEFAULT, 0),
                Some(d),
                "{}",
                def.name
            );
        }
        opendefocus_ofx::params::ParamValue::Int(i)
        | opendefocus_ofx::params::ParamValue::Choice(i) => {
            assert_eq!(
                prop_int(props, OFX_PARAM_PROP_DEFAULT, 0),
                Some(i),
                "{}",
                def.name
            );
        }
        opendefocus_ofx::params::ParamValue::Bool(b) => {
            // OFX passes booleans as int.
            assert_eq!(
                prop_int(props, OFX_PARAM_PROP_DEFAULT, 0),
                Some(b as c_int),
                "{}",
                def.name
            );
        }
    }
    if let opendefocus_ofx::params::ParamKind::Choice { options, .. } = def.kind {
        assert_eq!(
            prop_count(props, OFX_PARAM_PROP_CHOICE_OPTION),
            options.len(),
            "{}",
            def.name
        );
        for (i, option) in options.iter().enumerate() {
            assert_eq!(
                string_at(props, OFX_PARAM_PROP_CHOICE_OPTION, i as c_int),
                *option
            );
        }
    }
}

/// Checks that creating and then destroying an instance stores and clears the instance data.
#[test]
fn create_and_destroy_instance_round_trip() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.action(
            OFX_ACTION_CREATE_INSTANCE,
            session.effect(),
            ptr::null_mut()
        ),
        OFX_STAT_OK
    );
    assert_eq!(
        session.action(
            OFX_ACTION_DESTROY_INSTANCE,
            session.effect(),
            ptr::null_mut()
        ),
        OFX_STAT_OK
    );
}

/// Checks that Load succeeds when only the mandatory suites are present.
#[test]
fn load_succeeds_without_optional_suites() {
    let session = Session::new(&["OfxMessageSuite", "OfxMultiThreadSuite", "OfxMemorySuite"]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
}

/// Checks that Load fails with a missing-host-feature status when a mandatory suite is absent.
#[test]
fn load_fails_without_a_mandatory_suite() {
    let session = Session::new(&["OfxParameterSuite", "OfxPropertySuite"]);
    assert_eq!(session.load(), OFX_STAT_ERR_MISSING_HOST_FEATURE);
    assert_eq!(session.describe(), OFX_STAT_ERR_MISSING_HOST_FEATURE);
}

/// Checks that actions given a null handle report a bad-handle status.
#[test]
fn null_handles_are_bad_handles() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    let null = ptr::null_mut();
    for action in [
        OFX_ACTION_DESCRIBE,
        OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT,
        OFX_ACTION_CREATE_INSTANCE,
        OFX_ACTION_DESTROY_INSTANCE,
        OFX_IMAGE_EFFECT_ACTION_RENDER,
    ] {
        assert_eq!(
            session.action(action, null, ptr::null_mut()),
            OFX_STAT_ERR_BAD_HANDLE,
            "{action:?}"
        );
    }
    // A non-null handle reaches the action; Render without in-args fails.
    assert_eq!(session.render_without_args(), OFX_STAT_FAILED);
}

/// Checks that Unload clears the stored host and suites.
#[test]
fn unload_after_load_clears_the_host() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(
        session.action(OFX_ACTION_UNLOAD, ptr::null_mut(), ptr::null_mut()),
        OFX_STAT_OK
    );
    // The suites are gone after Unload, so Describe cannot run.
    assert_eq!(session.describe(), OFX_STAT_ERR_MISSING_HOST_FEATURE);
}

/// Checks that a double parameter value written through the variadic entry point reads back unchanged.
#[test]
fn param_double_round_trips_through_the_variadic_pointer_type() {
    use opendefocus_ofx::suites::{param_get_f64, param_get_i32};

    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );

    let size = session.param_handle("size").expect("size");
    let quality = session.param_handle("quality").expect("quality");
    assert_eq!(param_get_f64(size, 0.0), Ok(5.0));
    // Quality defaults to Accurate (index 1), the farm quality the CLI renders with.
    assert_eq!(param_get_i32(quality, 0.0), Ok(1));
}

/// Checks that an error message reaches the message suite with its text intact.
#[test]
fn message_error_passes_its_text_through_the_message_suite() {
    use opendefocus_ofx::suites::message_error;

    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    let effect = session.effect() as opendefocus_ofx::ofx_sys::OfxImageEffectHandle;
    assert_eq!(message_error(effect, c"render failed"), Ok(()));
    assert_eq!(
        session.last_message(),
        Some(("OfxMessageError".to_string(), "render failed".to_string()))
    );
}

/// Checks that an image stored on a clip is returned by the image-effect suite with its bounds and pixels.
#[test]
fn clip_image_round_trip_through_the_image_effect_suite() {
    use opendefocus_ofx::suites::stored_suites;

    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    // Two pixels in one row, four floats each.
    let pixels = vec![0.25_f32; 2 * 4];
    let address = session.set_clip_pixels("Source", 2, 1, &pixels);

    let suites = stored_suites().expect("suites");
    let effect = session.effect() as OfxImageEffectHandle;
    let mut clip: OfxImageClipHandle = ptr::null_mut();
    let mut clip_props: OfxPropertySetHandle = ptr::null_mut();
    // SAFETY: the mock's suite functions accept these handles and out pointers.
    unsafe {
        assert_eq!(
            (suites.image_effect.clipGetHandle)(
                effect,
                c"Source".as_ptr(),
                &raw mut clip,
                &raw mut clip_props
            ),
            OFX_STAT_OK
        );
        let mut image: OfxPropertySetHandle = ptr::null_mut();
        assert_eq!(
            (suites.image_effect.clipGetImage)(clip, 0.0, ptr::null(), &raw mut image),
            OFX_STAT_OK
        );
        let image_addr = image as usize;
        assert_eq!(
            prop_pointer(image_addr, OFX_IMAGE_PROP_DATA, 0),
            Some(address)
        );
        assert_eq!(prop_int(image_addr, OFX_IMAGE_PROP_ROW_BYTES, 0), Some(32));
        assert_eq!(prop_int(image_addr, OFX_IMAGE_PROP_BOUNDS, 2), Some(2));
        assert_eq!((suites.image_effect.clipReleaseImage)(image), OFX_STAT_OK);
        assert_eq!((suites.image_effect.abort)(effect), 0);
    }
    assert_eq!(session.images_released(), 1);
}

/// Loads the cdylib built from this package and calls its entry points through `dlopen`.
/// Skips with a message when the library has not been built (for example `cargo test` alone).
#[test]
fn dlopen_smoke_test_of_built_cdylib() {
    let exe = std::env::current_exe().expect("test executable path");
    // target/<profile>/deps/mock_host-<hash>; the cdylib lives in target/<profile>.
    let profile_dir = exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("target profile directory");
    let file = format!(
        "{}opendefocus_ofx{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let path = profile_dir.join(file);
    if !path.exists() {
        eprintln!(
            "skipping dlopen smoke test: {} not built; run `cargo build --release -p opendefocus-ofx`",
            path.display()
        );
        return;
    }

    // SAFETY: loading the plugin library runs only its static initializers, which do nothing.
    let lib = unsafe { libloading::Library::new(&path) }.expect("load cdylib");
    // SAFETY: the symbols have the signatures declared in plugin.rs.
    unsafe {
        let count: libloading::Symbol<unsafe extern "C" fn() -> c_int> = lib
            .get(b"OfxGetNumberOfPlugins\0")
            .expect("OfxGetNumberOfPlugins");
        assert_eq!(count(), 1);
        let get: libloading::Symbol<unsafe extern "C" fn(c_int) -> *mut OfxPlugin> =
            lib.get(b"OfxGetPlugin\0").expect("OfxGetPlugin");
        let descriptor = get(0);
        assert!(!descriptor.is_null());
        assert!(get(1).is_null());
        let descriptor = &*descriptor;
        assert_eq!(
            CStr::from_ptr(descriptor.pluginIdentifier),
            c"org.opendefocus.OpenDefocus"
        );
        assert_eq!(descriptor.apiVersion, 1);
        // A null Describe handle must come back as kOfxStatErrBadHandle (9).
        let status = (descriptor.mainEntry)(
            OFX_ACTION_DESCRIBE.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        assert_eq!(status, opendefocus_ofx::ofx_sys::OFX_STAT_ERR_BAD_HANDLE);
    }
}
