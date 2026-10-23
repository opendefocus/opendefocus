//! Checks the generated enum conversions and the typed getters that fall back on unknown raw values.
use super::*;
use super::{
    defocus::DefocusMode,
    non_uniform::AxialAberrationType,
    render::{Filter, FilterMode, Quality, ResultMode, Settings as RenderSettings},
};

#[test]
/// Verifies the unknown-enum error names the rejected raw value.
fn unknown_enum_value_displays_the_raw_value() {
    assert_eq!(
        UnknownEnumValue(7).to_string(),
        "unknown enumeration value 7"
    );
}

#[test]
/// Verifies defocus mode raw values, names and conversions, rejecting unknown raw values.
fn defocus_mode_conversions_cover_valid_and_invalid_values() {
    assert_eq!(DefocusMode::try_from(0), Ok(DefocusMode::Twod));
    assert_eq!(DefocusMode::try_from(1), Ok(DefocusMode::Depth));
    assert_eq!(DefocusMode::try_from(2), Err(UnknownEnumValue(2)));
    assert!(DefocusMode::is_valid(1));
    assert!(!DefocusMode::is_valid(-1));
    assert_eq!(DefocusMode::Depth.as_str_name(), "DEPTH");
    assert_eq!(DefocusMode::from_str_name("TWOD"), Some(DefocusMode::Twod));
    assert_eq!(DefocusMode::from_str_name("twod"), None);
    assert_eq!(i32::from(DefocusMode::Depth), 1);
}

#[test]
/// Verifies the defocus getters fall back to 2D for unknown modes and the proxy scale to 1.
fn defocus_getters_fall_back_to_defaults() {
    let mut settings = defocus::Settings {
        defocus_mode: 9,
        ..Default::default()
    };
    assert_eq!(settings.defocus_mode(), DefocusMode::Twod);
    settings.set_defocus_mode(DefocusMode::Depth);
    assert_eq!(settings.defocus_mode, 1);
    assert_eq!(settings.defocus_mode(), DefocusMode::Depth);

    assert!((settings.proxy_scale() - 1.0).abs() < f32::EPSILON);
    settings.proxy_scale = Some(0.25);
    assert!((settings.proxy_scale() - 0.25).abs() < f32::EPSILON);
}

#[test]
/// Verifies axial aberration colour raw values, names, and the fallback for unknown values.
fn axial_aberration_type_conversions_and_fallback() {
    assert_eq!(
        AxialAberrationType::try_from(2),
        Ok(AxialAberrationType::GreenPurple)
    );
    assert_eq!(AxialAberrationType::try_from(3), Err(UnknownEnumValue(3)));
    assert!(AxialAberrationType::is_valid(0));
    assert!(!AxialAberrationType::is_valid(3));
    assert_eq!(
        AxialAberrationType::from_str_name("BLUE_YELLOW"),
        Some(AxialAberrationType::BlueYellow)
    );
    assert_eq!(AxialAberrationType::BlueYellow.as_str_name(), "BLUE_YELLOW");
    assert_eq!(i32::from(AxialAberrationType::GreenPurple), 2);

    let mut aberration = non_uniform::AxialAberration {
        color_type: 42,
        ..Default::default()
    };
    assert_eq!(aberration.color_type(), AxialAberrationType::RedBlue);
    aberration.set_color_type(AxialAberrationType::BlueYellow);
    assert_eq!(aberration.color_type, 1);
    assert_eq!(aberration.color_type(), AxialAberrationType::BlueYellow);
}

#[test]
/// Verifies quality raw values, names and conversions, rejecting values past accurate.
fn quality_conversions_cover_both_modes() {
    for (raw, quality) in [(0, Quality::Fast), (1, Quality::Accurate)] {
        assert_eq!(Quality::try_from(raw), Ok(quality));
        assert_eq!(i32::from(quality), raw);
        assert!(Quality::is_valid(raw));
    }
    assert_eq!(Quality::try_from(2), Err(UnknownEnumValue(2)));
    assert!(!Quality::is_valid(-1));
    assert!(!Quality::is_valid(2));
    assert_eq!(Quality::from_str_name("ACCURATE"), Some(Quality::Accurate));
    assert_eq!(Quality::Accurate.as_str_name(), "ACCURATE");
    assert_eq!(Quality::from_str_name("ULTRA"), None);
}

#[test]
/// Verifies filter mode raw values skip 2, and names round-trip.
fn filter_mode_conversions_skip_two() {
    assert_eq!(FilterMode::try_from(3), Ok(FilterMode::Image));
    assert_eq!(FilterMode::try_from(2), Err(UnknownEnumValue(2)));
    assert!(FilterMode::is_valid(1));
    assert!(!FilterMode::is_valid(2));
    assert_eq!(i32::from(FilterMode::Image), 3);
    assert_eq!(FilterMode::BokehCreator.as_str_name(), "BOKEH_CREATOR");
    assert_eq!(FilterMode::from_str_name("IMAGE"), Some(FilterMode::Image));
    assert_eq!(FilterMode::from_str_name("image"), None);
}

