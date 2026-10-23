//! Plain Rust data types for the circle of confusion settings.
//!
//! Enum-typed fields are stored as `i32` so that unknown raw values survive
//! round-trips; use the typed getters and setters to access them.

/// Error returned when an `i32` is not a valid variant of an enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownEnumValue(pub i32);

impl core::fmt::Display for UnknownEnumValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown enumeration value {}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UnknownEnumValue {}

/// Size of sensor
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct Filmback {
    /// Width of sensor
    pub width: f32,
    /// Height of sensor
    pub height: f32,
}

impl Default for Filmback {
    fn default() -> Self {
        Self {
            width: 24.576,
            height: 18.672,
        }
    }
}

/// Resolution of recording
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct Resolution {
    /// Width of plate
    pub width: u32,
    /// Height of plate
    pub height: u32,
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

/// World unit used for depth measurements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub enum WorldUnit {
    /// Meters
    M = 3,
    /// Millimeters
    Mm = 0,
    /// Centimeters
    Cm = 1,
    /// Decimeters
    Dm = 2,
    /// Inches
    In = 4,
    /// Feet
    Ft = 5,
}

impl WorldUnit {
    /// String value of the enum field names used in the schema.
    #[must_use]
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::M => "M",
            Self::Mm => "MM",
            Self::Cm => "CM",
            Self::Dm => "DM",
            Self::In => "IN",
            Self::Ft => "FT",
        }
    }

    /// Creates an enum from field names used in the schema.
    #[must_use]
    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "M" => Some(Self::M),
            "MM" => Some(Self::Mm),
            "CM" => Some(Self::Cm),
            "DM" => Some(Self::Dm),
            "IN" => Some(Self::In),
            "FT" => Some(Self::Ft),
            _ => None,
        }
    }
}

impl From<WorldUnit> for i32 {
    fn from(value: WorldUnit) -> i32 {
        value as i32
    }
}

impl TryFrom<i32> for WorldUnit {
    type Error = UnknownEnumValue;

    fn try_from(value: i32) -> Result<WorldUnit, UnknownEnumValue> {
        match value {
            3 => Ok(WorldUnit::M),
            0 => Ok(WorldUnit::Mm),
            1 => Ok(WorldUnit::Cm),
            2 => Ok(WorldUnit::Dm),
            4 => Ok(WorldUnit::In),
            5 => Ok(WorldUnit::Ft),
            _ => Err(UnknownEnumValue(value)),
        }
    }
}

/// Math mode of provided depth channel
///
/// This enum defines how the depth channel's mathematical representation
/// should be interpreted. The mode is specified by the render engine that
/// generates the depth information.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub enum Math {
    /// One divided by Z (inverse depth)
    OneDividedByZ = 1,
    /// Real depth values
    Real = 0,
}

impl Math {
    /// String value of the enum field names used in the schema.
    #[must_use]
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::OneDividedByZ => "ONE_DIVIDED_BY_Z",
            Self::Real => "REAL",
        }
    }

    /// Creates an enum from field names used in the schema.
    #[must_use]
    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "ONE_DIVIDED_BY_Z" => Some(Self::OneDividedByZ),
            "REAL" => Some(Self::Real),
            _ => None,
        }
    }
}

impl From<Math> for i32 {
    fn from(value: Math) -> i32 {
        value as i32
    }
}

impl TryFrom<i32> for Math {
    type Error = UnknownEnumValue;

    fn try_from(value: i32) -> Result<Math, UnknownEnumValue> {
        match value {
            1 => Ok(Math::OneDividedByZ),
            0 => Ok(Math::Real),
            _ => Err(UnknownEnumValue(value)),
        }
    }
}

