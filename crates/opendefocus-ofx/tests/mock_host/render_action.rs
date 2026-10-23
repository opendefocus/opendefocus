//! Tests of the Render, `GetRegionsOfInterest`, clip preferences and settings-read actions through the mock host, including failure paths.

use super::*;

/// A session past Load, Describe, DescribeInContext(Filter) and `CreateInstance`.
pub(super) fn ready_session() -> Session {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    assert_eq!(session.create_instance(), OFX_STAT_OK);
    session
}

/// Returns the last message text the plugin sent, if any.
pub(super) fn message_text(session: &Session) -> Option<String> {
    session.last_message().map(|(_, text)| text)
}

/// Checks that the instance data is set on create and cleared on destroy.
#[test]
fn instance_data_is_created_and_destroyed() {
    let session = ready_session();
    let data = session.instance_data().expect("instance data is set");
    assert_ne!(data, 0);
    assert_eq!(session.destroy_instance(), OFX_STAT_OK);
    assert_eq!(session.instance_data(), Some(0));
    assert_eq!(session.destroy_instance(), OFX_STAT_OK);
}

/// In-args of `InstanceChanged` for a parameter change, whose kOfxPropType is kOfxTypeParameter.
pub(super) fn parameter_changed_args() -> OfxPropertySetHandle {
    let props = with(State::alloc);
    store(
        (props, OFX_PROP_TYPE.to_string_lossy().into_owned()),
        0,
        Value::Str(OFX_TYPE_PARAMETER.to_owned()),
    );
    props as OfxPropertySetHandle
}

/// The kOfxParamPropEnabled value of a parameter, if the plugin set it.
pub(super) fn enabled_prop(session: &Session, name: &str) -> Option<c_int> {
    prop_int(session.param_props(name)?, OFX_PARAM_PROP_ENABLED, 0)
}

/// Checks that turning an Enable toggle off disables its sub-parameters.
#[test]
fn enable_toggles_grey_out_their_sub_parameters() {
    let session = ready_session();
    // Toggles are off by default, so the gated parameters start disabled.
    assert_eq!(enabled_prop(&session, "catseyeAmount"), Some(0));

    session.set_param_value("catseyeEnable", Value::Int(1));
    assert_eq!(
        session.action(
            OFX_ACTION_INSTANCE_CHANGED,
            session.effect(),
            parameter_changed_args()
        ),
        OFX_STAT_OK
    );
    for name in [
        "catseyeAmount",
        "catseyeGamma",
        "catseyeSoftness",
        "catseyeInverse",
    ] {
        assert_eq!(enabled_prop(&session, name), Some(1), "{name}");
    }
    assert_eq!(enabled_prop(&session, "astigmatismAmount"), Some(0));

    session.set_param_value("catseyeEnable", Value::Int(0));
    session.action(
        OFX_ACTION_INSTANCE_CHANGED,
        session.effect(),
        parameter_changed_args(),
    );
    assert_eq!(enabled_prop(&session, "catseyeAmount"), Some(0));
}

/// Checks that creating an instance sets enabled states from the saved parameter values.
#[test]
fn create_instance_sets_enabled_state_from_saved_values() {
    let session = Session::new(&[]);
    assert_eq!(session.load(), OFX_STAT_OK);
    assert_eq!(session.describe(), OFX_STAT_OK);
    assert_eq!(
        session.describe_in_context(OFX_IMAGE_EFFECT_CONTEXT_FILTER),
        OFX_STAT_OK
    );
    // A saved project with barndoors on. No InstanceChanged is sent.
    session.set_param_value("barndoorsEnable", Value::Int(1));
    assert_eq!(session.create_instance(), OFX_STAT_OK);

    assert_eq!(enabled_prop(&session, "barndoorsTop"), Some(1));
    assert_eq!(enabled_prop(&session, "barndoorsAmount"), Some(1));
    assert_eq!(enabled_prop(&session, "catseyeAmount"), Some(0));
    assert_eq!(enabled_prop(&session, "astigmatismGamma"), Some(0));
}

