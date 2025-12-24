#![warn(unused_extern_crates)]
use std::time::Duration;
mod channels;
mod render;
use anyhow::Error;
use anyhow::Result;
use channels::{Channel, ChannelSetInit};
use chrono::Datelike;
use chrono::Local;
use cxx::SharedPtr;
use opendefocus::datamodel::bokeh_creator;
use opendefocus::datamodel::circle_of_confusion;
use opendefocus::datamodel::circle_of_confusion::WorldUnit;
use opendefocus::{
    OpenDefocusRenderer,
    abort::set_aborted,
    datamodel::{
        Settings, UVector2, Vector2,
        circle_of_confusion::{CameraData, Filmback},
        defocus::DefocusMode,
        render::{FilterMode, Quality, ResultMode},
    },
};
use tokio::{runtime::Runtime, time::sleep};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use strum::IntoEnumIterator;
mod consts;
mod knobs;
use crate::ffi::Math;
use crate::ffi::Mode;
use crate::ffi::create_format;
use crate::ffi::get_channelset;
use crate::ffi::input_connected;

use crate::{
    consts::{FlagMask, KnobDefinition},
    ffi::{
        Format, Knob_Callback, KnobChanged, KnobParameters, NukeCameraData, NukeSpecificSettings,
        Op, ValueChange, create_divider_knob, create_newline_knob, create_tab_knob,
        create_text_knob, sample_channel, set_knobchanged,
    },
    knobs::{create_knob, create_knob_with_value},
    render::render_convolution,
};

#[cxx::bridge]
mod ffi {
    struct NukeCameraData {
        focal_length: f32,
        f_stop: f32,
        focal_point: f32,
        filmback: [f32; 2],
        near: f32,
        far: f32,
    }

    #[derive(Default)]
    enum ValueType {
        #[default]
        Float,
        Int,
        Bool,
        Text,
    }

    #[derive(Default, Clone)]
    struct ValueChange {
        // we cant have optional types or traits yet in cxx so this is a bit cumbersome but fine for now
        pub value_type: ValueType,
        pub float_value: f32,
        pub int_value: i32,
        pub bool_value: bool,
        pub text_value: String,
    }

    #[derive(Default, Clone)]
    struct KnobChanged {
        pub enabled: bool,
        pub visible: bool,
        pub set_value: bool,
        pub value_change: ValueChange,
    }

    #[derive(Clone, Debug)]
    enum KnobType {
        Float,
        Int,
        Bool,
        Enumeration,
        XY,
        InputChannelSet,
        InputOnlyChannel,
        Text,
        NamedText,
        Divider,
        Tab,
        Newline,
    }

    #[derive(Clone, Debug)]
    /// Common parameters for knobs
    struct KnobParameters {
        label: String,
        enum_labels: Vec<String>,
        /// Unique name of knob, used for lookup
        name: String,
        /// Hover tooltip in user interface
        tooltip: String,
        /// Values for enumeration
        with_flags: u64,
        without_flags: u64,
        input: u32,
        count: u32,
        range: [f32; 2],
    }

    #[derive(Clone)]
    #[repr(u32)]
    enum Math {
        Direct,
        OneDividedByZ,
        Real,
    }
    #[derive(Clone)]
    #[repr(u32)]
    enum FilterType {
        Simple,
        Disc,
        Blade,
        Image,
    }

    #[derive(Clone)]
    #[repr(u32)]
    enum Mode {
        TwoD,
        Depth,
        Camera,
    }

    #[derive(Clone)]
    struct NukeSpecificSettings {
        channels: SharedPtr<ChannelSet>,
        filter_format: SharedPtr<Format>,
        depth_channel: Channel,
        focal_point: [f32; 2],
        custom_stripe_height: u32,
        use_custom_stripe_height: bool,
        world_unit: i32,
        math: Math,
        mode: Mode,
        filter_type: FilterType,
    }

    #[namespace = "DD::Image"]
    unsafe extern "C++" {
        include!("DDImage/Op.h");
        type Op;
        /// Return aborted state of Node tree
        fn aborted(&self) -> bool;
        // fn validate(&self, for_real: bool);
    }