/// Camera parameters that are used when using the camera-based circle of
/// confusion
///
/// This struct contains camera-specific parameters that are used to calculate
/// circle of confusion.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct CameraData {
    /// Focal length of the camera lens
    pub focal_length: f32,
    /// F-stop (aperture) of the camera
    pub f_stop: f32,
    /// Filmback size (width and height)
    pub filmback: Filmback,
    /// Near field distance for depth of field
    pub near_field: f32,
    /// Far field distance for depth of field
    pub far_field: f32,
    /// World unit for depth measurements (raw enum value, see [`WorldUnit`])
    pub world_unit: i32,
    /// Resolution camera is recording at
    pub resolution: Resolution,
}

impl Default for CameraData {
    fn default() -> Self {
        Self {
            focal_length: 50.0,
            f_stop: 16.0,
            filmback: Filmback::default(),
            near_field: 0.1,
            far_field: 10000.0,
            world_unit: WorldUnit::M as i32,
            resolution: Resolution::default(),
        }
    }
}

impl CameraData {
    /// Returns the `world_unit` field as a [`WorldUnit`].
    ///
    /// Unknown values fall back to [`WorldUnit::M`], the first declared variant.
    #[must_use]
    pub fn world_unit(&self) -> WorldUnit {
        WorldUnit::try_from(self.world_unit).unwrap_or(WorldUnit::M)
    }

    /// Sets the `world_unit` field from a [`WorldUnit`].
    pub fn set_world_unit(&mut self, value: WorldUnit) {
        self.world_unit = value as i32;
    }
}

/// Settings for the circle of confusion calculation.
///
/// This struct contains all the parameters needed to calculate the circle of
/// confusion.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct Settings {
    /// Base size for circle of confusion
    pub size: f32,
    /// Maximum size for circle of confusion (clamped)
    pub max_size: f32,
    /// Mathematical interpretation of depth (raw enum value, see [`Math`])
    pub math: i32,
    /// Focal plane position
    pub focal_plane: f32,
    /// Protection value for circle of confusion, this adds a region that has a `CoC`
    /// of zero.
    pub protect: f32,
    /// Pixel aspect ratio
    pub pixel_aspect: f32,
    /// Camera parameters
    pub camera_data: Option<CameraData>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            size: 5.0,
            max_size: 10.0,
            math: Math::OneDividedByZ as i32,
            focal_plane: 0.0,
            protect: 0.0,
            pixel_aspect: 1.0,
            camera_data: None,
        }
    }
}

impl Settings {
    /// Returns the `math` field as a [`Math`].
    ///
    /// Unknown values fall back to [`Math::OneDividedByZ`], the first declared variant.
    #[must_use]
    pub fn math(&self) -> Math {
        Math::try_from(self.math).unwrap_or(Math::OneDividedByZ)
    }

    /// Sets the `math` field from a [`Math`].
    pub fn set_math(&mut self, value: Math) {
        self.math = value as i32;
    }
}

/// Start and end value of the depth of field range.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct DepthOfField {
    /// Near end of the in-focus range, in the same units as the depth value.
    pub x: f32,
    /// Far end of the in-focus range, in the same units as the depth value.
    pub y: f32,
}

