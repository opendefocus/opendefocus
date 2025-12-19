use core::fmt;

use crate::ffi::{
    FilterType, KnobChanged, KnobParameters, KnobType, Mode, NukeSpecificSettings, Op, ValueChange,
    ValueType, input_connected,
};
use bitflags::bitflags;
use convert_case::ccase;
use documented::DocumentedFields;
use strum_macros::EnumIter;

impl KnobChanged {
    pub fn new(enabled: bool, visible: bool) -> Self {
        Self {
            enabled,
            visible,
            set_value: false,
            ..Default::default()
        }
    }

    pub fn with_value_change(&mut self, value: ValueChange) -> &mut Self {
        self.set_value = true;
        self.value_change = value;
        self
    }
    pub fn build(&self) -> Self {
        self.clone()
    }
}

impl ValueChange {
    pub fn text(text: &str) -> Self {
        Self {
            value_type: ValueType::Text,
            text_value: text.to_owned(),
            ..Default::default()
        }
    }
    pub fn float(value: f32) -> Self {
        Self {
            value_type: ValueType::Float,
            float_value: value,
            ..Default::default()
        }
    }
    pub fn bool(value: bool) -> Self {
        Self {
            value_type: ValueType::Bool,
            bool_value: value,
            ..Default::default()
        }
    }
    pub fn int(value: i32) -> Self {
        Self {
            value_type: ValueType::Int,
            int_value: value,
            ..Default::default()
        }
    }
}

#[derive(Debug, EnumIter)]
pub enum KnobDefinition {
    Channels,
    UseGpuIfAvailable,
    DeviceName,
    Mode,
    RenderResult,
    ShowImage,
    DepthChannel,
    Math,
    WorldUnit,
    Quality,
    FarmQuality,
    Samples,
    FocusPlane,
    UseCameraFocal,
    FocusPointUtility,
    ProtectRange,
    Size,
    MaxSize,
    CameraMaxSize,
    GammaCorrection,
    PreviewFilter,
    FilterType,
    FilterResolution,
    RingColor,
    InnerColor,
    RingSize,
    OuterBlur,
    InnerBlur,
    Blades,
    Angle,
    Curvature,
    NoiseSize,
    NoiseIntensity,
    NoiseSeed,
    AspectRatio,
    CatseyeEnable,
    CatseyeAmount,
    CatseyeInverse,
    CatseyeInverseForeground,
    CatseyeGamma,
    CatseyeSoftness,
    CatseyeDimensionBased,
    BarndoorsEnable,
    BarndoorsAmount,
    BarndoorsInverse,
    BarndoorsInverseForeground,
    BarndoorsGamma,
    BarndoorsTop,
    BarndoorsBottom,
    BarndoorsLeft,
    BarndoorsRight,
    AstigmatismEnable,
    AstigmatismAmount,
    AstigmatismGamma,
    AxialAberrationEnable,
    AxialAberrationAmount,
    AxialAberrationType,
    InverseForegroundFilterShape,
    SizeMultiplier,
    FocalPlaneOffset,
    UseCustomStripeHeight,
    CustomStripeHeight,
}
impl fmt::Display for KnobDefinition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl KnobDefinition {
    pub fn knob_type(&self) -> KnobType {
        match &self {
            Self::UseGpuIfAvailable => KnobType::Bool,
            Self::Channels => KnobType::InputChannelSet,
            Self::DeviceName => KnobType::NamedText,
            Self::Mode => KnobType::Enumeration,
            Self::RenderResult => KnobType::Enumeration,
            Self::ShowImage => KnobType::Bool,
            Self::DepthChannel => KnobType::InputOnlyChannel,
            Self::Math => KnobType::Enumeration,
            Self::WorldUnit => KnobType::Enumeration,
            Self::Quality => KnobType::Enumeration,
            Self::FarmQuality => KnobType::Enumeration,
            Self::Samples => KnobType::Int,
            Self::FocusPlane => KnobType::Float,
            Self::UseCameraFocal => KnobType::Bool,
            Self::FocusPointUtility => KnobType::XY,
            Self::ProtectRange => KnobType::Float,
            Self::Size => KnobType::Float,
            Self::MaxSize => KnobType::Float,
            Self::CameraMaxSize => KnobType::Float,
            Self::GammaCorrection => KnobType::Float,
            Self::PreviewFilter => KnobType::Bool,
            Self::FilterType => KnobType::Enumeration,
            Self::FilterResolution => KnobType::Int,
            Self::RingColor => KnobType::Float,
            Self::InnerColor => KnobType::Float,
            Self::RingSize => KnobType::Float,
            Self::OuterBlur => KnobType::Float,
            Self::InnerBlur => KnobType::Float,
            Self::Blades => KnobType::Int,
            Self::Angle => KnobType::Float,
            Self::Curvature => KnobType::Float,
            Self::NoiseSize => KnobType::Float,
            Self::NoiseIntensity => KnobType::Float,
            Self::NoiseSeed => KnobType::Int,
            Self::AspectRatio => KnobType::Float,
            Self::CatseyeEnable => KnobType::Bool,
            Self::CatseyeAmount => KnobType::Float,
            Self::CatseyeInverse => KnobType::Bool,
            Self::CatseyeInverseForeground => KnobType::Bool,
            Self::CatseyeGamma => KnobType::Float,
            Self::CatseyeSoftness => KnobType::Float,
            Self::CatseyeDimensionBased => KnobType::Bool,
            Self::BarndoorsEnable => KnobType::Bool,
            Self::BarndoorsAmount => KnobType::Float,
            Self::BarndoorsInverse => KnobType::Bool,
            Self::BarndoorsInverseForeground => KnobType::Bool,
            Self::BarndoorsGamma => KnobType::Float,
            Self::BarndoorsTop => KnobType::Float,
            Self::BarndoorsBottom => KnobType::Float,
            Self::BarndoorsLeft => KnobType::Float,
            Self::BarndoorsRight => KnobType::Float,
            Self::AstigmatismEnable => KnobType::Bool,
            Self::AstigmatismAmount => KnobType::Float,
            Self::AstigmatismGamma => KnobType::Float,
            Self::AxialAberrationEnable => KnobType::Bool,
            Self::AxialAberrationAmount => KnobType::Float,
            Self::AxialAberrationType => KnobType::Enumeration,
            Self::InverseForegroundFilterShape => KnobType::Bool,
            Self::SizeMultiplier => KnobType::Float,
            Self::FocalPlaneOffset => KnobType::Float,
            Self::UseCustomStripeHeight => KnobType::Bool,
            Self::CustomStripeHeight => KnobType::Int,
        }
    }