    #[namespace = "DD::Image"]
    unsafe extern "C++" {
        include!("DDImage/Knobs.h");
        type Knob;
        type Knob_Callback;
    }

    #[namespace = "DD::Image"]
    unsafe extern "C++" {
        include!("DDImage/Format.h");
        type Format;
        fn width(&self) -> i32;
        #[rust_name = "set_width"]
        fn width(self: Pin<&mut Format>, width: i32);
        fn height(&self) -> i32;
        #[rust_name = "set_height"]
        fn height(self: Pin<&mut Format>, height: i32);
    }


    // unsafe extern "C++" {
    //     include!("opendefocus-nuke/include/opendefocus.hpp");
    //     type OpenDefocus;

    // }


    #[namespace = "DD::Image"]
    unsafe extern "C++" {
        include!("DDImage/ChannelSet.h");
        type ChannelSet;
        type Channel = crate::channels::Channel;
        type ChannelSetInit = crate::channels::ChannelSetInit;

    }

    extern "Rust" {

        fn log_info(msg: String);
        fn log_warning(msg: String);
        fn log_error(msg: String);
        #[Self = "NukeSpecificSettings"]
        fn create() -> NukeSpecificSettings;

        #[Self = "NukeCameraData"]
        fn create(
            focal_length: f32,
            f_stop: f32,
            focal_point: f32,
            filmback: [f32; 2],
            near: f32,
            far: f32,
        ) -> NukeCameraData;

        type OpenDefocusNukeInstance;
        #[Self = "OpenDefocusNukeInstance"]
        fn create(nuke_settings: NukeSpecificSettings, gui: bool) -> Result<Box<OpenDefocusNukeInstance>>;
        fn create_knobs(&mut self, callback: &Knob_Callback);
        fn set_proxy_scale(&mut self, value: f32);
        fn set_camera_data(&mut self, value: NukeCameraData);
        fn calculate_filter_box(&self) -> [u32; 4];
        fn stripe_height(&self) -> u32;
        fn is_2d(&self) -> bool;
        fn store_format(&mut self, format: SharedPtr<Format>);
        fn fetch_filter(&self) -> bool;
        fn render_filter_only(&self) -> bool;
        fn get_padding(&self) -> u32;
        fn nuke_settings(&self) -> NukeSpecificSettings;
        fn validate(
            &mut self,
            node: &Op,
            resolution: [i32; 2],
            center: [f32; 2],
            pixel_aspect: f32,
        ) -> Result<()>;
        fn set_aborted(&self, value: bool);
        fn use_stripes(&self) -> bool;
        fn fetch_image(&self) -> bool;
        fn fetch_depth(&self) -> bool;
        fn render(
            &self,
            node: &'static Op,
            image: &'static mut [f32],
            depth: &'static [f32],
            channels: usize,
            full_region: [i32; 4],
            render_region: [i32; 4],
            filter: &'static [f32],
            filter_channels: usize,
            filter_region: [i32; 2],
        ) -> Result<()>;
        fn knob_changed(&mut self, node: &Op, knob_name: String) -> Result<bool>;
    }

    unsafe extern "C++" {
        include!("opendefocus-nuke/include/opendefocus.hpp");

        unsafe fn create_float_knob(
            callback: &Knob_Callback,
            value: *mut f32,
            parameters: KnobParameters,
        );

        unsafe fn create_bool_knob(
            callback: &Knob_Callback,
            value: *mut bool,
            parameters: KnobParameters,
        );

        unsafe fn create_int_knob(
            callback: &Knob_Callback,
            value: *mut i32,
            parameters: KnobParameters,
        );

        unsafe fn create_enumeration_knob(
            callback: &Knob_Callback,
            value: *mut i32,
            parameters: KnobParameters,
        );

        unsafe fn create_xy_knob(
            callback: &Knob_Callback,
            value: &mut [f32],
            parameters: KnobParameters,
        );

        unsafe fn create_inputchannelset_knob(
            callback: &Knob_Callback,
            value: *mut UniquePtr<ChannelSet>,
            parameters: KnobParameters,
        );
        unsafe fn create_inputonlychannel_knob(
            callback: &Knob_Callback,
            value: *mut Channel,
            parameters: KnobParameters,
        );

        fn create_text_knob(callback: &Knob_Callback, parameters: KnobParameters);
        fn create_divider_knob(callback: &Knob_Callback);
        fn create_tab_knob(callback: &Knob_Callback, parameters: KnobParameters);
        fn create_newline_knob(callback: &Knob_Callback, parameters: KnobParameters);
        fn sample_channel(node: &Op, channel: Channel, coordinates: [f32; 2]) -> f32;
        fn set_knobchanged(node: &Op, name: String, knob_changed: KnobChanged);
        fn input_connected(node: &Op, input: u32) -> bool;
        fn create_format() -> SharedPtr<Format>;
        fn get_channelset(channels: ChannelSetInit) -> SharedPtr<ChannelSet>;

    }
}