/// Calculator state, precomputed from [`Settings`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct Calculator {
    /// `CoC` settings
    pub settings: Settings,
    /// Focus point used for calculations
    pub internal_focus: f32,
    /// Multiplier unit to use for world unit conversion
    pub world_unit_multiplier: f32,
    /// Start and end value of `DoF`
    pub depth_of_field: DepthOfField,
    /// Hyperfocal distance calculated value
    pub hyperfocal_distance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filmback_default_is_super_35_sensor() {
        let filmback = Filmback::default();
        assert_eq!(filmback.width, 24.576);
        assert_eq!(filmback.height, 18.672);
    }

    #[test]
    fn resolution_default_is_full_hd() {
        assert_eq!(
            Resolution::default(),
            Resolution {
                width: 1920,
                height: 1080
            }
        );
    }

    #[test]
    fn camera_data_default_matches_documented_values() {
        let camera = CameraData::default();
        assert_eq!(camera.focal_length, 50.0);
        assert_eq!(camera.f_stop, 16.0);
        assert_eq!(camera.near_field, 0.1);
        assert_eq!(camera.far_field, 10000.0);
        assert_eq!(camera.world_unit, WorldUnit::M as i32);
        assert_eq!(camera.world_unit(), WorldUnit::M);
    }

    #[test]
    fn settings_default_uses_inverse_depth_and_no_camera() {
        let settings = Settings::default();
        assert_eq!(settings.size, 5.0);
        assert_eq!(settings.max_size, 10.0);
        assert_eq!(settings.math(), Math::OneDividedByZ);
        assert_eq!(settings.focal_plane, 0.0);
        assert_eq!(settings.protect, 0.0);
        assert_eq!(settings.pixel_aspect, 1.0);
        assert!(settings.camera_data.is_none());
    }

    #[test]
    fn world_unit_names_round_trip() {
        let units = [
            (WorldUnit::M, "M"),
            (WorldUnit::Mm, "MM"),
            (WorldUnit::Cm, "CM"),
            (WorldUnit::Dm, "DM"),
            (WorldUnit::In, "IN"),
            (WorldUnit::Ft, "FT"),
        ];
        for (unit, name) in units {
            assert_eq!(unit.as_str_name(), name);
            assert_eq!(WorldUnit::from_str_name(name), Some(unit));
        }
        assert_eq!(WorldUnit::from_str_name("mm"), None);
        assert_eq!(WorldUnit::from_str_name(""), None);
    }

    #[test]
    fn world_unit_raw_values_round_trip() {
        let units = [
            (WorldUnit::Mm, 0),
            (WorldUnit::Cm, 1),
            (WorldUnit::Dm, 2),
            (WorldUnit::M, 3),
            (WorldUnit::In, 4),
            (WorldUnit::Ft, 5),
        ];
        for (unit, raw) in units {
            assert_eq!(i32::from(unit), raw);
            assert_eq!(WorldUnit::try_from(raw), Ok(unit));
        }
        assert_eq!(WorldUnit::try_from(6), Err(UnknownEnumValue(6)));
        assert_eq!(WorldUnit::try_from(-1), Err(UnknownEnumValue(-1)));
    }

    #[test]
    fn math_names_round_trip() {
        assert_eq!(Math::Real.as_str_name(), "REAL");
        assert_eq!(Math::OneDividedByZ.as_str_name(), "ONE_DIVIDED_BY_Z");
        assert_eq!(Math::from_str_name("REAL"), Some(Math::Real));
        assert_eq!(
            Math::from_str_name("ONE_DIVIDED_BY_Z"),
            Some(Math::OneDividedByZ)
        );
        assert_eq!(Math::from_str_name("real"), None);
    }

    #[test]
    fn math_raw_values_round_trip() {
        assert_eq!(i32::from(Math::Real), 0);
        assert_eq!(i32::from(Math::OneDividedByZ), 1);
        assert_eq!(Math::try_from(0), Ok(Math::Real));
        assert_eq!(Math::try_from(1), Ok(Math::OneDividedByZ));
        assert_eq!(Math::try_from(2), Err(UnknownEnumValue(2)));
    }

    #[test]
    fn unknown_enum_value_displays_its_raw_value() {
        assert_eq!(
            UnknownEnumValue(7).to_string(),
            "unknown enumeration value 7"
        );
    }

    #[test]
    fn unknown_world_unit_falls_back_to_metres() {
        let mut camera = CameraData {
            world_unit: 42,
            ..CameraData::default()
        };
        assert_eq!(camera.world_unit(), WorldUnit::M);
        camera.set_world_unit(WorldUnit::Ft);
        assert_eq!(camera.world_unit, WorldUnit::Ft as i32);
        assert_eq!(camera.world_unit(), WorldUnit::Ft);
    }

    #[test]
    fn unknown_math_falls_back_to_inverse_depth() {
        let mut settings = Settings {
            math: 42,
            ..Settings::default()
        };
        assert_eq!(settings.math(), Math::OneDividedByZ);
        settings.set_math(Math::Real);
        assert_eq!(settings.math, Math::Real as i32);
        assert_eq!(settings.math(), Math::Real);
    }
}