    // pub fn from_name(name: &str) -> Result<Self> {

    // }

    pub fn knob_changed(
        &self,
        settings: &opendefocus::datamodel::Settings,
        nuke_settings: &NukeSpecificSettings,
        node: &Op,
    ) -> KnobChanged {
        match &self {
            Self::Mode => KnobChanged::new(!input_connected(node, 2), true),
            Self::RenderResult => KnobChanged::new(nuke_settings.mode != Mode::TwoD, true),

            Self::FocusPlane => KnobChanged::new(
                nuke_settings.mode == Mode::Depth
                    || (nuke_settings.mode == Mode::Camera && !settings.defocus.use_camera_focal),
                true,
            ),
            Self::DeviceName => KnobChanged::new(true, true)
                .with_value_change(ValueChange::text(settings.render.device_name()))
                .build(),
            Self::ShowImage => KnobChanged::new(
                settings.render.result_mode()
                    == opendefocus::datamodel::render::ResultMode::FocalPlaneSetup,
                true,
            ),
            Self::Samples => KnobChanged::new(
                true,
                settings.render.quality() == opendefocus::datamodel::render::Quality::Custom
                    || settings.render.farm_quality()
                        == opendefocus::datamodel::render::Quality::Custom,
            ),
            Self::Math => KnobChanged::new(nuke_settings.mode != Mode::TwoD, true),
            Self::Size => KnobChanged::new(
                nuke_settings.mode == Mode::TwoD
                    || nuke_settings.mode == Mode::Depth && !settings.defocus.use_direct_math,
                true,
            ),
            Self::MaxSize => KnobChanged::new(
                nuke_settings.mode != Mode::TwoD,
                settings.defocus.circle_of_confusion.camera_data.is_none(),
            ),
            Self::CameraMaxSize => KnobChanged::new(true, nuke_settings.mode == Mode::Camera),
            Self::WorldUnit => KnobChanged::new(nuke_settings.mode == Mode::Camera, true),
            Self::FocusPointUtility => KnobChanged::new(nuke_settings.mode != Mode::TwoD, true),
            Self::UseCameraFocal => KnobChanged::new(nuke_settings.mode == Mode::Camera, true),
            Self::ProtectRange => KnobChanged::new(nuke_settings.mode != Mode::TwoD, true),
            Self::FocalPlaneOffset => KnobChanged::new(nuke_settings.mode != Mode::TwoD, true),
            Self::CatseyeAmount => KnobChanged::new(settings.non_uniform.catseye.enable, true),
            Self::CatseyeGamma => KnobChanged::new(settings.non_uniform.catseye.enable, true),
            Self::CatseyeDimensionBased => {
                KnobChanged::new(settings.non_uniform.catseye.enable, true)
            }
            Self::CatseyeInverse => KnobChanged::new(settings.non_uniform.catseye.enable, true),
            Self::CatseyeInverseForeground => {
                KnobChanged::new(settings.non_uniform.catseye.enable, true)
            }
            Self::CatseyeSoftness => KnobChanged::new(settings.non_uniform.catseye.enable, true),
            Self::BarndoorsInverse => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::BarndoorsInverseForeground => {
                KnobChanged::new(settings.non_uniform.barndoors.enable, true)
            }
            Self::BarndoorsAmount => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::BarndoorsGamma => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::BarndoorsTop => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::BarndoorsBottom => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::BarndoorsLeft => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::BarndoorsRight => KnobChanged::new(settings.non_uniform.barndoors.enable, true),
            Self::AstigmatismAmount => {
                KnobChanged::new(settings.non_uniform.astigmatism.enable, true)
            }
            Self::AstigmatismGamma => {
                KnobChanged::new(settings.non_uniform.astigmatism.enable, true)
            }
            Self::AxialAberrationAmount => {
                KnobChanged::new(settings.non_uniform.axial_aberration.enable, true)
            }
            Self::AxialAberrationType => {
                KnobChanged::new(settings.non_uniform.axial_aberration.enable, true)
            }

            Self::PreviewFilter => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::FilterResolution => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::RingColor => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::InnerColor => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::RingSize => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::InnerBlur => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::OuterBlur => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::Blades => KnobChanged::new(nuke_settings.filter_type == FilterType::Blade, true),
            Self::Angle => KnobChanged::new(nuke_settings.filter_type == FilterType::Blade, true),
            Self::Curvature => {
                KnobChanged::new(nuke_settings.filter_type == FilterType::Blade, true)
            }
            Self::NoiseSize => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::NoiseIntensity => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::NoiseSeed => KnobChanged::new(
                nuke_settings.filter_type == FilterType::Blade
                    || nuke_settings.filter_type == FilterType::Disc,
                true,
            ),
            Self::AspectRatio => {
                KnobChanged::new(nuke_settings.filter_type != FilterType::Image, true)
            }
            Self::CustomStripeHeight => {
                KnobChanged::new(nuke_settings.use_custom_stripe_height, true)
            }
            _ => KnobChanged::new(true, true),
        }
    }