unsafe impl Sync for ffi::Op {}

const FILTER_INPUT: u32 = 1;
const CAMERA_INPUT: u32 = 2;

impl NukeSpecificSettings {
    pub fn create() -> Self {
        Self {
            depth_channel: Channel::ChanZ,
            channels: get_channelset(ChannelSetInit::RGBA),
            filter_format: create_format(),
            focal_point: [200.0, 200.0],
            world_unit: WorldUnit::default() as i32,
            custom_stripe_height: 64,
            use_custom_stripe_height: false,
            math: Math::OneDividedByZ,
            filter_type: ffi::FilterType::Disc,
            mode: Mode::TwoD,
        }
    }
}

impl NukeCameraData {
    pub fn create(
        focal_length: f32,
        f_stop: f32,
        focal_point: f32,
        filmback: [f32; 2],
        near: f32,
        far: f32,
    ) -> Self {
        Self {
            focal_length,
            f_stop,
            focal_point,
            filmback,
            near,
            far,
        }
    }
}

pub struct OpenDefocusNukeInstance {
    settings: Settings,
    nuke_settings: NukeSpecificSettings,
    renderer: OpenDefocusRenderer,
    runtime: Runtime,
    post_initialized: bool,
}
impl OpenDefocusNukeInstance {
    pub fn create(nuke_settings: NukeSpecificSettings, gui: bool) -> Result<Box<Self>> {
        init_log()?;
        let mut settings = Settings::default();
        settings.render.gui = gui;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .build()?;
        let renderer = runtime.block_on(OpenDefocusRenderer::new(true, &mut settings))?;
        Ok(Box::new(Self {
            settings,
            renderer: renderer,
            nuke_settings,
            runtime,
            post_initialized: false,
        }))
    }

    pub fn validate(
        &mut self,
        node: &Op,
        resolution: [i32; 2],
        center: [f32; 2],
        pixel_aspect: f32,
    ) -> Result<()> {
        // TODO clean a bit, this is quick and dirty
        if !self.post_initialized
            && self.renderer.is_gpu()
            && !self.settings.render.use_gpu_if_available
        {
            self.renderer = self.runtime.block_on(OpenDefocusRenderer::new(
                self.settings.render.use_gpu_if_available,
                &mut self.settings,
            ))?;
            self.post_initialized = true;
        }

        if !input_connected(node, CAMERA_INPUT) {
            self.settings.defocus.circle_of_confusion.camera_data = None;
            if self.nuke_settings.mode == Mode::Camera {
                return Err(Error::msg(
                    "Camera as mode selected but no camera connected.",
                ));
            }
        }

        self.settings.render.resolution = UVector2 {
            x: resolution[0] as u32,
            y: resolution[1] as u32,
        };
        self.settings.render.center = Vector2 {
            x: center[0],
            y: center[1],
        };
        self.settings.defocus.circle_of_confusion.pixel_aspect = pixel_aspect;
        self.settings
            .render
            .filter
            .set_mode(match self.nuke_settings.filter_type {
                ffi::FilterType::Simple => FilterMode::Simple,
                ffi::FilterType::Image => FilterMode::Image,
                _ => FilterMode::BokehCreator,
            });
        if self.settings.render.filter.mode() == FilterMode::BokehCreator {
            self.settings
                .bokeh
                .set_filter_type(match self.nuke_settings.filter_type {
                    ffi::FilterType::Disc => bokeh_creator::FilterType::Disc,
                    _ => bokeh_creator::FilterType::Blade,
                });
        }

        match self.nuke_settings.math {
            ffi::Math::Direct => self.settings.defocus.use_direct_math = true,
            ffi::Math::OneDividedByZ => self
                .settings
                .defocus
                .circle_of_confusion
                .set_math(circle_of_confusion::Math::OneDividedByZ),
            ffi::Math::Real => self
                .settings
                .defocus
                .circle_of_confusion
                .set_math(circle_of_confusion::Math::Real),
            _ => return Err(Error::msg("Invalid math selected.")),
        }

        match self.nuke_settings.mode {
            ffi::Mode::TwoD => self.settings.defocus.set_defocus_mode(DefocusMode::Twod),
            _ => self.settings.defocus.set_defocus_mode(DefocusMode::Depth),
        }

        if self.settings.render.filter.mode() == FilterMode::Image {
            if !input_connected(node, FILTER_INPUT) {
                return Err(Error::msg(
                    "Filter input mode selected but no filter connected.",
                ));
            }
        }

        Ok(())
    }

