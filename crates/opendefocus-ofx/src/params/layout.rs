//! Page and group layout of the parameter set: the pages, the groups nested under them, and
//! the page order written to the host. The tables here decide where each parameter appears in the UI.

use super::{OFX_PLUGIN_PROP_PARAM_PAGE_ORDER, Props, Result, c_int, c_string};

/// A parameter page: the tab a host shows its children on.
#[derive(Clone, Copy)]
pub struct PageDef {
    /// Name in the parameter set. Must differ from every parameter and group name.
    pub name: &'static str,
    /// Tab title shown to the user.
    pub label: &'static str,
}

/// Pages in display order (kOfxPluginPropParamPageOrder). Every leaf parameter is the child of
/// exactly one page, through its group's page (see [`GroupDef::page`]).
pub const PAGES: &[PageDef] = &[
    PageDef {
        name: "pageDefocus",
        label: "Defocus",
    },
    PageDef {
        name: "pageCamera",
        label: "Camera",
    },
    PageDef {
        name: "pageBokeh",
        label: "Bokeh",
    },
    PageDef {
        name: "pageNonUniform",
        label: "Non-Uniform",
    },
];

/// A group (sub-section) that parameters are placed in.
#[derive(Clone, Copy)]
pub struct GroupDef {
    /// Name in the parameter set, referenced by each parameter's `group`.
    pub name: &'static str,
    /// Section title shown to the user.
    pub label: &'static str,
    /// The page whose layout lists this group's parameters.
    pub page: &'static str,
    /// The group this one is nested in, for sub-sections.
    pub parent: Option<&'static str>,
}

/// Groups in display order. Every parameter's `group` must name one of these. Groups are not
/// page children (OFX allows only non-group parameters there), so a group's page is the page of
/// its parameters.
pub const GROUPS: &[GroupDef] = &[
    GroupDef {
        name: "defocus",
        label: "Defocus",
        page: "pageDefocus",
        parent: None,
    },
    GroupDef {
        name: "camera",
        label: "Camera",
        page: "pageCamera",
        parent: None,
    },
    GroupDef {
        name: "bokeh",
        label: "Bokeh",
        page: "pageBokeh",
        parent: None,
    },
    GroupDef {
        name: "nonUniform",
        label: "Non-Uniform",
        page: "pageNonUniform",
        parent: None,
    },
    GroupDef {
        name: "catseye",
        label: "Catseye",
        page: "pageNonUniform",
        parent: Some("nonUniform"),
    },
    GroupDef {
        name: "axialAberration",
        label: "Axial Aberration",
        page: "pageNonUniform",
        parent: Some("nonUniform"),
    },
    GroupDef {
        name: "astigmatism",
        label: "Astigmatism",
        page: "pageNonUniform",
        parent: Some("nonUniform"),
    },
    GroupDef {
        name: "barndoors",
        label: "Barndoors",
        page: "pageNonUniform",
        parent: Some("nonUniform"),
    },
];

/// Returns the page that lists the parameters of `group`, or `None` if no group in [`GROUPS`] has
/// that name.
#[must_use]
pub fn page_of(group: &str) -> Option<&'static str> {
    GROUPS.iter().find(|g| g.name == group).map(|g| g.page)
}

/// Sets kOfxPluginPropParamPageOrder on the effect property set (`DescribeInContext`).
///
/// # Errors
///
/// Returns [`OfxError`] if the host rejects the property.
///
/// # Panics
///
/// Panics if a page name in the static table contains a NUL byte, which it never does.
pub fn set_page_order(props: &Props<'_>) -> Result<()> {
    for (i, page) in PAGES.iter().enumerate() {
        props.set_string(
            OFX_PLUGIN_PROP_PARAM_PAGE_ORDER,
            i as c_int,
            &c_string(page.name)?,
        )?;
    }
    Ok(())
}