#[test]
/// Verifies result mode raw values, names and conversions.
fn result_mode_conversions_cover_both_modes() {
    assert_eq!(ResultMode::try_from(1), Ok(ResultMode::FocalPlaneSetup));
    assert_eq!(ResultMode::try_from(2), Err(UnknownEnumValue(2)));
    assert!(ResultMode::is_valid(0));
    assert!(!ResultMode::is_valid(2));
    assert_eq!(
        ResultMode::FocalPlaneSetup.as_str_name(),
        "FOCAL_PLANE_SETUP"
    );
    assert_eq!(
        ResultMode::from_str_name("RESULT"),
        Some(ResultMode::Result)
    );
    assert_eq!(i32::from(ResultMode::FocalPlaneSetup), 1);
}

#[test]
/// Verifies the render getters fall back to their defaults for unknown raw values and unset options.
fn render_getters_fall_back_for_unknown_values() {
    let mut settings = RenderSettings {
        result_mode: 9,
        quality: 9,
        farm_quality: 9,
        ..Default::default()
    };
    assert_eq!(settings.result_mode(), ResultMode::Result);
    assert_eq!(settings.quality(), Quality::Fast);
    assert_eq!(settings.farm_quality(), Quality::Accurate);
    assert_eq!(settings.timeout(), 60);
    assert_eq!(settings.device_name(), "CPU - Native");

    settings.set_result_mode(ResultMode::FocalPlaneSetup);
    settings.set_quality(Quality::Accurate);
    settings.set_farm_quality(Quality::Fast);
    settings.timeout = Some(5);
    settings.device_name = Some("GPU".to_owned());
    assert_eq!(settings.result_mode(), ResultMode::FocalPlaneSetup);
    assert_eq!(settings.quality(), Quality::Accurate);
    assert_eq!(settings.farm_quality(), Quality::Fast);
    assert_eq!(settings.timeout(), 5);
    assert_eq!(settings.device_name(), "GPU");
}

#[test]
/// Verifies the effective quality follows the GUI flag and the filter mode getter falls back for unknown values.
fn effective_quality_follows_gui_flag_and_filter_mode_falls_back() {
    let mut settings = RenderSettings {
        gui: true,
        quality: Quality::Fast as i32,
        farm_quality: Quality::Accurate as i32,
        ..Default::default()
    };
    assert_eq!(settings.get_quality(), Quality::Fast);
    settings.gui = false;
    assert_eq!(settings.get_quality(), Quality::Accurate);

    let mut filter = Filter {
        preview: false,
        resolution: 64,
        mode: 2,
    };
    assert_eq!(filter.mode(), FilterMode::Simple);
    filter.set_mode(FilterMode::Image);
    assert_eq!(filter.mode, 3);
    assert_eq!(filter.mode(), FilterMode::Image);
}

#[test]
/// Verifies every enum variant has the stable field name that `from_str_name` maps back to it, and unknown names are rejected.
fn enum_field_names_round_trip_for_every_variant() {
    let cases: Vec<(&str, DefocusMode)> =
        vec![("TWOD", DefocusMode::Twod), ("DEPTH", DefocusMode::Depth)];
    for (name, variant) in cases {
        assert_eq!(variant.as_str_name(), name);
        assert_eq!(DefocusMode::from_str_name(name), Some(variant));
    }
    assert_eq!(DefocusMode::from_str_name("depth"), None);

    let cases = [
        ("RED_BLUE", AxialAberrationType::RedBlue),
        ("BLUE_YELLOW", AxialAberrationType::BlueYellow),
        ("GREEN_PURPLE", AxialAberrationType::GreenPurple),
    ];
    for (name, variant) in cases {
        assert_eq!(variant.as_str_name(), name);
        assert_eq!(AxialAberrationType::from_str_name(name), Some(variant));
    }
    assert_eq!(AxialAberrationType::from_str_name("RED"), None);
    assert_eq!(
        AxialAberrationType::try_from(0),
        Ok(AxialAberrationType::RedBlue)
    );
    assert_eq!(
        AxialAberrationType::try_from(1),
        Ok(AxialAberrationType::BlueYellow)
    );

    let cases = [("FAST", Quality::Fast), ("ACCURATE", Quality::Accurate)];
    for (name, variant) in cases {
        assert_eq!(variant.as_str_name(), name);
        assert_eq!(Quality::from_str_name(name), Some(variant));
    }
    assert_eq!(Quality::from_str_name("ULTRA"), None);

    let cases = [
        ("SIMPLE", FilterMode::Simple),
        ("BOKEH_CREATOR", FilterMode::BokehCreator),
        ("IMAGE", FilterMode::Image),
    ];
    for (name, variant) in cases {
        assert_eq!(variant.as_str_name(), name);
        assert_eq!(FilterMode::from_str_name(name), Some(variant));
    }
    assert_eq!(FilterMode::from_str_name("IMAGE_ONLY"), None);

    let cases = [
        ("RESULT", ResultMode::Result),
        ("FOCAL_PLANE_SETUP", ResultMode::FocalPlaneSetup),
    ];
    for (name, variant) in cases {
        assert_eq!(variant.as_str_name(), name);
        assert_eq!(ResultMode::from_str_name(name), Some(variant));
    }
    assert_eq!(ResultMode::from_str_name("RESULTS"), None);
}