    pub fn set_aborted(&self, value: bool) {
        set_aborted(value);
    }

    pub fn stripe_height(&self) -> u32 {
        if self.nuke_settings.use_custom_stripe_height {
            return self.nuke_settings.custom_stripe_height.clamp(1, 512);
        }
        if self.settings.render.result_mode() == ResultMode::FocalPlaneSetup {
            return 32;
        }
        if !self.renderer.is_gpu() {
            return 64;
        };
        match self.settings.render.get_quality() {
            Quality::Low => 256,
            Quality::Medium => 128,
            _ => 64,
        }
    }

    pub fn store_format(&mut self, format: SharedPtr<Format>) {
        self.nuke_settings.filter_format = format;
    }

    pub fn fetch_image(&self) -> bool {
        if self.settings.render.result_mode() == ResultMode::FocalPlaneSetup
            && !self.settings.defocus.show_image
            && self.settings.defocus.defocus_mode() != DefocusMode::Twod
        {
            return false;
        }
        if self.render_filter_only() {
            return false;
        }
        true
    }

    pub fn fetch_depth(&self) -> bool {
        if self.render_filter_only() {
            return false;
        }
        if self.settings.defocus.defocus_mode() == DefocusMode::Twod {
            return false;
        }
        true
    }

    pub fn render_filter_only(&self) -> bool {
        self.settings.render.filter.preview
            && (self.settings.render.filter.mode() == FilterMode::BokehCreator)
    }

    pub fn get_padding(&self) -> u32 {
        if self.render_filter_only() {
            return 0;
        }
        if self.settings.render.result_mode() == ResultMode::FocalPlaneSetup {
            return 0;
        }
        self.settings.defocus.get_padding()
    }

    pub fn is_2d(&self) -> bool {
        self.settings.defocus.defocus_mode() == DefocusMode::Twod
    }

    pub fn nuke_settings(&self) -> NukeSpecificSettings {
        self.nuke_settings.clone()
    }

    pub fn fetch_filter(&self) -> bool {
        self.settings.render.filter.mode() == FilterMode::Image
    }

    pub fn use_stripes(&self) -> bool {
        !self.render_filter_only()
    }

