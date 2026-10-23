//! Plain Rust data types for the `OpenDefocus` settings.
//!
//! Messages nested in a message live in a module named after it, and enums are
//! `#[repr(i32)]` so that enum-typed fields are stored as `i32`; unknown raw values
//! survive round-trips. Use the typed getters and setters to access enum fields.

/// Error returned when an `i32` is not a valid variant of an enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownEnumValue(pub i32);

impl core::fmt::Display for UnknownEnumValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown enumeration value {}", self.0)
    }
}

impl std::error::Error for UnknownEnumValue {}

/// Vector containing 4 i32's
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IVector4 {
    /// x
    pub x: i32,
    /// y
    pub y: i32,
    /// z
    pub z: i32,
    /// w
    pub w: i32,
}

/// Vector containing 2 u32's
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UVector2 {
    /// x
    pub x: u32,
    /// y
    pub y: u32,
}

/// Vector containing 2 floats
#[derive(Clone, Debug, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vector2 {
    /// x
    pub x: f32,
    /// y
    pub y: f32,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Empty placeholder for the `defocus` settings group. Its fields live in [`defocus::Settings`].
pub struct Defocus {}

/// Nested message and enum types in `Defocus`.
pub mod defocus {
    use super::UnknownEnumValue;

    /// Settings that define the defocus calculation.
    #[derive(Clone, Debug, Copy, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Settings {
        /// Render 2d or use depth
        pub defocus_mode: i32,
        /// Circle of confusion settings
        pub circle_of_confusion: circle_of_confusion::Settings,
        /// Flag indicating whether to show the image
        pub show_image: bool,
        /// Maximum size for camera-based defocus
        pub camera_max_size: f32,
        /// Flag indicating whether to use camera focal distance
        pub use_camera_focal: bool,
        /// Flag indicating wheter to directly use the input depth
        pub use_direct_math: bool,
        /// Flag indicating whether to use camera focal distance
        pub focal_plane_offset: f32,
        /// Size multiplier for defocus
        pub size_multiplier: f32,
        /// Proxy scale factor
        pub proxy_scale: Option<f32>,
        /// Gamma correction to boost bokehs
        pub gamma_correction: f32,
    }

    impl Default for Settings {
        fn default() -> Self {
            Self {
                defocus_mode: DefocusMode::Twod as i32,
                circle_of_confusion: circle_of_confusion::Settings::default(),
                show_image: false,
                camera_max_size: 10.0,
                use_camera_focal: true,
                use_direct_math: false,
                focal_plane_offset: 0.0,
                size_multiplier: 1.0,
                proxy_scale: None,
                gamma_correction: 1.0,
            }
        }
    }

    impl Settings {
        /// Returns the value of `defocus_mode`, or the default if it is set to an invalid enum value.
        #[must_use]
        pub fn defocus_mode(&self) -> DefocusMode {
            TryFrom::try_from(self.defocus_mode).unwrap_or(DefocusMode::Twod)
        }

        /// Sets `defocus_mode` to the provided enum value.
        pub fn set_defocus_mode(&mut self, value: DefocusMode) {
            self.defocus_mode = value as i32;
        }

        /// Returns the value of `proxy_scale`, or the default value if `proxy_scale` is unset.
        #[must_use]
        pub fn proxy_scale(&self) -> f32 {
            self.proxy_scale.unwrap_or(1.0)
        }
    }

    /// Nested enum types in `Settings`.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[repr(i32)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum DefocusMode {
        /// Simple 2d convolution
        #[default]
        Twod = 0,
        /// Convolution by distance
        Depth = 1,
    }

    impl DefocusMode {
        /// Returns `true` if `value` is a variant of `DefocusMode`.
        #[must_use]
        pub const fn is_valid(value: i32) -> bool {
            matches!(value, 0 | 1)
        }

        /// String value of the enum field names used in the settings definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// and safe for programmatic use.
        #[must_use]
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Twod => "TWOD",
                Self::Depth => "DEPTH",
            }
        }

        /// Creates an enum from field names used in the settings definition.
        #[must_use]
        pub fn from_str_name(value: &str) -> Option<Self> {
            match value {
                "TWOD" => Some(Self::Twod),
                "DEPTH" => Some(Self::Depth),
                _ => None,
            }
        }
    }

    impl From<DefocusMode> for i32 {
        fn from(value: DefocusMode) -> i32 {
            value as i32
        }
    }

    impl TryFrom<i32> for DefocusMode {
        type Error = UnknownEnumValue;

        fn try_from(value: i32) -> Result<Self, UnknownEnumValue> {
            match value {
                0 => Ok(Self::Twod),
                1 => Ok(Self::Depth),
                _ => Err(UnknownEnumValue(value)),
            }
        }
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Empty placeholder for the `non_uniform` settings group. Its fields live in [`non_uniform::Settings`].
pub struct NonUniform {}

/// Nested message and enum types in `NonUniform`.
pub mod non_uniform {
    use super::UnknownEnumValue;

    /// Artifact that causes the bokeh shape to be cut by the distance to the
    /// center of the lens.
    #[derive(Clone, Debug, Copy, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[allow(
        clippy::struct_excessive_bools,
        reason = "mirrors the independent boolean flags of the settings schema"
    )]
    pub struct Catseye {
        /// Flag indicating whether the catseye effect is enabled
        pub enable: bool,
        /// Amount of the catseye effect
        pub amount: f32,
        /// Gamma correction for the effect
        pub gamma: f32,
        /// Softness of the effect edges
        pub softness: f32,
        /// Flag indicating whether to invert the effect
        pub inverse: bool,
        /// Flag indicating whether to invert the effect for foreground
        pub inverse_foreground: bool,
        /// Flag indicating whether the effect should be relative to screen size
        pub relative_to_screen: bool,
    }

    impl Default for Catseye {
        fn default() -> Self {
            Self {
                enable: false,
                amount: 0.5,
                gamma: 1.0,
                softness: 0.2,
                inverse: false,
                inverse_foreground: true,
                relative_to_screen: false,
            }
        }
    }

    /// Artifact that is caused by the light rays entering the lens at a different
    /// rate at different focal distances.
    #[derive(Clone, Debug, Copy, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct AxialAberration {
        /// Flag indicating whether axial aberration is enabled
        pub enable: bool,
        /// Amount of the axial aberration effect
        pub amount: f32,
        /// Color type for the aberration
        pub color_type: i32,
    }

    impl Default for AxialAberration {
        fn default() -> Self {
            Self {
                enable: false,
                amount: 0.5,
                color_type: AxialAberrationType::RedBlue as i32,
            }
        }
    }

    impl AxialAberration {
        /// Returns the value of `color_type`, or the default if it is set to an invalid enum value.
        #[must_use]
        pub fn color_type(&self) -> AxialAberrationType {
            TryFrom::try_from(self.color_type).unwrap_or(AxialAberrationType::RedBlue)
        }

        /// Sets `color_type` to the provided enum value.
        pub fn set_color_type(&mut self, value: AxialAberrationType) {
            self.color_type = value as i32;
        }
    }

    /// Artifact that is caused by light being blocked on the lens by matte boxes.
    ///
    /// This struct represents barndoors, which simulates physical obstacles
    /// that partially block light from reaching the lens.
    #[derive(Clone, Debug, Copy, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Barndoors {
        /// Flag indicating whether barndoors are enabled
        pub enable: bool,
        /// Amount of the barndoors effect
        pub amount: f32,
        /// Gamma correction for the effect
        pub gamma: f32,
        /// Flag indicating whether to invert the effect
        pub inverse: bool,
        /// Flag indicating whether to invert the effect for foreground
        pub inverse_foreground: bool,
        /// Top position of the barndoors
        pub top: f32,
        /// Bottom position of the barndoors
        pub bottom: f32,
        /// Left position of the barndoors
        pub left: f32,
        /// Right position of the barndoors
        pub right: f32,
    }

    impl Default for Barndoors {
        fn default() -> Self {
            Self {
                enable: false,
                amount: 0.5,
                gamma: 1.0,
                inverse: false,
                inverse_foreground: true,
                top: 100.0,
                bottom: 100.0,
                left: 100.0,
                right: 100.0,
            }
        }
    }

    /// Artifact that causes the kernel shape to be stretched more towards the edge
    /// of the image.
    #[derive(Clone, Debug, Copy, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Astigmatism {
        /// Flag indicating whether astigmatism is enabled
        pub enable: bool,
        /// Amount of the astigmatism effect
        pub amount: f32,
        /// Gamma correction for the effect
        pub gamma: f32,
    }

    impl Default for Astigmatism {
        fn default() -> Self {
            Self {
                enable: false,
                amount: 0.5,
                gamma: 1.0,
            }
        }
    }

    /// Combination struct of all non uniform settings.
    ///
    /// These artifacts replicate unique artifacts,
    /// that are different according to the position of the convolution.
    #[derive(Clone, Debug, Copy, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Settings {
        /// Catseye effect settings
        pub catseye: Catseye,
        /// Axial aberration settings
        pub axial_aberration: AxialAberration,
        /// Astigmatism settings
        pub astigmatism: Astigmatism,
        /// Barndoors settings
        pub barndoors: Barndoors,
        /// Flag indicating whether to invert effects for foreground
        pub inverse_foreground: bool,
    }

    impl Default for Settings {
        fn default() -> Self {
            Self {
                catseye: Catseye::default(),
                axial_aberration: AxialAberration::default(),
                astigmatism: Astigmatism::default(),
                barndoors: Barndoors::default(),
                inverse_foreground: true,
            }
        }
    }

    /// Color combination for axial aberration.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[repr(i32)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum AxialAberrationType {
        /// Red and blue color combination
        #[default]
        RedBlue = 0,
        /// Blue and yellow color combination
        BlueYellow = 1,
        /// Green and purple color combination
        GreenPurple = 2,
    }

    impl AxialAberrationType {
        /// Returns `true` if `value` is a variant of `AxialAberrationType`.
        #[must_use]
        pub const fn is_valid(value: i32) -> bool {
            matches!(value, 0..=2)
        }

        /// String value of the enum field names used in the settings definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// and safe for programmatic use.
        #[must_use]
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::RedBlue => "RED_BLUE",
                Self::BlueYellow => "BLUE_YELLOW",
                Self::GreenPurple => "GREEN_PURPLE",
            }
        }

        /// Creates an enum from field names used in the settings definition.
        #[must_use]
        pub fn from_str_name(value: &str) -> Option<Self> {
            match value {
                "RED_BLUE" => Some(Self::RedBlue),
                "BLUE_YELLOW" => Some(Self::BlueYellow),
                "GREEN_PURPLE" => Some(Self::GreenPurple),
                _ => None,
            }
        }
    }

    impl From<AxialAberrationType> for i32 {
        fn from(value: AxialAberrationType) -> i32 {
            value as i32
        }
    }

    impl TryFrom<i32> for AxialAberrationType {
        type Error = UnknownEnumValue;

        fn try_from(value: i32) -> Result<Self, UnknownEnumValue> {
            match value {
                0 => Ok(Self::RedBlue),
                1 => Ok(Self::BlueYellow),
                2 => Ok(Self::GreenPurple),
                _ => Err(UnknownEnumValue(value)),
            }
        }
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Empty placeholder for the `render` settings group. Its fields live in [`render::Settings`].
pub struct Render {}

/// Nested message and enum types in `Render`.
pub mod render {
    use super::UnknownEnumValue;

    /// Filter settings
    #[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Filter {
        /// Render the filter only
        pub preview: bool,
        /// Filter resolution
        pub resolution: u32,
        /// Filter mode that will be used as convolution filter.
        pub mode: i32,
    }

    impl Default for Filter {
        fn default() -> Self {
            Self {
                preview: false,
                resolution: 256,
                mode: FilterMode::Simple as i32,
            }
        }
    }

    impl Filter {
        /// Returns the value of `mode`, or the default if it is set to an invalid enum value.
        #[must_use]
        pub fn mode(&self) -> FilterMode {
            TryFrom::try_from(self.mode).unwrap_or(FilterMode::Simple)
        }

        /// Sets `mode` to the provided enum value.
        pub fn set_mode(&mut self, value: FilterMode) {
            self.mode = value as i32;
        }
    }

    /// Rendering settings
    #[derive(Clone, Debug, PartialEq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Settings {
        /// Render mode to use as output
        pub result_mode: i32,
        /// Resolution of the render (width, height)
        pub resolution: super::UVector2,
        /// Center point of the render
        pub center: super::Vector2,
        /// Quality level for rendering
        pub quality: i32,
        /// Quality level for farm rendering (background processing)
        pub farm_quality: i32,
        /// Flag indicating whether GUI components should be used
        pub gui: bool,
        /// Flag indicating whether to invert the Y axis
        pub inverse_y: bool,
        /// Flag indicating whether to use GPU if available
        pub use_gpu_if_available: bool,
        /// Settings to specify how to render the filter
        pub filter: Filter,
        /// Stop rendering after this time
        pub timeout: Option<u32>,
        /// Device name used for rendering
        pub device_name: Option<String>,
    }

    impl Default for Settings {
        fn default() -> Self {
            Self {
                result_mode: ResultMode::Result as i32,
                resolution: super::UVector2::default(),
                center: super::Vector2::default(),
                quality: Quality::Fast as i32,
                farm_quality: Quality::Accurate as i32,
                gui: true,
                inverse_y: false,
                use_gpu_if_available: true,
                filter: Filter::default(),
                timeout: None,
                device_name: None,
            }
        }
    }

    impl Settings {
        /// Returns the value of `result_mode`, or the default if it is set to an invalid enum value.
        #[must_use]
        pub fn result_mode(&self) -> ResultMode {
            TryFrom::try_from(self.result_mode).unwrap_or(ResultMode::Result)
        }

        /// Sets `result_mode` to the provided enum value.
        pub fn set_result_mode(&mut self, value: ResultMode) {
            self.result_mode = value as i32;
        }

        /// Returns the value of `quality`, or the default if it is set to an invalid enum value.
        #[must_use]
        pub fn quality(&self) -> Quality {
            TryFrom::try_from(self.quality).unwrap_or(Quality::Fast)
        }

        /// Sets `quality` to the provided enum value.
        pub fn set_quality(&mut self, value: Quality) {
            self.quality = value as i32;
        }

        /// Returns the value of `farm_quality`, or the default if it is set to an invalid enum value.
        #[must_use]
        pub fn farm_quality(&self) -> Quality {
            TryFrom::try_from(self.farm_quality).unwrap_or(Quality::Accurate)
        }

        /// Sets `farm_quality` to the provided enum value.
        pub fn set_farm_quality(&mut self, value: Quality) {
            self.farm_quality = value as i32;
        }

        /// Returns the value of `timeout`, or the default value if `timeout` is unset.
        #[must_use]
        pub fn timeout(&self) -> u32 {
            self.timeout.unwrap_or(60)
        }

        /// Returns the value of `device_name`, or the default value if `device_name` is unset.
        #[must_use]
        pub fn device_name(&self) -> &str {
            match self.device_name {
                Some(ref val) => &val[..],
                None => "CPU - Native",
            }
        }
    }

    /// Rendering specifications
    #[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct RenderSpecs {
        /// Full region to be processed (x, y, width, height)
        pub full_region: super::IVector4,
        /// Region to be rendered (x, y, width, height)
        pub render_region: super::IVector4,
    }

    /// Quality level for rendering
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[repr(i32)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum Quality {
        /// Sampled gathers: fastest, approximate
        #[default]
        Fast = 0,
        /// Exact source-pixel integration: slowest, highest precision
        Accurate = 1,
    }

    impl Quality {
        /// Returns `true` if `value` is a variant of `Quality`.
        #[must_use]
        pub const fn is_valid(value: i32) -> bool {
            matches!(value, 0..=1)
        }

        /// String value of the enum field names used in the settings definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// and safe for programmatic use.
        #[must_use]
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Fast => "FAST",
                Self::Accurate => "ACCURATE",
            }
        }

        /// Creates an enum from field names used in the settings definition.
        #[must_use]
        pub fn from_str_name(value: &str) -> Option<Self> {
            match value {
                "FAST" => Some(Self::Fast),
                "ACCURATE" => Some(Self::Accurate),
                _ => None,
            }
        }
    }

    impl From<Quality> for i32 {
        fn from(value: Quality) -> i32 {
            value as i32
        }
    }

    impl TryFrom<i32> for Quality {
        type Error = UnknownEnumValue;

        fn try_from(value: i32) -> Result<Self, UnknownEnumValue> {
            match value {
                0 => Ok(Self::Fast),
                1 => Ok(Self::Accurate),
                _ => Err(UnknownEnumValue(value)),
            }
        }
    }

    /// Filter mode that will be used as convolution filter.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[repr(i32)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum FilterMode {
        /// Simple is the most lightweight, as its calculated without much effort on the fly
        #[default]
        Simple = 0,
        /// Generate using the bokeh creator
        BokehCreator = 1,
        /// Image mode that uses the input image for convolution filter
        Image = 3,
    }

    impl FilterMode {
        /// Returns `true` if `value` is a variant of `FilterMode`.
        #[must_use]
        pub const fn is_valid(value: i32) -> bool {
            matches!(value, 0 | 1 | 3)
        }

        /// String value of the enum field names used in the settings definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// and safe for programmatic use.
        #[must_use]
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Simple => "SIMPLE",
                Self::BokehCreator => "BOKEH_CREATOR",
                Self::Image => "IMAGE",
            }
        }

        /// Creates an enum from field names used in the settings definition.
        #[must_use]
        pub fn from_str_name(value: &str) -> Option<Self> {
            match value {
                "SIMPLE" => Some(Self::Simple),
                "BOKEH_CREATOR" => Some(Self::BokehCreator),
                "IMAGE" => Some(Self::Image),
                _ => None,
            }
        }
    }

    impl From<FilterMode> for i32 {
        fn from(value: FilterMode) -> i32 {
            value as i32
        }
    }

    impl TryFrom<i32> for FilterMode {
        type Error = UnknownEnumValue;

        fn try_from(value: i32) -> Result<Self, UnknownEnumValue> {
            match value {
                0 => Ok(Self::Simple),
                1 => Ok(Self::BokehCreator),
                3 => Ok(Self::Image),
                _ => Err(UnknownEnumValue(value)),
            }
        }
    }

    /// Render mode to use as output
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    #[repr(i32)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub enum ResultMode {
        /// Standard defocus result
        #[default]
        Result = 0,
        /// Focal plane setup visualization
        FocalPlaneSetup = 1,
    }

    impl ResultMode {
        /// Returns `true` if `value` is a variant of `ResultMode`.
        #[must_use]
        pub const fn is_valid(value: i32) -> bool {
            matches!(value, 0 | 1)
        }

        /// String value of the enum field names used in the settings definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// and safe for programmatic use.
        #[must_use]
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Self::Result => "RESULT",
                Self::FocalPlaneSetup => "FOCAL_PLANE_SETUP",
            }
        }

        /// Creates an enum from field names used in the settings definition.
        #[must_use]
        pub fn from_str_name(value: &str) -> Option<Self> {
            match value {
                "RESULT" => Some(Self::Result),
                "FOCAL_PLANE_SETUP" => Some(Self::FocalPlaneSetup),
                _ => None,
            }
        }
    }

    impl From<ResultMode> for i32 {
        fn from(value: ResultMode) -> i32 {
            value as i32
        }
    }

    impl TryFrom<i32> for ResultMode {
        type Error = UnknownEnumValue;

        fn try_from(value: i32) -> Result<Self, UnknownEnumValue> {
            match value {
                0 => Ok(Self::Result),
                1 => Ok(Self::FocalPlaneSetup),
                _ => Err(UnknownEnumValue(value)),
            }
        }
    }
}

/// Settings object that contains all `OpenDefocus` settings.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Settings {
    /// Bokeh settings
    pub bokeh: bokeh_creator::Settings,
    /// Defocus settings
    pub defocus: defocus::Settings,
    /// Non-uniform defocus settings
    pub non_uniform: non_uniform::Settings,
    /// Rendering settings
    pub render: render::Settings,
}

#[cfg(test)]
/// Tests for the enum conversions and typed getters of the settings model.
mod tests;