    pub fn to_snake_case(&self) -> String {
        ccase!(snake, self.to_string())
    }

    pub fn parameters(&self) -> KnobParameters {
        match &self {
            Self::Channels => KnobParameters::create(&self.to_snake_case())
            .with_label("channels")
            .with_tooltip("Channels that are being processed.")
            .with_input(0)
            .build(),

            Self::DeviceName => KnobParameters::create(&self.to_snake_case())
            .with_label("CPU")
            .with_tooltip(
                "Device that is used for rendering. It will also specify its associated graphics API that's being used (Metal on macOS, Vulkan on Windows/Linux.)",
            )
            .build(),
            Self::UseGpuIfAvailable => KnobParameters::create(&self.to_snake_case()).with_label("use GPU if available").with_tooltip("Prefer GPU or use CPU").with_flag(FlagMask::STARTLINE).build(),
            Self::Mode => KnobParameters::create(&self.to_snake_case())
                .with_label("mode")
                .with_enum_label("2d")
                .with_enum_label("depth")
                .with_enum_label("camera")
                .with_tooltip(
                    "Select the render mode that will be used for rendering the defocus.<br><br><b>2D</b>: Render a defocus that has the size specified by the size slider. It will be the same size over the entire image.<br><b>Depth</b>: Use the depth map for rendering the defocus. This will be the depth map that is specified in the depth channel knob.<br><b>Camera</b>: This will use both the depth map and the camera input to render the defocus. Most defocus settings will be specified by the camera and are not modifiable on this node. This is optically accurate according to the camera. This includes the <i>f-stop</i>, <i>focal length</i>, <i>filmback</i> width and height, and <i>focal distance</i>.",
                )
                .build(),

            Self::RenderResult => KnobParameters::create(&self.to_snake_case())
                .with_label("render")
                .with_enum_label("result")
                .with_enum_label("focal-plane setup")
                .with_tooltip(
                    "Rendering mode to use. By default, it renders the result, but there are also utility render modes available.<br><br><b>Focal plane preview</b>: Render an overlay that shows the depth channel focus areas. Red means in foreground, green means in focus, blue means in background.",
                )
                .build(),

            Self::ShowImage => KnobParameters::create(&self.to_snake_case())
                .with_label("show image")
                .with_tooltip(opendefocus::datamodel::defocus::Settings::get_field_docs("show_image").unwrap())
                .build(),

            Self::DepthChannel => KnobParameters::create(&self.to_snake_case())
                .with_label("depth channel")
                .with_tooltip(
                    "Specify which channel to use for fetching the depth channel information.",
                )
                .with_input(0)
                .with_count(1)
                .build(),

            Self::Math => KnobParameters::create(&self.to_snake_case())
                .with_label("math")
                .with_enum_label("direct")
                .with_enum_label("1/z")
                .with_enum_label("real")
                .with_tooltip(
                    "Math that is used in the depth channel. This depends on the render engine that rendered the depth channel.<br><br><b>1/z</b>: Compatible with depth maps rendered by <i>Nuke</i> and <i>RenderMan</i>. This depth map is a value between 0 and 1, and can be converted into a real depth map by applying the expression 1/z.<br><b>Real</b>: Value in depth map is the real-world unit from the camera. This can be in cm, meters, inches, etc.<br><b>Direct</b>: The depth map provides the defocus size. A pixel with a value of 20 would mean that the defocus will be rendered with a size of 40px.",
                )
                .build(),

            Self::WorldUnit => KnobParameters::create(&self.to_snake_case())
                .with_label("world unit")
                .with_enum_label("mm")
                .with_enum_label("cm")
                .with_enum_label("dm")
                .with_enum_label("m")
                .with_enum_label("in")
                .with_enum_label("ft")
                .with_tooltip(opendefocus::datamodel::circle_of_confusion::CameraData::get_field_docs("world_unit").unwrap())
                .without_flag(FlagMask::STARTLINE)
                .build(),

            Self::Quality => KnobParameters::create(&self.to_snake_case())
                .with_label("quality")
                .with_enum_label("low")
                .with_enum_label("medium")
                .with_enum_label("high")
                .with_enum_label("custom")
                .with_tooltip(opendefocus::datamodel::render::Settings::get_field_docs("quality").unwrap())
                .build(),

            Self::FarmQuality => KnobParameters::create(&self.to_snake_case())
                .with_label("farm quality")
                .with_enum_label("low")
                .with_enum_label("medium")
                .with_enum_label("high")
                .with_enum_label("custom")
                .with_tooltip(
                    opendefocus::datamodel::render::Settings::get_field_docs("farm_quality").unwrap(),
                )
                .without_flag(FlagMask::STARTLINE)
                .build(),

            Self::Samples => KnobParameters::create(&self.to_snake_case())
                .with_label("samples")
                .with_tooltip(opendefocus::datamodel::render::Settings::get_field_docs("samples").unwrap())
                .with_flag(FlagMask::STARTLINE)
                .build(),

            Self::FocusPlane => KnobParameters::create(&self.to_snake_case())
                .with_label("focal plane")
                .with_tooltip(opendefocus::datamodel::circle_of_confusion::Settings::get_field_docs("focal_plane").unwrap())
                .build(),

            Self::UseCameraFocal => KnobParameters::create(&self.to_snake_case())
                .with_label("override by camera")
                .with_tooltip(opendefocus::datamodel::defocus::Settings::get_field_docs("use_camera_focal").unwrap())
                .build(),

            Self::FocusPointUtility => KnobParameters::create(&self.to_snake_case())
                .with_label("focus point")
                .with_tooltip(
                    "Utility tool to sample the focus point from the depth channel and set the value in the <i>focus plane</i> knob.<br><br><b>Note:</b> Don't animate this value, as this is only a utility to sample. If you want animation on focus, set a keyframe on the focal plane instead.",
                )
                .with_flag(FlagMask::NO_ANIMATION)
                .build(),

            Self::ProtectRange => KnobParameters::create(&self.to_snake_case())
                .with_label("protect")
                .with_tooltip(opendefocus::datamodel::circle_of_confusion::Settings::get_field_docs("protect").unwrap())
                .build(),

            Self::Size => KnobParameters::create(&self.to_snake_case())
                .with_label("size")
                .with_tooltip(opendefocus::datamodel::circle_of_confusion::Settings::get_field_docs("protect").unwrap())
                .with_flag(FlagMask::LOG_SLIDER)
                .with_range(0.0, 100.0)
                .build(),

            Self::MaxSize => KnobParameters::create(&self.to_snake_case())
                .with_label("maximum")
                .with_tooltip(opendefocus::datamodel::circle_of_confusion::Settings::get_field_docs("max_size").unwrap())
                .with_flag(FlagMask::LOG_SLIDER)
                .with_range(0.0, 100.0)
                .build(),
            Self::CameraMaxSize => KnobParameters::create(&self.to_snake_case())
                .with_label("maximum")
                .with_tooltip(opendefocus::datamodel::circle_of_confusion::Settings::get_field_docs("max_size").unwrap())
                .with_flag(FlagMask::LOG_SLIDER)
                .with_range(0.0, 100.0)
                .build(),
            Self::GammaCorrection => KnobParameters::create(&self.to_snake_case())
                .with_label("gamma correction")
                .with_tooltip(opendefocus::datamodel::defocus::Settings::get_field_docs("gamma_correction").unwrap())
                .with_range(0.2, 5.0)
                .with_flag(FlagMask::FORCE_RANGE | FlagMask::LOG_SLIDER)
                .build(),
            Self::PreviewFilter => KnobParameters::create(&self.to_snake_case())
                .with_label("preview filter")
                .with_tooltip(opendefocus::datamodel::render::Filter::get_field_docs("preview").unwrap())
                .build(),
            Self::FilterType => KnobParameters::create(&self.to_snake_case())
                .with_label("filter type")
                .with_enum_label("simple")
                .with_enum_label("disc")
                .with_enum_label("blade")
                .with_enum_label("image")
                .with_tooltip(
                    "Select the filter shape type.<br><br><b>Circle</b>: Creates a perfect circular shape.<br><b>Bladed</b>: Creates a bladed shape, specified by the amount of blades.<br><b>Image</b>: Use your own bokeh kernel provided by the filter input. This will be automatically selected when the filter input is used.<br><br>When image is selected, none of the bokeh knobs will be available as everything is controlled by the image.",
                )
                .build(),
            Self::FilterResolution => KnobParameters::create(&self.to_snake_case())
                .with_label("resolution")
                .with_tooltip(opendefocus::datamodel::render::Filter::get_field_docs("resolution").unwrap())
                .build(),
            Self::RingColor => KnobParameters::create(&self.to_snake_case())
                .with_label("ring color")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("ring_color").unwrap())
                .with_range(0.0, 1.0)
                .with_flag(FlagMask::FORCE_RANGE)
                .build(),
            Self::InnerColor => KnobParameters::create(&self.to_snake_case())
                .with_label("inner color")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("inner_color").unwrap())
                .with_range(0.001, 1.0)
                .with_flag(FlagMask::FORCE_RANGE)
                .build(),
            Self::RingSize => KnobParameters::create(&self.to_snake_case())
                .with_label("ring size")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("ring_size").unwrap())
                .build(),
            Self::OuterBlur => KnobParameters::create(&self.to_snake_case())
                .with_label("outer blur")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("outer_blur").unwrap())
                .build(),

            Self::InnerBlur => KnobParameters::create(&self.to_snake_case())
                .with_label("inner blur")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("inner_blur").unwrap())
                .build(),

            Self::Blades => KnobParameters::create(&self.to_snake_case())
                .with_label("blades")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("blades").unwrap())
                .with_range(3.0, 16.0)
                .with_flag(FlagMask::FORCE_RANGE)
                .build(),

            Self::Angle => KnobParameters::create(&self.to_snake_case())
                .with_label("angle")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("angle").unwrap())
                .with_range(-180.0, 180.0)
                .build(),

            Self::Curvature => KnobParameters::create(&self.to_snake_case())
                .with_label("curvature")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("curvature").unwrap())
                .build(),

            Self::NoiseSize => KnobParameters::create(&self.to_snake_case())
                .with_label("noise size")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Noise::get_field_docs("size").unwrap())
                .build(),
            Self::NoiseIntensity => KnobParameters::create(&self.to_snake_case())
                .with_label("noise intensity")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Noise::get_field_docs("intensity").unwrap())
                .build(),
            Self::NoiseSeed => KnobParameters::create(&self.to_snake_case())
                .with_label("noise seed")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Noise::get_field_docs("seed").unwrap())
                .build(),
            Self::AspectRatio => KnobParameters::create(&self.to_snake_case())
                .with_label("aspect ratio")
                .with_tooltip(opendefocus::datamodel::bokeh_creator::Settings::get_field_docs("aspect_ratio").unwrap())
                .with_range(0.0, 2.0)
                .build(),
            Self::CatseyeEnable => KnobParameters::create(&self.to_snake_case())
                .with_label("enable catseye")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("enable").unwrap())
                .build(),
            Self::CatseyeAmount => KnobParameters::create(&self.to_snake_case())
                .with_label("amount")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("amount").unwrap())
                .build(),

            Self::CatseyeInverse => KnobParameters::create(&self.to_snake_case())
                .with_label("inverse")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("inverse").unwrap())
                .build(),

            Self::CatseyeInverseForeground => KnobParameters::create(&self.to_snake_case())
                .with_label("inverse foreground")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("inverse_foreground").unwrap())
                .build(),

            Self::CatseyeGamma => KnobParameters::create(&self.to_snake_case())
                .with_label("gamma")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("gamma").unwrap())
                .with_range(0.2, 4.0)
                .with_flag(FlagMask::LOG_SLIDER)
                .build(),

            Self::CatseyeSoftness => KnobParameters::create(&self.to_snake_case())
                .with_label("softness")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("softness").unwrap())
                .with_range(0.01, 1.0)
                .with_flag(FlagMask::FORCE_RANGE)
                .build(),

            Self::CatseyeDimensionBased => KnobParameters::create(&self.to_snake_case())
                .with_label("dimension based")
                .with_tooltip(opendefocus::datamodel::non_uniform::Catseye::get_field_docs("relative_to_screen").unwrap())
                .build(),

            Self::BarndoorsEnable => KnobParameters::create(&self.to_snake_case())
                .with_label("enable barndoors")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("enable").unwrap())
                .build(),
            Self::BarndoorsAmount => KnobParameters::create(&self.to_snake_case())
                .with_label("amount")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("amount").unwrap())
                .build(),

            Self::BarndoorsInverse => KnobParameters::create(&self.to_snake_case())
                .with_label("inverse")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("inverse").unwrap())
                .build(),

            Self::BarndoorsInverseForeground => KnobParameters::create(&self.to_snake_case())
                .with_label("inverse foreground")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("inverse_foreground").unwrap())
                .build(),

            Self::BarndoorsGamma => KnobParameters::create(&self.to_snake_case())
                .with_label("gamma")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("gamma").unwrap())
                .with_range(0.2, 4.0)
                .with_flag(FlagMask::LOG_SLIDER)
                .build(),

            Self::BarndoorsTop => KnobParameters::create(&self.to_snake_case())
                .with_label("top")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("top").unwrap())
                .build(),

            Self::BarndoorsBottom => KnobParameters::create(&self.to_snake_case())
                .with_label("bottom")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("bottom").unwrap())
                .build(),

            Self::BarndoorsLeft => KnobParameters::create(&self.to_snake_case())
                .with_label("left")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("left").unwrap())
                .build(),

            Self::BarndoorsRight => KnobParameters::create(&self.to_snake_case())
                .with_label("right")
                .with_tooltip(opendefocus::datamodel::non_uniform::Barndoors::get_field_docs("right").unwrap())
                .build(),

            Self::AstigmatismEnable => KnobParameters::create(&self.to_snake_case())
                .with_label("enable astigmatism")
                .with_tooltip(opendefocus::datamodel::non_uniform::Astigmatism::get_field_docs("enable").unwrap())
                .build(),
            Self::AstigmatismAmount => KnobParameters::create(&self.to_snake_case())
                .with_label("amount")
                .with_tooltip(opendefocus::datamodel::non_uniform::Astigmatism::get_field_docs("amount").unwrap())
                .build(),

            Self::AstigmatismGamma => KnobParameters::create(&self.to_snake_case())
                .with_label("gamma")
                .with_tooltip(opendefocus::datamodel::non_uniform::Astigmatism::get_field_docs("gamma").unwrap())
                .with_range(0.2, 4.0)
                .with_flag(FlagMask::LOG_SLIDER)
                .build(),

            Self::AxialAberrationEnable => KnobParameters::create(&self.to_snake_case())
                .with_label("enable axial aberration")
                .with_tooltip(opendefocus::datamodel::non_uniform::AxialAberration::get_field_docs("enable").unwrap())
                .build(),

            Self::AxialAberrationAmount => KnobParameters::create(&self.to_snake_case())
                .with_label("amount")
                .with_tooltip(opendefocus::datamodel::non_uniform::AxialAberration::get_field_docs("amount").unwrap())
                .with_range(-1.0, 1.0)
                .build(),

            Self::AxialAberrationType => KnobParameters::create(&self.to_snake_case())
                .with_label("type")
                .with_enum_label("red/blue")
                .with_enum_label("blue/yellow")
                .with_enum_label("green/purple")
                .with_tooltip(opendefocus::datamodel::non_uniform::AxialAberration::get_field_docs("color_type").unwrap())
                .without_flag(FlagMask::STARTLINE)
                .build(),

            Self::InverseForegroundFilterShape => KnobParameters::create(&self.to_snake_case())
                .with_label("inverse foreground filter shape")
                .with_tooltip(opendefocus::datamodel::non_uniform::Settings::get_field_docs("inverse_foreground").unwrap())
                .build(),
            Self::SizeMultiplier => KnobParameters::create(&self.to_snake_case())
                .with_label("size multiplier")
                .with_tooltip(opendefocus::datamodel::defocus::Settings::get_field_docs("size_multiplier").unwrap())
                .with_range(0.0, 2.0)
                .build(),
            Self::FocalPlaneOffset => KnobParameters::create(&self.to_snake_case())
                .with_label("focal plane offset")
                .with_tooltip(opendefocus::datamodel::defocus::Settings::get_field_docs("focal_plane_offset").unwrap())
                .with_range(-5.0, 5.0)
                .build(),
            Self::UseCustomStripeHeight => KnobParameters::create(&self.to_snake_case())
                .with_label("use custom stripe height")
                .with_tooltip("Override the stripe height, possible to gain a bit of performance or render heavier stuff.")
                .build(),
            Self::CustomStripeHeight => KnobParameters::create(&self.to_snake_case())
                .with_label("height")
                .with_tooltip("Height in pixels")
                .with_range(0 as f32, 512 as f32)
                .with_flag(FlagMask::FORCE_RANGE)
                .build(),
        }
    }
}

