//! Entry points the host calls directly: the exported plugin table, the actions the plugin leaves
//! to the host, the `InstanceChanged` type filter, the suite lookup at Load, and the error and
//! string helpers that the plugin and its hosts share.

use super::*;
use opendefocus_ofx::ofx_sys::{OFX_IMAGE_EFFECT_ACTION_IS_IDENTITY, OFX_STAT_REPLY_DEFAULT};

/// Checks that the plugin table answers only for index 0 and that index 0 is the descriptor.
#[test]
fn get_plugin_answers_only_for_the_first_index() {
    assert!(!OfxGetPlugin(0).is_null());
    assert!(OfxGetPlugin(1).is_null());
    assert!(OfxGetPlugin(-1).is_null());
}

/// Checks that a null action name is rejected before any state is touched.
#[test]
fn null_action_name_fails() {
    // SAFETY: the null action is rejected before any argument is read.
    let status =
        unsafe { (plugin().mainEntry)(ptr::null(), ptr::null(), ptr::null_mut(), ptr::null_mut()) };
    assert_eq!(status, OFX_STAT_FAILED);
}

/// Checks that `IsIdentity` is left to the host: the plugin does not handle it, so it replies with
/// the default.
#[test]
fn is_identity_is_left_to_the_host() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.action(
            OFX_IMAGE_EFFECT_ACTION_IS_IDENTITY,
            session.effect(),
            ptr::null_mut()
        ),
        OFX_STAT_REPLY_DEFAULT
    );
}

/// Checks that `InstanceChanged` syncs the Enable dependents only for parameter changes, and leaves
/// them alone for a change to any other kind of object.
#[test]
fn instance_changed_syncs_only_for_parameter_changes() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    assert_eq!(session.create_instance(), OFX_STAT_OK);
    let amount = session
        .param_props("catseyeAmount")
        .expect("catseyeAmount is defined");
    assert_eq!(prop_int(amount, OFX_PARAM_PROP_ENABLED, 0), Some(0));

    session.set_param_value("catseyeEnable", Value::Int(1));
    assert_eq!(
        session.action(
            OFX_ACTION_INSTANCE_CHANGED,
            session.effect(),
            change_args("OfxImageEffectInstance")
        ),
        OFX_STAT_OK
    );
    assert_eq!(prop_int(amount, OFX_PARAM_PROP_ENABLED, 0), Some(0));

    assert_eq!(
        session.action(
            OFX_ACTION_INSTANCE_CHANGED,
            session.effect(),
            change_args(&OFX_TYPE_PARAMETER.to_string_lossy())
        ),
        OFX_STAT_OK
    );
    assert_eq!(prop_int(amount, OFX_PARAM_PROP_ENABLED, 0), Some(1));
}

/// Checks that an `InstanceChanged` without a type is not an error: there is nothing to sync.
#[test]
fn instance_changed_without_a_type_is_ignored() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    let args = with(State::alloc) as OfxPropertySetHandle;
    assert_eq!(
        session.action(OFX_ACTION_INSTANCE_CHANGED, session.effect(), args),
        OFX_STAT_OK
    );
}

/// Checks that an `InstanceChanged` whose type is an empty (null) string is ignored like one with no type.
#[test]
fn instance_changed_with_an_empty_type_is_ignored() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    let args = with(State::alloc);
    store(
        (args, OFX_PROP_TYPE.to_string_lossy().into_owned()),
        0,
        Value::NullStr,
    );
    assert_eq!(
        session.action(
            OFX_ACTION_INSTANCE_CHANGED,
            session.effect(),
            args as OfxPropertySetHandle
        ),
        OFX_STAT_OK
    );
}

/// Checks that Load fails with a missing host feature when no host was recorded by `setHost`.
#[test]
fn load_suites_without_a_host_is_a_missing_feature() {
    let _session = Session::new(&[]);
    opendefocus_ofx::suites::clear();
    // SAFETY: no host is recorded, so the function returns before touching any pointer.
    let error = unsafe { opendefocus_ofx::suites::load_suites() }
        .err()
        .map(|e| e.0);
    assert_eq!(error, Some(OFX_STAT_ERR_MISSING_HOST_FEATURE));
}

/// Checks the text of a host-call error, which the plugin's messages are built from.
#[test]
fn ofx_error_names_its_status() {
    assert_eq!(
        opendefocus_ofx::suites::OfxError(-1).to_string(),
        "OFX host call failed with status -1"
    );
    assert_eq!(
        opendefocus_ofx::suites::OfxError(OFX_STAT_ERR_MEMORY).to_string(),
        format!("OFX host call failed with status {OFX_STAT_ERR_MEMORY}")
    );
}

/// Checks that the settings read refuses null in-args rather than dereferencing them.
#[test]
fn effect_settings_without_in_args_fails() {
    let session = ready_session();
    // SAFETY: the effect is this session's loaded instance; the null in-args are rejected first.
    let error = unsafe {
        opendefocus_ofx::render::effect_settings(session.effect().cast(), ptr::null_mut())
    }
    .err()
    .map(|e| e.0);
    assert_eq!(error, Some(OFX_STAT_FAILED));
}

/// Checks that a string parameter's value is copied out, and that a null string reads as empty.
#[test]
fn param_get_string_copies_the_host_string() {
    let session = ready_session();
    let handle = session
        .param_handle("defocusMode")
        .expect("defocusMode is defined");
    session.set_param_value(
        "defocusMode",
        Value::Str(CString::new("sample text").expect("no NUL")),
    );
    assert_eq!(
        opendefocus_ofx::suites::param_get_string(handle, 0.0).as_deref(),
        Ok("sample text")
    );
    session.set_param_value("defocusMode", Value::NullStr);
    assert_eq!(
        opendefocus_ofx::suites::param_get_string(handle, 0.0).as_deref(),
        Ok("")
    );
}
