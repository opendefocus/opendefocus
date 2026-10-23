//! Tests that every parameter is reached from exactly one page in page order, and that display ranges reach the host.

use super::*;

/// Each page lists its own top-level groups, in page order. Every parameter is reached from
/// exactly one page through its group chain (Parent), and no page lists another page's group.
#[test]
fn every_parameter_is_reached_from_exactly_one_page_in_page_order() {
    let session = ready_session();
    let order = session.effect_props();
    assert_eq!(
        prop_count(order, OFX_PLUGIN_PROP_PARAM_PAGE_ORDER),
        PAGES.len()
    );
    let mut listed: HashMap<String, &str> = HashMap::new();
    for (i, page) in PAGES.iter().enumerate() {
        assert_eq!(
            string_at(order, OFX_PLUGIN_PROP_PARAM_PAGE_ORDER, i as c_int),
            page.name
        );
        let props = session.param_props(page.name).expect("page is defined");
        for child in 0..prop_count(props, OFX_PARAM_PROP_PAGE_CHILD) {
            let name = string_at(props, OFX_PARAM_PROP_PAGE_CHILD, child as c_int);
            let group = GROUPS
                .iter()
                .find(|group| group.name == name)
                .unwrap_or_else(|| panic!("{name} is a top-level group"));
            assert!(group.parent.is_none(), "{name} is nested, not a page child");
            assert_eq!(group.page, page.name, "{name} is on its own page");
            assert!(
                listed.insert(name.clone(), page.name).is_none(),
                "{name} is on more than one page"
            );
        }
    }
    for def in PARAMS {
        let mut top = def.group;
        while let Some(parent) = GROUPS.iter().find(|g| g.name == top).and_then(|g| g.parent) {
            top = parent;
        }
        assert_eq!(
            listed.get(top).copied(),
            page_of(def.group),
            "{} is reached from its page",
            def.name
        );
        let props = session.param_props(def.name).expect("parameter is defined");
        assert_eq!(string_at(props, OFX_PARAM_PROP_PARENT, 0), def.group);
    }
    assert_eq!(
        listed.len(),
        GROUPS.iter().filter(|g| g.parent.is_none()).count()
    );
}

/// The Nuke slider ranges reach the host as display ranges, and every display range lies inside
/// the hard range. Parameters without a Nuke range keep the hard range.
#[test]
fn display_ranges_reach_the_host() {
    let session = ready_session();
    for def in PARAMS {
        let props = session.param_props(def.name).expect("parameter is defined");
        let display = match def.kind {
            ParamKind::Double { min, max, .. } => {
                let shown = (
                    prop_double(props, OFX_PARAM_PROP_DISPLAY_MIN, 0),
                    prop_double(props, OFX_PARAM_PROP_DISPLAY_MAX, 0),
                );
                assert!(
                    min <= shown.0.unwrap() && shown.1.unwrap() <= max,
                    "{}",
                    def.name
                );
                shown
            }
            ParamKind::Int { min, max, .. } => {
                let shown = (
                    prop_int(props, OFX_PARAM_PROP_DISPLAY_MIN, 0).map(f64::from),
                    prop_int(props, OFX_PARAM_PROP_DISPLAY_MAX, 0).map(f64::from),
                );
                assert!(
                    f64::from(min) <= shown.0.unwrap() && shown.1.unwrap() <= f64::from(max),
                    "{}",
                    def.name
                );
                shown
            }
            _ => continue,
        };
        if let Some(&(_, lo, hi)) = DISPLAY_RANGES.iter().find(|(name, _, _)| *name == def.name) {
            assert_eq!(display, (Some(lo), Some(hi)), "{}", def.name);
        } else {
            let hard = match def.kind {
                ParamKind::Double { min, max, .. } => (min, max),
                ParamKind::Int { min, max, .. } => (f64::from(min), f64::from(max)),
                _ => continue,
            };
            assert_eq!(display, (Some(hard.0), Some(hard.1)), "{}", def.name);
        }
    }
}
