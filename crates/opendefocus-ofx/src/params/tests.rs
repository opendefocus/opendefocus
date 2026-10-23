//! Unit tests for the parameter table: row names, page and group consistency, display ranges and
//! the table defaults against `Settings::default()`.

use super::*;

/// Checks that no page name is reused as a parameter or group name.
#[test]
fn page_names_differ_from_parameters_and_groups() {
    for page in PAGES {
        assert!(PARAMS.iter().all(|def| def.name != page.name));
        assert!(GROUPS.iter().all(|group| group.name != page.name));
    }
}

/// Checks that each group is placed on an existing page and each parent names an existing group.
#[test]
fn every_group_is_on_a_page_and_every_parent_is_a_group() {
    for group in GROUPS {
        assert!(PAGES.iter().any(|page| page.name == group.page));
        if let Some(parent) = group.parent {
            assert!(GROUPS.iter().any(|g| g.name == parent), "{parent}");
        }
    }
}

/// Checks that every display range names a numeric parameter and lies inside its hard range.
#[test]
fn display_ranges_name_parameters_and_lie_inside_the_hard_range() {
    for &(name, lo, hi) in DISPLAY_RANGES {
        let def = PARAMS
            .iter()
            .find(|def| def.name == name)
            .unwrap_or_else(|| panic!("{name} is a parameter"));
        let (min, max) = match def.kind {
            ParamKind::Double { min, max, .. } => (min, max),
            ParamKind::Int { min, max, .. } => (f64::from(min), f64::from(max)),
            _ => panic!("{name} is a numeric parameter"),
        };
        assert!(min <= lo && lo < hi && hi <= max, "{name}");
    }
}

/// The settings a default CLI render uses: `Settings::default()` with the farm quality
/// applied to both quality fields. The CLI renders with farm quality, so the expected
/// quality is `Settings::default().render.farm_quality`, not `quality` (Fast).
/// Deriving it here keeps this test in step with the core default.
fn expected_defaults() -> Settings {
    let mut expected = Settings::default();
    let farm_quality = expected.render.farm_quality;
    expected.render.quality = farm_quality;
    expected
}

/// Checks that applying the table defaults equals the expected default settings.
#[test]
fn table_defaults_equal_settings_default() {
    assert_eq!(default_settings(), expected_defaults());
}

/// Field-by-field check of the exposed fields, so a mismatch names the field. Quality is
/// the one exception: the table default is Accurate, the farm quality the CLI renders with,
/// while `Settings::default().render.quality` is Fast.
#[test]
fn exposed_defaults_match_settings_default_field_by_field() {
    let table = default_settings();
    let core = Settings::default();
    let (t, c) = (&table.defocus, &core.defocus);
    let (tc, cc) = (&t.circle_of_confusion, &c.circle_of_confusion);
    assert_eq!(t.defocus_mode, c.defocus_mode, "defocus_mode");
    assert_eq!(tc.size, cc.size, "size");
    assert_eq!(tc.max_size, cc.max_size, "max_size");
    assert_eq!(tc.focal_plane, cc.focal_plane, "focal_plane");
    assert_eq!(tc.math, cc.math, "math");
    assert_eq!(tc.protect, cc.protect, "protect");
    assert_eq!(t.use_direct_math, c.use_direct_math, "use_direct_math");
    assert_eq!(t.size_multiplier, c.size_multiplier, "size_multiplier");
    assert_eq!(t.gamma_correction, c.gamma_correction, "gamma_correction");
    assert_eq!(tc.camera_data, cc.camera_data, "camera_data");
    assert_eq!(table.bokeh, core.bokeh, "bokeh");
    assert_eq!(table.non_uniform, core.non_uniform, "non_uniform");
    assert_eq!(table.render.quality, Quality::Accurate as i32, "quality");
    assert_eq!(
        table.render.farm_quality, core.render.farm_quality,
        "farm_quality"
    );
}

/// The camera rows must default to the values `CameraData` defaults to, so enabling the
/// camera from the plain rows gives the same data as the core default.
#[test]
fn camera_data_defaults_equal_the_camera_rows() {
    let camera = CameraData::default();
    let row = |name: &str| {
        PARAMS
            .iter()
            .find(|def| def.name == name)
            .expect("camera row")
            .default_value()
    };
    assert_eq!(
        float(row("focalLength")),
        camera.focal_length,
        "focal length"
    );
    assert_eq!(float(row("fStop")), camera.f_stop, "f-stop");
    assert_eq!(
        float(row("filmbackWidth")),
        camera.filmback.width,
        "filmback width"
    );
    assert_eq!(
        float(row("filmbackHeight")),
        camera.filmback.height,
        "filmback height"
    );
    assert_eq!(choice(row("worldUnit")), camera.world_unit, "world unit");
}

/// Checks that the camera toggle is listed before every other camera row.
#[test]
fn camera_toggle_precedes_camera_rows() {
    let toggle = PARAMS
        .iter()
        .position(|def| def.name == "cameraEnabled")
        .expect("camera toggle");
    for (i, def) in PARAMS.iter().enumerate() {
        if def.group == "camera" && def.name != "cameraEnabled" {
            assert!(i > toggle, "{} must follow cameraEnabled", def.name);
        }
    }
}

/// Checks that applying the defaults overwrites values that were changed before.
#[test]
fn applying_defaults_overwrites_scrambled_fields() {
    let mut settings = Settings::default();
    settings.defocus.defocus_mode = 1;
    settings.defocus.circle_of_confusion.size = 42.0;
    settings.defocus.circle_of_confusion.max_size = 43.0;
    settings.defocus.circle_of_confusion.focal_plane = 44.0;
    settings.render.quality = Quality::Fast as i32;
    for def in PARAMS {
        (def.apply)(&mut settings, def.default_value());
    }
    assert_eq!(settings, expected_defaults());
}

/// Checks that parameter names are unique and each parameter's group exists.
#[test]
fn parameter_names_are_unique_and_groups_exist() {
    for (i, def) in PARAMS.iter().enumerate() {
        assert!(!PARAMS[..i].iter().any(|other| other.name == def.name));
        assert!(GROUPS.iter().any(|g| g.name == def.group));
    }
}