bitflags! {
    /// General flags (must not intersect any class-specific flags):
    /// General knob flags begin at 0x80.
    /// Usually if you're adding a new General flag you just need to look at the last flag and use the next available value.
    /// When adding a general flag, search the file first to make sure the value isn't already in use
    #[derive(Debug, Clone, Copy, PartialEq)]
    #[repr(C)]
    pub struct FlagMask: u64 {
        /// Grey out and lock interface. Prevents copy/paste (see READ_ONLY to allow this).
        const DISABLED                 = 0x0000000000000080;
        /// Disable right click and button animation menu.
        const NO_ANIMATION             = 0x0000000000000100;
        /// Disables calling to_script. No writing to script file or copy/paste.
        const DO_NOT_WRITE             = 0x0000000000000200;
        /// Disables param and viewer widgets. Cannot be made visible again. See HIDDEN for this.
        const INVISIBLE                = 0x0000000000000400;
        /// Allows more complex knobs to resize param panel to fill available space.
        const RESIZABLE                = 0x0000000000000800;
        /// Start a new line in the param panel before knob widget.
        const STARTLINE                = 0x0000000000001000;
        /// Start a new line in the param panel after knob widget.
        const ENDLINE                  = 0x0000000000002000;
        /// Removes knob from Op hash calculation, preventing rerendering on value change.
        const NO_RERENDER              = 0x0000000000004000;
        /// Disables viewer widget handles from drawing.
        const NO_HANDLES               = 0x0000000000008000;
        /// Always calls knob_changed, regardless of whether it has previously returned false.
        const KNOB_CHANGED_ALWAYS      = 0x0000000000010000;
        /// Prevents knob_changed being called on value change. Set if prev knob_changed returned false.
        const NO_KNOB_CHANGED          = 0x0000000000020000;
        /// Disables param panel and viewer widgets. Can be managed dynamically with show/hide.
        const HIDDEN                   = 0x0000000000040000;
        /// Disables laying down of undo/redo points.
        const NO_UNDO                  = 0x0000000000080000;
        /// Forces data to always be written regardless. Deprecated. Override not_default instead.
        const ALWAYS_SAVE              = 0x0000000000100000;
        /// For internal use only.
        const NODE_KNOB                = 0x0000000000200000;
        /// Force viewer widgets to be visible regardless of current node tab.
        const HANDLES_ANYWAY           = 0x0000000000400000;
        /// Presents a blacked out undefined value interface on supporting knobs.
        const INDETERMINATE            = 0x0000000000800000;
        /// Defines whether a color chip can be in the 'unset' state. Defaults to false.
        const COLOURCHIP_HAS_UNSET     = 0x0000000001000000;
        /// Switches param panel widget to be more viewer Toolbar friendly in supported knobs (eg Button).
        const SMALL_UI                 = 0x0000000002000000;
        /// Disables numeric input box widget on supported knobs.
        const NO_NUMERIC_FIELDS        = 0x0000000004000000;
        /// Recursive knob_changed calls are prevented unless overriden using this flag.
        const KNOB_CHANGED_RECURSIVE   = 0x0000000008000000;
        /// As with DISABLED, except value can be copied from and expression linked against.
        const READ_ONLY                = 0x0000000010000000;
        /// Disables curve editor.
        const NO_CURVE_EDITOR          = 0x0000000020000000;
        /// Disables view menu and splitting when in a multiview script.
        const NO_MULTIVIEW             = 0x0000000040000000;
        /// Forces early synchronisation of data allowing usage in pre-op calls such as split_input().
        const EARLY_STORE              = 0x0000000080000000;
        /// Should be set on all knobs which modify geometry or associated transforms.
        const MODIFIES_GEOMETRY        = 0x0000000100000000;
        /// Similar to READ_ONLY & NO_RERENDER together - data changes don't count as a script change.
        const OUTPUT_ONLY              = 0x0000000200000000;
        /// Prevents knob_changed_finished being called on value change. Set if prev call returned false.
        const NO_KNOB_CHANGED_FINISHED = 0x0000000400000000;
        /// Do not use.
        const SET_SIZE_POLICY          = 0x0000000800000000;
        /// Force knob to expand to fill available space. - only for Enum knobs currently
        const EXPAND_TO_WIDTH          = 0x0000001000000000;
        /// Disables viewer widget handles from drawing. Unlike the NO_HANDLES flag, the state of this flag will never change internally within Nuke
        const NEVER_DRAW_HANDLES       = 0x0000002000000000;
        /// Always call knob_changed on a properly cooked Op, even if KNOB_CHANGED_ALWAYS is on
        const KNOB_CHANGED_RIGHTCONTEXT= 0x0000004000000000;
        /// This value of this knob should never be saved to a NodePreset. Can be used, for example, for data knobs.
        const DONT_SAVE_TO_NODEPRESET  = 0x0000008000000000;
        /// DO NOT USE. This value is used by the colorchip knob.
        const RESERVED_COLORCHIP_KNOB  = 0x0000010000000000;
        /// Prevents knobs from being modified from Python/Tcl
        const READ_ONLY_IN_SCRIPTS     = 0x0000020000000000;
        /// Label is always aligned to the top of the Knob
        const ALWAYS_ALIGN_LABEL_TOP   = 0x0000040000000000;
        /// Modifies SLIDER to be a tiny slider underneath lineedit. Should be a numeric knob flag but we've overrun the < 0x80 condition.
        const TINY_SLIDER              = 0x0000080000000000;
        /// Prevents Animation Curve_Knob and Views being shown. Animation is still possible, unless NO_ANIMATION is set of course.
        const HIDE_ANIMATION_AND_VIEWS = 0x0000100000000000;
        /// Prevents Color Panel Dropdown from being available. Popup color panel will stil be available.
        const NO_COLOR_DROPDOWN        = 0x0000200000000000;
        /// Indicate that this knob should only be displayed when using the NodeGraph, since the Timeline uses gpuEngine, which might not support all the same knobs.
        const NODEGRAPH_ONLY           = 0x0000400000000000;
        /// Prevents 'execute' being called on the knob
        const NO_SCRIPT_EXECUTE        = 0x0000800000000000;
        /// Should be set on all knobs which modify timing
        const MODIFIES_TIME            = 0x0001000000000000;
        /// This knob must be drawn in the style of Viewer toolbar knobs
        const TOOLBAR_BUTTON_DRAWSTYLE = 0x0002000000000000;
        /// Used to lock modifications to this knobs flags
        const FLAGS_LOCKED             = 0x0004000000000000;
        /// Skip reading the knob from script, must be set before loading from script Enumeration_Knob only
        const DO_NOT_READ              = 0x0008000000000000;
        /// This knob's value is supplied from an asset
        const FROM_ASSET               = 0x0010000000000000;
        /// DO NOT USE. This value is used by numeric knobs.
        const RESERVED_NUMERIC_KNOB    = 0x0020000000000000;

        /// Enables switchable numeric box & slider to multiple boxes (array knob derived numeric knobs).
        const MAGNITUDE                = 0x0000000000000001;
        /// Enables slider on single numeric knob, or array knob with MAGNITUDE set (numeric knobs).
        const SLIDER                   = 0x0000000000000002;
        /// Switches linear slider to log slider, or cubic depending on range (numeric knobs with SLIDER).
        const LOG_SLIDER               = 0x0000000000000004;
        /// Stores and presents integer value rather than float (numeric knobs).
        const STORE_INTEGER            = 0x0000000000000008;
        /// Forces stored and presented value to be clamped to range set (numeric knobs).
        const FORCE_RANGE              = 0x0000000000000010;
        /// Switches widget for angle UI (single value numeric knobs).
        const ANGLE                    = 0x0000000000000020;
        /// Disables proxyscaling on knobs supporting it (XY_Knob & WH_Knob derivatives).
        const NO_PROXYSCALE            = 0x0000000000000040;
        /// Disables pixel aspect scaling on knobs supporting it (XY_Knob & WH_Knob derivatives).
        const NO_PIXELASPECTSCALE      = 0x0020000000000000;

        // String Knobs
        /// Disables concatenation of minor undo events (string knobs)
        const GRANULAR_UNDO            = 0x0000000000000001;
        /// Badly named. Actually disables relative paths (string knobs).
        const NO_RECURSIVE_PATHS       = 0x0000000000000002;
        /// For strings containing TCL expressions, don't replace with TCL error messages if an error occurs
        const NO_TCL_ERROR             = 0x0000000000000004;

        // Enumeration
        /// Forces menu entries to be written to script. Used by dynamic menus (enumeration knobs).
        const SAVE_MENU                = 0x0000000002000000;
        /// Make Enumeration knobs adjust their width to the size of the largest munu item.
        const EXPAND_TO_CONTENTS       = 0x0000000000000001;
        /// Make Enumeration knobs use exact match when setting a value. If an attempt is made to set an
        const EXACT_MATCH_ONLY         = 0x0000000000000002;
        /// Make Cascading Enumeration knobs not serialise out cascading prefixes
        const STRIP_CASCADE_PREFIX     = 0x0000000000000004;

        // SceneView / Path knob
        /// Knob only allows one item to be selected at a time
        const SINGLE_SELECTION_ONLY     = 0x0000000000000001;
        /// Show Add Layer/Delete Layer buttons
        const SHOW_BUTTONS              = 0x0000000000000002;
        /// Show Scene Graph Dialog button to choose Prim(s) from the Scene
        const SHOW_SCENE_PICK_BUTTON    = 0x0000000000000008;

        // BeginGroup
        /// Stores the open/closed state of group knobs (group knobs).
        const CLOSED                   = 0x0000000000000001;
        /// Make the group into a viewer toolbar. General used via BeginToolbar (group knobs).
        const TOOLBAR_GROUP            = 0x0000000000000002;
        /// Defines which side of viewer toolbar appears on. Pick one at toolbar construction time (toolbar).
        const TOOLBAR_LEFT             = 0x0000000000000000;
        /// Defines which side of viewer toolbar appears on. Pick one at toolbar construction time (toolbar).
        const TOOLBAR_TOP              = 0x0000000000000010;
        /// Defines which side of viewer toolbar appears on. Pick one at toolbar construction time (toolbar).
        const TOOLBAR_BOTTOM           = 0x0000000000000020;
        /// Defines which side of viewer toolbar appears on. Pick one at toolbar construction time (toolbar).
        const TOOLBAR_RIGHT            = 0x0000000000000030;
        /// A mask for the position part of the flags
        const TOOLBAR_POSITION         = 0x0000000000000030;

        // ChannelSet/Channel:
        /// Disable individual channel checkbox widgets (channel/channelset knobs).
        const NO_CHECKMARKS            = 0x0000000000000001;
        /// Disable 4th channel pulldown widget (channel/channelset knobs).
        const NO_ALPHA_PULLDOWN        = 0x0000000000000002;
        /// channel/channelset knobs will always consider every channel in a layer to be enabled.
        const FULL_LAYER_ENABLED       = 0x0000000000000004;
        /// Disables the layer selection dropdown. Forces the layer to always be RGBA.
        const FORCE_RGBA_LAYER         = 0x0000000000000008;

        // Format knob
        /// Sets default knob value from script proxy format rather than full res (format knob).
        const PROXY_DEFAULT            = 0x0000000000000001;

        // ColorChip knob
        /// The ColorChip_knob discards alpha values by default. Set this flag to make it keep them, instead.
        const COLORCHIP_PRESERVE_ALPHA  = 0x0000010000000000;

        // Colorspace Knob
        /// Allows the knob to display Nuke's native colorspaces if Color Management is set to Nuke.
        const ALLOW_NUKE_COLORSPACES   = 0x0000000000000001;

        // Text knob
        /// Enable word wrapping-wrapping for the Text_knob
        const WORD_WRAP                 = 0x0000000000000001;

        // Eyedropper knob
        /// Tick checkbox in the ticker
        const CHECKED                   = 0x0000000000000001;
    }
}

impl Default for FlagMask {
    fn default() -> Self {
        Self::empty()
    }
}