    /// Renderer that is called synchronously but spawns tasks for rendering
    pub fn render(
        &self,
        node: &'static Op,
        image: &'static mut [f32],
        depth: &'static [f32],
        channels: usize,
        full_region: [i32; 4],
        render_region: [i32; 4],
        filter: &'static [f32],
        filter_channels: usize,
        filter_region: [i32; 2],
    ) -> Result<()> {
        let aborted_checker = self.runtime.spawn(async move {
            loop {
                if node.aborted() {
                    set_aborted(true);
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        });
        let renderer = self.runtime.spawn(render_convolution(
            self.settings.clone(),
            self.renderer.clone(),
            image,
            self.fetch_depth(),
            depth,
            channels,
            full_region,
            render_region,
            filter,
            filter_channels,
            filter_region,
        ));
        self.runtime.block_on(renderer)??;
        aborted_checker.abort();
        Ok(())
    }

    pub fn set_camera_data(&mut self, value: NukeCameraData) {
        self.settings.defocus.circle_of_confusion.camera_data = Some(CameraData {
            f_stop: value.f_stop,
            focal_length: value.focal_length,
            filmback: Filmback {
                width: value.filmback[0],
                height: value.filmback[1],
            },
            far_field: value.far,
            near_field: value.near,
            world_unit: self.nuke_settings.world_unit,
            ..Default::default()
        });
        if self.settings.defocus.use_camera_focal {
            self.settings.defocus.circle_of_confusion.focal_plane = value.focal_point;
        }
    }

    pub fn create_knobs(&mut self, callback: &Knob_Callback) {
        self.create_defocus_knobs(callback);
        self.create_filter_knobs(callback);
        self.create_nonuniform_knobs(callback);
        self.create_advanced_knobs(callback);
    }

    pub fn create_advanced_knobs(&mut self, callback: &Knob_Callback) {
        create_tab_knob(
            callback,
            KnobParameters::create("").with_label("Advanced").build(),
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::SizeMultiplier,
            &mut self.settings.defocus.size_multiplier,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::FocalPlaneOffset,
            &mut self.settings.defocus.focal_plane_offset,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::UseCustomStripeHeight,
            &mut self.nuke_settings.use_custom_stripe_height,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CustomStripeHeight,
            &mut self.nuke_settings.custom_stripe_height,
        );
    }

    pub fn create_nonuniform_knobs(&mut self, callback: &Knob_Callback) {
        create_tab_knob(
            callback,
            KnobParameters::create("").with_label("Non-uniform").build(),
        );

        create_divider_knob(callback);

        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeEnable,
            &mut self.settings.non_uniform.catseye.enable,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeAmount,
            &mut self.settings.non_uniform.catseye.amount,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeInverse,
            &mut self.settings.non_uniform.catseye.inverse,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeInverseForeground,
            &mut self.settings.non_uniform.catseye.inverse_foreground,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeGamma,
            &mut self.settings.non_uniform.catseye.gamma,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeSoftness,
            &mut self.settings.non_uniform.catseye.softness,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CatseyeDimensionBased,
            &mut self.settings.non_uniform.catseye.relative_to_screen,
        );

        create_divider_knob(callback);

        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsEnable,
            &mut self.settings.non_uniform.barndoors.enable,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsAmount,
            &mut self.settings.non_uniform.barndoors.amount,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsInverse,
            &mut self.settings.non_uniform.barndoors.inverse,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsInverseForeground,
            &mut self.settings.non_uniform.barndoors.inverse_foreground,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsGamma,
            &mut self.settings.non_uniform.barndoors.gamma,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsTop,
            &mut self.settings.non_uniform.barndoors.top,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsBottom,
            &mut self.settings.non_uniform.barndoors.bottom,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsLeft,
            &mut self.settings.non_uniform.barndoors.left,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::BarndoorsRight,
            &mut self.settings.non_uniform.barndoors.right,
        );

        create_divider_knob(callback);

        create_knob_with_value(
            callback,
            KnobDefinition::AstigmatismEnable,
            &mut self.settings.non_uniform.astigmatism.enable,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::AstigmatismAmount,
            &mut self.settings.non_uniform.astigmatism.amount,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::AstigmatismGamma,
            &mut self.settings.non_uniform.astigmatism.gamma,
        );

        create_divider_knob(callback);

        create_knob_with_value(
            callback,
            KnobDefinition::AxialAberrationEnable,
            &mut self.settings.non_uniform.axial_aberration.enable,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::AxialAberrationAmount,
            &mut self.settings.non_uniform.axial_aberration.amount,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::AxialAberrationType,
            &mut self.settings.non_uniform.axial_aberration.color_type,
        );

        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::InverseForegroundFilterShape,
            &mut self.settings.non_uniform.inverse_foreground,
        );
    }

    pub fn create_filter_knobs(&mut self, callback: &Knob_Callback) {
        create_tab_knob(
            callback,
            KnobParameters::create("").with_label("Bokeh").build(),
        );
        create_knob_with_value(
            callback,
            KnobDefinition::PreviewFilter,
            &mut self.settings.render.filter.preview,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::FilterType,
            &mut self.nuke_settings.filter_type,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::FilterResolution,
            &mut self.settings.render.filter.resolution,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::RingColor,
            &mut self.settings.bokeh.ring_color,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::InnerColor,
            &mut self.settings.bokeh.inner_color,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::RingSize,
            &mut self.settings.bokeh.ring_size,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::OuterBlur,
            &mut self.settings.bokeh.outer_blur,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::InnerBlur,
            &mut self.settings.bokeh.inner_blur,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::Blades,
            &mut self.settings.bokeh.blades,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::Angle,
            &mut self.settings.bokeh.angle,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::Curvature,
            &mut self.settings.bokeh.curvature,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::NoiseSize,
            &mut self.settings.bokeh.noise.size,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::NoiseIntensity,
            &mut self.settings.bokeh.noise.intensity,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::NoiseSeed,
            &mut self.settings.bokeh.noise.seed,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::AspectRatio,
            &mut self.settings.bokeh.aspect_ratio,
        );
    }

    pub fn calculate_filter_box(&self) -> [u32; 4] {
        self.settings
            .render
            .filter
            .calculate_filter_box(self.settings.bokeh.aspect_ratio)
    }

    /// Initialize the defocus knobs tab
    pub fn create_defocus_knobs(&mut self, callback: &Knob_Callback) {
        create_tab_knob(
            callback,
            KnobParameters::create("").with_label("OpenDefocus").build(),
        );
        create_knob_with_value(
            callback,
            KnobDefinition::Channels,
            &mut self.nuke_settings.channels,
        );
        create_newline_knob(
            callback,
            KnobParameters::create("")
                .with_label("Local device: ")
                .build(),
        );
        create_knob(callback, KnobDefinition::DeviceName);
        create_knob_with_value(
            callback,
            KnobDefinition::UseGpuIfAvailable,
            &mut self.settings.render.use_gpu_if_available,
        );
        create_newline_knob(callback, KnobParameters::create("").build());
        create_divider_knob(callback);
        create_knob_with_value(callback, KnobDefinition::Mode, &mut self.nuke_settings.mode);
        create_knob_with_value(
            callback,
            KnobDefinition::RenderResult,
            &mut self.settings.render.result_mode,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::ShowImage,
            &mut self.settings.defocus.show_image,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::DepthChannel,
            &mut self.nuke_settings.depth_channel,
        );
        create_knob_with_value(callback, KnobDefinition::Math, &mut self.nuke_settings.math);
        create_knob_with_value(
            callback,
            KnobDefinition::WorldUnit,
            &mut self.nuke_settings.world_unit,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::Quality,
            &mut self.settings.render.quality,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::FarmQuality,
            &mut self.settings.render.farm_quality,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::Samples,
            &mut self.settings.render.samples,
        );
        create_divider_knob(callback);
        create_knob_with_value(
            callback,
            KnobDefinition::FocusPlane,
            &mut self.settings.defocus.circle_of_confusion.focal_plane,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::UseCameraFocal,
            &mut self.settings.defocus.use_camera_focal,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::FocusPointUtility,
            &mut self.nuke_settings.focal_point,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::ProtectRange,
            &mut self.settings.defocus.circle_of_confusion.protect,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::Size,
            &mut self.settings.defocus.circle_of_confusion.size,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::MaxSize,
            &mut self.settings.defocus.circle_of_confusion.max_size,
        );
        create_knob_with_value(
            callback,
            KnobDefinition::CameraMaxSize,
            &mut self.settings.defocus.camera_max_size,
        );
        create_divider_knob(callback);
        create_text_knob(
            callback,
            KnobParameters::create("")
                .with_label(&version_footer())
                .build(),
        );
    }

    pub fn knob_changed(&mut self, node: &Op, knob_name: String) -> Result<bool> {
        // TODO optimize to just call when actually needed, for now this is fine

        if knob_name == KnobDefinition::UseGpuIfAvailable.to_snake_case() {
            self.renderer = self.runtime.block_on(OpenDefocusRenderer::new(
                self.settings.render.use_gpu_if_available,
                &mut self.settings,
            ))?;
        };
        if knob_name == "inputChange" && input_connected(node, CAMERA_INPUT) {
            set_knobchanged(
                node,
                KnobDefinition::Mode.to_snake_case(),
                KnobChanged::new(true, true)
                    .with_value_change(ValueChange::int(2))
                    .build(),
            );
        }

        if knob_name == KnobDefinition::FocusPointUtility.to_snake_case() {
            self.focus_point_knobchanged(node);
        }

        for knob in KnobDefinition::iter() {
            let knob_changed = knob.knob_changed(&self.settings, &self.nuke_settings, node);
            set_knobchanged(node, knob.parameters().name, knob_changed);
        }
        Ok(true)
    }

    fn focus_point_knobchanged(&mut self, node: &Op) {
        let sampled_value = sample_channel(
            node,
            self.nuke_settings.depth_channel.clone(),
            self.nuke_settings.focal_point,
        );

        if self.nuke_settings.mode == Mode::Camera && self.settings.defocus.use_camera_focal {
            set_knobchanged(
                node,
                KnobDefinition::UseCameraFocal.parameters().name,
                KnobChanged::new(true, true)
                    .with_value_change(ValueChange::bool(false))
                    .build(),
            )
        };

        if sampled_value != 0.0 {
            set_knobchanged(
                node,
                KnobDefinition::FocusPlane.parameters().name,
                KnobChanged::new(true, true)
                    .with_value_change(ValueChange::float(sampled_value))
                    .build(),
            )
        }
    }

    pub fn set_proxy_scale(&mut self, value: f32) {
        self.settings.defocus.proxy_scale = Some(value);
    }
}