/// Checks that a render without a source clip fails.
#[test]
fn render_without_source_fails() {
    let session = ready_session();
    let zeros = vec![0.0_f32; 8 * 8 * 4];
    session.set_clip_image("Output", [0, 0, 8, 8], 4, &zeros, false);
    assert_eq!(session.render([0, 0, 8, 8], 0.0), OFX_STAT_FAILED);
    assert!(
        message_text(&session)
            .expect("message")
            .starts_with("Source clip has no image"),
    );
    assert_eq!(session.images_fetched(), 0);
}

/// Checks that depth mode without a connected depth clip fails and reports a message.
#[test]
fn depth_mode_without_connected_depth_fails_with_message() {
    let session = ready_session();
    session.set_param_value("defocusMode", Value::Int(1));
    let zeros = vec![0.0_f32; 8 * 8 * 4];
    session.set_clip_image("Source", [0, 0, 8, 8], 4, &zeros, false);
    session.set_clip_image("Output", [0, 0, 8, 8], 4, &zeros, false);
    assert_eq!(session.render([0, 0, 8, 8], 0.0), OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some("Depth clip is not connected")
    );
    assert_eq!(session.images_fetched(), 0);
}

/// Checks that a render fails when the source bounds do not cover the render window.
#[test]
fn source_bounds_must_cover_the_render_window() {
    let session = ready_session();
    let zeros = vec![0.0_f32; 64 * 64 * 4];
    // The region of definition is the frame; the Source bounds are a smaller block of it.
    session.set_clip_image_with_rod(
        "Source",
        [0, 0, 32, 32],
        [0, 0, 64, 64],
        4,
        &vec![0.0; 32 * 32 * 4],
        false,
    );
    session.set_clip_image("Output", [0, 0, 64, 64], 4, &zeros, false);
    assert_eq!(session.render([0, 0, 64, 64], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with("Source image bounds")
            && text.contains("do not cover the requested region"),
        "message: {text}"
    );
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Checks that a render fails when the window lies outside the source region of definition.
#[test]
fn render_window_must_lie_inside_the_source_region_of_definition() {
    let session = ready_session();
    // The bounds cover the window, but the region of definition (the frame) is smaller.
    session.set_clip_image_with_rod(
        "Source",
        [0, 0, 64, 64],
        [0, 0, 32, 32],
        4,
        &vec![0.0; 64 * 64 * 4],
        false,
    );
    session.set_clip_image("Output", [0, 0, 64, 64], 4, &vec![0.0; 64 * 64 * 4], false);
    assert_eq!(session.render([0, 0, 64, 64], 0.0), OFX_STAT_FAILED);
    let text = message_text(&session).unwrap_or_default();
    assert!(
        text.starts_with("render window must lie inside the Source region of definition"),
        "message: {text}"
    );
    assert_eq!(session.images_fetched(), session.images_released());
}

/// Checks that `GetRegionsOfInterest` without in-args fails.
#[test]
fn regions_of_interest_without_in_args_fails() {
    let session = ready_session();
    let (status, _) = session.regions_of_interest(None, 0.0);
    assert_eq!(status, OFX_STAT_FAILED);
    assert_eq!(
        message_text(&session).as_deref(),
        Some("regions of interest arguments are missing")
    );
}

/// Checks that an empty region request leaves the host's default region of interest in place.
#[test]
fn regions_of_interest_for_an_empty_window_keeps_the_host_default() {
    let session = ready_session();
    session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
    let (status, out) = session.regions_of_interest(Some([0.0; 4]), 0.0);
    assert_eq!(status, OFX_STAT_OK);
    assert_eq!(prop_count(out, c"OfxImageClipPropRoI_Source"), 0);
}

/// Checks that every fetched image is released when a render fails.
#[test]
fn images_are_released_on_every_failure_path() {
    // Depth is fetched and rejected after Source and Output were fetched.
    {
        let session = ready_session();
        session.set_param_value("defocusMode", Value::Int(1));
        session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
        session.set_clip_image("Depth", [0, 0, 8, 8], 4, &vec![0.0; 8 * 8 * 4], false);
        session.set_clip_image("Output", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
        assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
        assert_eq!(session.images_fetched(), 3);
        assert_eq!(session.images_released(), 3);
    }
    // Output is fetched and rejected after Source was fetched.
    {
        let session = ready_session();
        session.set_clip_image("Source", [0, 0, 16, 16], 4, &vec![0.0; 16 * 16 * 4], false);
        session.set_clip_image("Output", [0, 0, 8, 8], 4, &vec![0.0; 8 * 8 * 4], false);
        assert_eq!(session.render([0, 0, 16, 16], 0.0), OFX_STAT_FAILED);
        assert_eq!(session.images_fetched(), 2);
        assert_eq!(session.images_released(), 2);
    }
}

/// Checks that a render without in-args fails.
#[test]
fn render_without_in_args_fails() {
    let session = ready_session();
    assert_eq!(session.render_without_args(), OFX_STAT_FAILED);
    assert_eq!(session.images_fetched(), 0);
}

/// Checks that clip preferences declare float premultiplied RGBA output.
#[test]
fn clip_preferences_declare_float_premultiplied_rgba_output() {
    let session = ready_session();
    let (status, out) = session.clip_preferences();
    assert_eq!(status, OFX_STAT_OK);
    assert_eq!(
        prop_string(out, OFX_IMAGE_CLIP_PROP_COMPONENTS_OUTPUT, 0).as_deref(),
        Some("OfxImageComponentRGBA")
    );
    assert_eq!(
        prop_string(out, OFX_IMAGE_CLIP_PROP_DEPTH_OUTPUT, 0).as_deref(),
        Some("OfxBitDepthFloat")
    );
    // kOfxImagePreMultiplied; the string value differs from its macro name by ABI.
    assert_eq!(
        prop_string(out, OFX_IMAGE_EFFECT_PROP_PRE_MULTIPLICATION, 0).as_deref(),
        Some("OfxImageAlphaPremultiplied")
    );
}

/// Checks that clip preferences without out-args fail.
#[test]
fn clip_preferences_without_out_args_fails() {
    let session = ready_session();
    assert_eq!(
        session.action(
            OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES,
            session.effect(),
            ptr::null_mut(),
        ),
        OFX_STAT_FAILED
    );
}

/// Checks that settings read through the mock host equal the defaults.
#[test]
fn read_settings_through_the_mock_host_matches_defaults() {
    let session = ready_session();
    // SAFETY: the handle is the parameter set the mock returns from getParamSet for this effect.
    let settings = unsafe { read_settings(session.param_set(), 0.0) }.expect("settings are read");
    let mut expected = default_settings();
    expected.render.gui = false;
    expected.render.use_gpu_if_available = true;
    assert_eq!(settings, expected);
}

/// Sets a non-default value in every group, reads the settings back and checks each group.
#[test]
fn read_settings_maps_each_group_from_the_param_set() {
    use opendefocus::datamodel::bokeh_creator::FilterType;
    use opendefocus::datamodel::circle_of_confusion::{CameraData, Filmback, WorldUnit};
    use opendefocus::datamodel::non_uniform::AxialAberrationType;
    use opendefocus::datamodel::render::{FilterMode, Quality};

    let session = ready_session();
    session.set_param_value("defocusMode", Value::Int(1));
    session.set_param_value("size", Value::Double(3.0));
    session.set_param_value("maxSize", Value::Double(12.0));
    session.set_param_value("focalPlane", Value::Double(4.0));
    session.set_param_value("math", Value::Int(2));
    session.set_param_value("protect", Value::Double(2.5));
    session.set_param_value("sizeMultiplier", Value::Double(1.5));
    session.set_param_value("gamma", Value::Double(2.0));
    session.set_param_value("quality", Value::Int(Quality::Fast as i32));
    session.set_param_value("cameraEnabled", Value::Int(1));
    session.set_param_value("focalLength", Value::Double(35.0));
    session.set_param_value("fStop", Value::Double(2.8));
    session.set_param_value("filmbackWidth", Value::Double(36.0));
    session.set_param_value("filmbackHeight", Value::Double(24.0));
    session.set_param_value("worldUnit", Value::Int(WorldUnit::Mm as i32));
    session.set_param_value("inverseForeground", Value::Int(0));
    session.set_param_value("catseyeEnable", Value::Int(1));
    session.set_param_value("catseyeAmount", Value::Double(1.5));
    session.set_param_value("catseyeGamma", Value::Double(0.8));
    session.set_param_value("catseyeSoftness", Value::Double(0.3));
    session.set_param_value("catseyeInverse", Value::Int(1));
    session.set_param_value("axialAberrationEnable", Value::Int(1));
    session.set_param_value("axialAberrationAmount", Value::Double(2.0));
    session.set_param_value(
        "axialAberrationColorType",
        Value::Int(AxialAberrationType::GreenPurple as i32),
    );
    session.set_param_value("astigmatismEnable", Value::Int(1));
    session.set_param_value("astigmatismAmount", Value::Double(0.75));
    session.set_param_value("astigmatismGamma", Value::Double(1.25));
    session.set_param_value("barndoorsEnable", Value::Int(1));
    session.set_param_value("barndoorsAmount", Value::Double(3.0));
    session.set_param_value("barndoorsGamma", Value::Double(0.5));
    session.set_param_value("barndoorsInverse", Value::Int(1));
    session.set_param_value("barndoorsTop", Value::Double(10.0));
    session.set_param_value("barndoorsBottom", Value::Double(20.0));
    session.set_param_value("barndoorsLeft", Value::Double(30.0));
    session.set_param_value("barndoorsRight", Value::Double(40.0));
    session.set_param_value("bokehFilterType", Value::Int(2));
    session.set_param_value("bokehRadius", Value::Double(2.0));
    session.set_param_value("bokehRingColor", Value::Double(0.5));
    session.set_param_value("bokehInnerColor", Value::Double(0.25));
    session.set_param_value("bokehRingSize", Value::Double(0.2));
    session.set_param_value("bokehOuterBlur", Value::Double(0.3));
    session.set_param_value("bokehInnerBlur", Value::Double(0.4));
    session.set_param_value("bokehBlades", Value::Int(7));
    session.set_param_value("bokehAngle", Value::Double(45.0));
    session.set_param_value("bokehCurvature", Value::Double(0.1));
    session.set_param_value("bokehAspectRatio", Value::Double(1.5));
    session.set_param_value("bokehNoiseSize", Value::Double(0.7));
    session.set_param_value("bokehNoiseIntensity", Value::Double(0.6));
    session.set_param_value("bokehNoiseOctaves", Value::Int(3));
    session.set_param_value("bokehNoiseSeed", Value::Int(42));

    // SAFETY: the handle is the parameter set the mock returns from getParamSet for this effect.
    let settings = unsafe { read_settings(session.param_set(), 0.0) }.expect("settings are read");

    let mut expected = default_settings();
    let coc = &mut expected.defocus.circle_of_confusion;
    expected.defocus.defocus_mode = 1;
    coc.size = 3.0;
    coc.max_size = 12.0;
    coc.focal_plane = 4.0;
    coc.protect = 2.5;
    expected.defocus.use_direct_math = true;
    expected.defocus.size_multiplier = 1.5;
    expected.defocus.gamma_correction = 2.0;
    expected.render.quality = Quality::Fast as i32;
    expected.render.farm_quality = Quality::Fast as i32;
    expected.render.gui = false;
    expected.render.use_gpu_if_available = true;
    let camera = CameraData {
        focal_length: 35.0,
        f_stop: 2.8,
        filmback: Filmback {
            width: 36.0,
            height: 24.0,
        },
        world_unit: WorldUnit::Mm as i32,
        ..CameraData::default()
    };
    expected.defocus.circle_of_confusion.camera_data = Some(camera);

    let non_uniform = &mut expected.non_uniform;
    non_uniform.inverse_foreground = false;
    non_uniform.catseye.enable = true;
    non_uniform.catseye.amount = 1.5;
    non_uniform.catseye.gamma = 0.8;
    non_uniform.catseye.softness = 0.3;
    non_uniform.catseye.inverse = true;
    non_uniform.axial_aberration.enable = true;
    non_uniform.axial_aberration.amount = 2.0;
    non_uniform.axial_aberration.color_type = AxialAberrationType::GreenPurple as i32;
    non_uniform.astigmatism.enable = true;
    non_uniform.astigmatism.amount = 0.75;
    non_uniform.astigmatism.gamma = 1.25;
    non_uniform.barndoors.enable = true;
    non_uniform.barndoors.amount = 3.0;
    non_uniform.barndoors.gamma = 0.5;
    non_uniform.barndoors.inverse = true;
    non_uniform.barndoors.top = 10.0;
    non_uniform.barndoors.bottom = 20.0;
    non_uniform.barndoors.left = 30.0;
    non_uniform.barndoors.right = 40.0;

    expected.render.filter.set_mode(FilterMode::BokehCreator);
    let bokeh = &mut expected.bokeh;
    bokeh.filter_type = FilterType::Blade as i32;
    bokeh.radius = 2.0;
    bokeh.ring_color = 0.5;
    bokeh.inner_color = 0.25;
    bokeh.ring_size = 0.2;
    bokeh.outer_blur = 0.3;
    bokeh.inner_blur = 0.4;
    bokeh.blades = 7;
    bokeh.angle = 45.0;
    bokeh.curvature = 0.1;
    bokeh.aspect_ratio = 1.5;
    bokeh.noise.size = 0.7;
    bokeh.noise.intensity = 0.6;
    bokeh.noise.octaves = 3;
    bokeh.noise.seed = 42;

    assert_eq!(
        settings.defocus.circle_of_confusion.camera_data,
        expected.defocus.circle_of_confusion.camera_data,
        "camera group"
    );
    assert_eq!(settings.defocus, expected.defocus, "defocus group");
    assert_eq!(settings.bokeh, expected.bokeh, "bokeh group");
    assert_eq!(
        settings.non_uniform, expected.non_uniform,
        "non-uniform groups"
    );
    assert_eq!(settings.render, expected.render, "render group");
}

/// Filter Type choices select the render filter mode and the bokeh filter type. Simple leaves
/// the bokeh settings unused, so the mode stays Simple.
#[test]
fn bokeh_filter_type_choices_set_the_filter_mode() {
    use opendefocus::datamodel::bokeh_creator::FilterType;
    use opendefocus::datamodel::render::FilterMode;

    let session = ready_session();
    let read = |session: &Session| {
        // SAFETY: the handle is the parameter set the mock returns from getParamSet for this effect.
        unsafe { read_settings(session.param_set(), 0.0) }.expect("settings are read")
    };

    // The default is Simple, which matches Settings::default().
    let settings = read(&session);
    assert_eq!(settings.render.filter.mode(), FilterMode::Simple);
    assert_eq!(settings.bokeh.filter_type, FilterType::Disc as i32);

    session.set_param_value("bokehFilterType", Value::Int(0));
    let settings = read(&session);
    assert_eq!(settings.render.filter.mode(), FilterMode::Simple);

    session.set_param_value("bokehFilterType", Value::Int(1));
    let settings = read(&session);
    assert_eq!(settings.render.filter.mode(), FilterMode::BokehCreator);
    assert_eq!(settings.bokeh.filter_type, FilterType::Disc as i32);

    session.set_param_value("bokehFilterType", Value::Int(2));
    let settings = read(&session);
    assert_eq!(settings.render.filter.mode(), FilterMode::BokehCreator);
    assert_eq!(settings.bokeh.filter_type, FilterType::Blade as i32);
}

/// Bokeh knobs are greyed out under Simple, and the blade count only applies to Blade.
#[test]
fn bokeh_knobs_follow_the_filter_type_choice() {
    let session = ready_session();
    assert_eq!(enabled_prop(&session, "bokehRadius"), Some(0));
    assert_eq!(enabled_prop(&session, "bokehBlades"), Some(0));

    let change = |session: &Session| {
        assert_eq!(
            session.action(
                OFX_ACTION_INSTANCE_CHANGED,
                session.effect(),
                parameter_changed_args()
            ),
            OFX_STAT_OK
        );
    };

    session.set_param_value("bokehFilterType", Value::Int(1));
    change(&session);
    assert_eq!(enabled_prop(&session, "bokehRadius"), Some(1));
    assert_eq!(enabled_prop(&session, "bokehNoiseSeed"), Some(1));
    assert_eq!(enabled_prop(&session, "bokehBlades"), Some(0));

    session.set_param_value("bokehFilterType", Value::Int(2));
    change(&session);
    assert_eq!(enabled_prop(&session, "bokehRadius"), Some(1));
    assert_eq!(enabled_prop(&session, "bokehBlades"), Some(1));

    session.set_param_value("bokehFilterType", Value::Int(0));
    change(&session);
    assert_eq!(enabled_prop(&session, "bokehRadius"), Some(0));
    assert_eq!(enabled_prop(&session, "bokehBlades"), Some(0));
}
