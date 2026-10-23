//! Enable-state sync: greys out the parameters that a toggle or filter choice does not apply to,
//! through the host's enabled property.

use super::{
    HostSuites, OFX_PARAM_PROP_ENABLED, OfxParamHandle, OfxParamSetHandle, OfxPropertySetHandle,
    PARAMS, Props, Result, c_int, c_string, check, param_get_bool, param_get_i32, stored_suites,
};

/// Enable toggles and the sub-parameters each one gates. Hosts grey out a gated parameter
/// while its toggle is off.
pub(super) const GATED_BY_TOGGLE: &[(&str, &[&str])] = &[
    (
        "catseyeEnable",
        &[
            "catseyeAmount",
            "catseyeGamma",
            "catseyeSoftness",
            "catseyeInverse",
        ],
    ),
    (
        "axialAberrationEnable",
        &["axialAberrationAmount", "axialAberrationColorType"],
    ),
    (
        "astigmatismEnable",
        &["astigmatismAmount", "astigmatismGamma"],
    ),
    (
        "barndoorsEnable",
        &[
            "barndoorsAmount",
            "barndoorsGamma",
            "barndoorsInverse",
            "barndoorsTop",
            "barndoorsBottom",
            "barndoorsLeft",
            "barndoorsRight",
        ],
    ),
];

/// Sets kOfxParamPropEnabled on each gated parameter from the current values of its toggle.
/// The bokeh knobs follow the filter type choice (see the loop at the end).
///
/// # Errors
///
/// Returns [`OfxError`] if the suites were not stored at Load or the host rejects a call.
///
/// # Safety
///
/// `param_set` must be the parameter set of the effect instance, valid for the call.
pub unsafe fn sync_enabled(param_set: OfxParamSetHandle) -> Result<()> {
    let suites = stored_suites()?;
    for (toggle, dependents) in GATED_BY_TOGGLE {
        // SAFETY: `param_set` is valid per this function's contract.
        let param = unsafe { param_handle(&suites, param_set, toggle) }?;
        let on = param_get_bool(param, 0.0)?;
        for name in *dependents {
            // SAFETY: as above.
            unsafe { set_enabled(&suites, param_set, name, on) }?;
        }
    }
    // The bokeh shape knobs apply to the Disc and Blade filters (choice index 1 and 2), and
    // the blade count only to Blade (index 2). The Simple filter (index 0) ignores them all.
    // SAFETY: as above.
    let filter = unsafe { param_handle(&suites, param_set, "bokehFilterType") }?;
    let filter_index = param_get_i32(filter, 0.0)?;
    for def in PARAMS {
        if def.group != "bokeh" || def.name == "bokehFilterType" {
            continue;
        }
        let on = if def.name == "bokehBlades" {
            filter_index == 2
        } else {
            filter_index != 0
        };
        // SAFETY: as above.
        unsafe { set_enabled(&suites, param_set, def.name, on) }?;
    }
    Ok(())
}

/// The handle of the named parameter in `param_set`.
///
/// # Safety
///
/// `param_set` must be valid for the call.
pub(super) unsafe fn param_handle(
    suites: &HostSuites,
    param_set: OfxParamSetHandle,
    name: &str,
) -> Result<OfxParamHandle> {
    let name = c_string(name)?;
    let mut param = core::ptr::null_mut();
    let mut props: OfxPropertySetHandle = core::ptr::null_mut();
    // SAFETY: `param_set` is valid per the caller contract; the name and out pointers live
    // for the call.
    check(unsafe {
        (suites.parameter.paramGetHandle)(param_set, name.as_ptr(), &raw mut param, &raw mut props)
    })?;
    Ok(param)
}

/// Sets kOfxParamPropEnabled on the named parameter of `param_set`.
///
/// # Safety
///
/// `param_set` must be valid for the call.
pub(super) unsafe fn set_enabled(
    suites: &HostSuites,
    param_set: OfxParamSetHandle,
    name: &str,
    enabled: bool,
) -> Result<()> {
    // SAFETY: forwarded from this function's contract.
    let param = unsafe { param_handle(suites, param_set, name) }?;
    let mut props: OfxPropertySetHandle = core::ptr::null_mut();
    // SAFETY: `param` was just returned by the host for this parameter set.
    check(unsafe { (suites.parameter.paramGetPropertySet)(param, &raw mut props) })?;
    Props::new(props, suites.property).set_int(OFX_PARAM_PROP_ENABLED, 0, enabled as c_int)
}