fn init_log() -> Result<()> {
    let filter = {
        let level = if std::env::var("DEBUG_LOGS").is_ok() {
            LevelFilter::DEBUG
        } else {
            LevelFilter::WARN
        };
        EnvFilter::builder()
            .with_default_directive(level.into())
            .from_env()?
            .add_directive("wgpu_hal::=warn".parse()?)
            .add_directive("wgpu_core::instance=error".parse()?)
            .add_directive("wgpu_hal::dx12::device=error".parse()?)
            .add_directive("naga::front::spv=error".parse()?)
            .add_directive("naga=warn".parse()?)
    };
    let _ = tracing_subscriber::fmt()
        .compact()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
    Ok(())
}

impl KnobParameters {
    fn create(name: &str) -> Self {
        Self {
            name: name.to_string(),
            label: String::default(),
            enum_labels: Vec::new(),
            tooltip: String::default(),
            with_flags: FlagMask::default().bits(),
            without_flags: 0,
            input: 0,
            count: 0,
            range: [0.0; 2],
        }
    }
    pub fn with_label(&mut self, label: &str) -> &mut Self {
        self.label = label.to_owned();
        self
    }

    pub fn with_enum_label(&mut self, label: &str) -> &mut Self {
        self.enum_labels.push(label.to_owned());
        self
    }
    pub fn with_tooltip(&mut self, tooltip: &str) -> &mut Self {
        self.tooltip = tooltip.to_owned();
        self
    }
    pub fn with_flag(&mut self, flags: FlagMask) -> &mut Self {
        self.with_flags = (FlagMask::from_bits(self.with_flags).unwrap_or_default() | flags).bits();
        self
    }
    pub fn without_flag(&mut self, flags: FlagMask) -> &mut Self {
        self.without_flags =
            (FlagMask::from_bits(self.without_flags).unwrap_or_default() | flags).bits();
        self
    }
    pub fn with_input(&mut self, input: u32) -> &mut Self {
        self.input = input;
        self
    }
    pub fn with_count(&mut self, count: u32) -> &mut Self {
        self.count = count;
        self
    }
    pub fn with_range(&mut self, start: f32, end: f32) -> &mut Self {
        self.range = [start, end];
        self
    }
    pub fn build(&self) -> Self {
        self.to_owned()
    }
}

pub fn log_info(msg: String) {
    log::info!("{msg}")
}
pub fn log_warning(msg: String) {
    log::warn!("{msg}")
}
pub fn log_error(msg: String) {
    log::error!("{msg}")
}

fn version_footer() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let current_month = Local::now().month();
    let decoration = if current_month == 12 { "🎄" } else { "" };

    format!(
        "<div style='color: #808080;'>
            OpenDefocus v{version} {decoration}
        </div>",
        decoration = decoration,
        version = version
    )
}
