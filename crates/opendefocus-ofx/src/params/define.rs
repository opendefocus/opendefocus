//! Host descriptor code that declares every group, parameter and page on the effect parameter set
//! during the describe action.

use super::{
    GROUPS, HostSuites, OFX_PARAM_PROP_ANIMATES, OFX_PARAM_PROP_CHOICE_OPTION,
    OFX_PARAM_PROP_DEFAULT, OFX_PARAM_PROP_DISPLAY_MAX, OFX_PARAM_PROP_DISPLAY_MIN,
    OFX_PARAM_PROP_HINT, OFX_PARAM_PROP_MAX, OFX_PARAM_PROP_MIN, OFX_PARAM_PROP_PAGE_CHILD,
    OFX_PARAM_PROP_PARENT, OFX_PARAM_TYPE_BOOLEAN, OFX_PARAM_TYPE_CHOICE, OFX_PARAM_TYPE_DOUBLE,
    OFX_PARAM_TYPE_GROUP, OFX_PARAM_TYPE_INTEGER, OFX_PARAM_TYPE_PAGE, OFX_PROP_LABEL,
    OfxParamSetHandle, OfxPropertySetHandle, PAGES, PARAMS, ParamKind, Props, Result, c_int,
    c_string, check, display_range,
};

/// Declares every group, parameter and the page that lays the parameters out, on the
/// parameter set of an effect descriptor.
///
/// # Errors
///
/// Returns [`OfxError`] if the host rejects a group, parameter or page definition.
///
/// # Panics
///
/// Panics if a name in the static parameter tables contains a NUL byte. The tables are fixed
/// at compile time and contain none.
///
/// # Safety
///
/// `param_set` must be the parameter set handle the host gave to the current
/// describe action, which is valid for the duration of that action.
pub unsafe fn define_params(suites: &HostSuites, param_set: OfxParamSetHandle) -> Result<()> {
    for group in GROUPS {
        let name = c_string(group.name)?;
        let mut props: OfxPropertySetHandle = core::ptr::null_mut();
        // SAFETY: `param_set` is valid per the caller contract; the strings and the out
        // pointer live for the call.
        check(unsafe {
            (suites.parameter.paramDefine)(
                param_set,
                OFX_PARAM_TYPE_GROUP.as_ptr(),
                name.as_ptr(),
                &raw mut props,
            )
        })?;
        let p = Props::new(props, suites.property);
        p.set_string(OFX_PROP_LABEL, 0, &c_string(group.label)?)?;
        if let Some(parent) = group.parent {
            p.set_string(OFX_PARAM_PROP_PARENT, 0, &c_string(parent)?)?;
        }
    }

    for def in PARAMS {
        let name = c_string(def.name)?;
        let mut props: OfxPropertySetHandle = core::ptr::null_mut();
        let type_name = match def.kind {
            ParamKind::Double { .. } => OFX_PARAM_TYPE_DOUBLE,
            ParamKind::Int { .. } => OFX_PARAM_TYPE_INTEGER,
            ParamKind::Bool { .. } => OFX_PARAM_TYPE_BOOLEAN,
            ParamKind::Choice { .. } => OFX_PARAM_TYPE_CHOICE,
        };
        // SAFETY: as above, `param_set` is valid for this action and the strings outlive
        // the call.
        check(unsafe {
            (suites.parameter.paramDefine)(
                param_set,
                type_name.as_ptr(),
                name.as_ptr(),
                &raw mut props,
            )
        })?;
        let p = Props::new(props, suites.property);
        let label = c_string(def.label)?;
        let hint = c_string(def.hint)?;
        p.set_string(OFX_PROP_LABEL, 0, &label)?;
        p.set_string(OFX_PARAM_PROP_HINT, 0, &hint)?;
        // Parent places the parameter in its group. The page lists it as a child too, since
        // a page is the layout a paged host shows.
        p.set_string(OFX_PARAM_PROP_PARENT, 0, &c_string(def.group)?)?;
        // kOfxParamPropAnimates is a bool; OFX passes bools as int.
        p.set_int(OFX_PARAM_PROP_ANIMATES, 0, 0)?;
        match def.kind {
            ParamKind::Double { min, max, default } => {
                let (display_min, display_max) = display_range(def.name).unwrap_or((min, max));
                p.set_double(OFX_PARAM_PROP_DEFAULT, 0, default)?;
                p.set_double(OFX_PARAM_PROP_MIN, 0, min)?;
                p.set_double(OFX_PARAM_PROP_MAX, 0, max)?;
                p.set_double(OFX_PARAM_PROP_DISPLAY_MIN, 0, display_min)?;
                p.set_double(OFX_PARAM_PROP_DISPLAY_MAX, 0, display_max)?;
            }
            ParamKind::Int { min, max, default } => {
                let (display_min, display_max) =
                    display_range(def.name).map_or((min, max), |(lo, hi)| (lo as i32, hi as i32));
                p.set_int(OFX_PARAM_PROP_DEFAULT, 0, default)?;
                p.set_int(OFX_PARAM_PROP_MIN, 0, min)?;
                p.set_int(OFX_PARAM_PROP_MAX, 0, max)?;
                p.set_int(OFX_PARAM_PROP_DISPLAY_MIN, 0, display_min)?;
                p.set_int(OFX_PARAM_PROP_DISPLAY_MAX, 0, display_max)?;
            }
            ParamKind::Bool { default } => {
                // OFX passes booleans as int.
                p.set_int(OFX_PARAM_PROP_DEFAULT, 0, default as c_int)?;
            }
            ParamKind::Choice { options, default } => {
                p.set_int(OFX_PARAM_PROP_DEFAULT, 0, default)?;
                for (i, option) in options.iter().enumerate() {
                    let option = c_string(option)?;
                    p.set_string(OFX_PARAM_PROP_CHOICE_OPTION, i as c_int, &option)?;
                }
            }
        }
    }

    // SAFETY: `param_set` is valid for this action per this function's contract.
    unsafe { define_pages(suites, param_set) }
}

/// Defines each page and lists its top-level groups as children. Parameters are nested in their
/// group by `Parent`, and sub-groups in their parent group. Hosts without paging ignore the pages.
///
/// # Safety
///
/// Same contract as [`define_params`].
pub(super) unsafe fn define_pages(suites: &HostSuites, param_set: OfxParamSetHandle) -> Result<()> {
    for page_def in PAGES {
        let name = c_string(page_def.name)?;
        let mut props: OfxPropertySetHandle = core::ptr::null_mut();
        // SAFETY: as in `define_params`, `param_set` is valid for this action and the strings
        // outlive the call.
        check(unsafe {
            (suites.parameter.paramDefine)(
                param_set,
                OFX_PARAM_TYPE_PAGE.as_ptr(),
                name.as_ptr(),
                &raw mut props,
            )
        })?;
        let page = Props::new(props, suites.property);
        page.set_string(OFX_PROP_LABEL, 0, &c_string(page_def.label)?)?;
        // The page lists its top-level groups; each group lists its parameters through Parent.
        // Natron shows only page children as page content, so a group that is not listed
        // (the sub-groups, nested in Non-Uniform) appears only inside its parent.
        let children = GROUPS
            .iter()
            .filter(|group| group.page == page_def.name && group.parent.is_none());
        for (i, group) in children.enumerate() {
            page.set_string(
                OFX_PARAM_PROP_PAGE_CHILD,
                i as c_int,
                &c_string(group.name)?,
            )?;
        }
    }
    Ok(())
}
