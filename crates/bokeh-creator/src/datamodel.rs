//! Settings types for the bokeh kernel: noise, the full settings bundle, and the
//! filter type enum with its integer and schema-name conversions.

/// Noise specific settings.
///
/// These settings can help to add some artifacts to the kernel,
/// to replicate more natural-like bokeh's.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct Noise {
    /// Size of the noise pattern
    pub size: f32,
    /// Intensity of the noise
    pub intensity: f32,
    /// Number of octaves for noise generation
    pub octaves: u32,
    /// Random seed for noise generation
    pub seed: u32,
}

impl Default for Noise {
    fn default() -> Self {
        Self {
            size: 0.1,
            intensity: 0.25,
            octaves: 7,
            seed: 0,
        }
    }
}

/// Data object that contains all bokeh settings.
///
/// This object is used to specify the settings when rendering.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
pub struct Settings {
    /// Type of filter to use for the bokeh
    pub filter_type: i32,
    /// Radius of the bokeh
    pub radius: f32,
    /// Color of the ring in the bokeh
    pub ring_color: f32,
    /// Color of the inner part of the bokeh
    pub inner_color: f32,
    /// Size of the ring in the bokeh
    pub ring_size: f32,
    /// Amount of blur applied to the outer part of the bokeh
    pub outer_blur: f32,
    /// Amount of blur applied to the inner part of the bokeh
    pub inner_blur: f32,
    /// Number of blades for blade-based bokeh filters
    pub blades: u32,
    /// Angle of the bokeh
    pub angle: f32,
    /// Curvature of the bokeh
    pub curvature: f32,
    /// Noise settings for the bokeh
    pub noise: Noise,
    /// Aspect ratio for the bokeh
    pub aspect_ratio: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            filter_type: FilterType::Disc as i32,
            radius: 1.0,
            ring_color: 1.0,
            inner_color: 0.4,
            ring_size: 0.1,
            outer_blur: 0.1,
            inner_blur: 0.05,
            blades: 5,
            angle: 0.0,
            curvature: 0.5,
            noise: Noise::default(),
            aspect_ratio: 1.0,
        }
    }
}

impl Settings {
    /// Returns the `filter_type` field as a [`FilterType`].
    ///
    /// Unknown values fall back to [`FilterType::Disc`].
    #[must_use]
    pub fn filter_type(&self) -> FilterType {
        FilterType::try_from(self.filter_type).unwrap_or(FilterType::Disc)
    }

    /// Sets the `filter_type` field from a [`FilterType`].
    pub fn set_filter_type(&mut self, value: FilterType) {
        self.filter_type = value as i32;
    }
}

/// Enum that contains the type of filter used for the bokeh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "documented", derive(documented::DocumentedFields))]
#[derive(Default)]
pub enum FilterType {
    /// Circular bokeh (basically same as curvature set to 1.0)
    #[default]
    Disc = 0,
    /// Blade-based bokeh (for replicating camera blades)
    Blade = 1,
}

impl FilterType {
    /// Returns `true` if `value` is a variant of `FilterType`.
    #[must_use]
    pub fn is_valid(value: i32) -> bool {
        matches!(value, 0 | 1)
    }

    /// Converts an `i32` to a `FilterType`, or `None` if `value` is not a valid variant.
    #[deprecated = "Use the TryFrom<i32> implementation instead"]
    #[must_use]
    pub fn from_i32(value: i32) -> ::core::option::Option<FilterType> {
        Self::try_from(value).ok()
    }

    /// String value of the enum field names used in the schema.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the schema does not change) and safe for programmatic use.
    #[must_use]
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Disc => "DISC",
            Self::Blade => "BLADE",
        }
    }

    /// Creates an enum from field names used in the schema.
    #[must_use]
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DISC" => Some(Self::Disc),
            "BLADE" => Some(Self::Blade),
            _ => None,
        }
    }
}

impl From<FilterType> for i32 {
    fn from(value: FilterType) -> i32 {
        value as i32
    }
}

impl TryFrom<i32> for FilterType {
    type Error = UnknownEnumValue;

    fn try_from(value: i32) -> Result<FilterType, UnknownEnumValue> {
        match value {
            0 => Ok(FilterType::Disc),
            1 => Ok(FilterType::Blade),
            _ => Err(UnknownEnumValue(value)),
        }
    }
}

/// Error returned when an `i32` is not a valid enum variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownEnumValue(pub i32);

impl core::fmt::Display for UnknownEnumValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown enumeration value {}", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UnknownEnumValue {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts that two `f32` values are equal within `f32::EPSILON`.
    fn assert_f32_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    /// The noise defaults are the documented literal values.
    #[test]
    fn noise_default_values() {
        let noise = Noise::default();
        assert_f32_eq(noise.size, 0.1);
        assert_f32_eq(noise.intensity, 0.25);
        assert_eq!(noise.octaves, 7);
        assert_eq!(noise.seed, 0);
    }

    /// The settings defaults match the literal values and the disc filter.
    #[test]
    fn settings_default_values() {
        let settings = Settings::default();
        assert_eq!(settings.filter_type, 0);
        assert_f32_eq(settings.radius, 1.0);
        assert_f32_eq(settings.ring_color, 1.0);
        assert_f32_eq(settings.inner_color, 0.4);
        assert_f32_eq(settings.ring_size, 0.1);
        assert_f32_eq(settings.outer_blur, 0.1);
        assert_f32_eq(settings.inner_blur, 0.05);
        assert_eq!(settings.blades, 5);
        assert_f32_eq(settings.angle, 0.0);
        assert_f32_eq(settings.curvature, 0.5);
        assert_eq!(settings.noise, Noise::default());
        assert_f32_eq(settings.aspect_ratio, 1.0);
    }

    /// The default settings report the disc filter through the typed getter.
    #[test]
    fn default_filter_type_is_disc() {
        assert_eq!(Settings::default().filter_type(), FilterType::Disc);
        assert_eq!(FilterType::default(), FilterType::Disc);
    }

    /// The typed setter stores the discriminant in the integer field.
    #[test]
    fn set_filter_type_stores_discriminant() {
        let mut settings = Settings::default();
        settings.set_filter_type(FilterType::Blade);
        assert_eq!(settings.filter_type, 1);
        assert_eq!(settings.filter_type(), FilterType::Blade);

        settings.set_filter_type(FilterType::Disc);
        assert_eq!(settings.filter_type, 0);
        assert_eq!(settings.filter_type(), FilterType::Disc);
    }

    /// Unknown integer filter values read back as the disc filter.
    #[test]
    fn unknown_filter_value_falls_back_to_disc() {
        for value in [2, 7, -1, i32::MAX] {
            let settings = Settings {
                filter_type: value,
                ..Settings::default()
            };
            assert_eq!(settings.filter_type(), FilterType::Disc, "value {value}");
        }
    }

    /// `try_from` maps 0 and 1 to their variants and rejects everything else.
    #[test]
    fn try_from_i32_maps_known_and_unknown_values() {
        assert_eq!(FilterType::try_from(0), Ok(FilterType::Disc));
        assert_eq!(FilterType::try_from(1), Ok(FilterType::Blade));
        assert_eq!(FilterType::try_from(2), Err(UnknownEnumValue(2)));
        assert_eq!(FilterType::try_from(-1), Err(UnknownEnumValue(-1)));
    }

    /// `is_valid` accepts exactly the discriminants 0 and 1.
    #[test]
    fn is_valid_accepts_only_known_discriminants() {
        assert!(FilterType::is_valid(0));
        assert!(FilterType::is_valid(1));
        assert!(!FilterType::is_valid(2));
        assert!(!FilterType::is_valid(-1));
        assert!(!FilterType::is_valid(i32::MIN));
    }

    /// The deprecated `from_i32` still returns `Some` for valid values and `None` otherwise.
    #[test]
    #[allow(deprecated, reason = "exercises the deprecated converter")]
    fn deprecated_from_i32_returns_option() {
        assert_eq!(FilterType::from_i32(0), Some(FilterType::Disc));
        assert_eq!(FilterType::from_i32(1), Some(FilterType::Blade));
        assert_eq!(FilterType::from_i32(3), None);
    }

    /// Schema names are the upper-case variant names.
    #[test]
    fn as_str_name_uses_schema_names() {
        assert_eq!(FilterType::Disc.as_str_name(), "DISC");
        assert_eq!(FilterType::Blade.as_str_name(), "BLADE");
    }

    /// Schema names parse to their variants and anything else is rejected, including case variants.
    #[test]
    fn from_str_name_parses_exact_schema_names_only() {
        assert_eq!(FilterType::from_str_name("DISC"), Some(FilterType::Disc));
        assert_eq!(FilterType::from_str_name("BLADE"), Some(FilterType::Blade));
        assert_eq!(FilterType::from_str_name("disc"), None);
        assert_eq!(FilterType::from_str_name("Blade"), None);
        assert_eq!(FilterType::from_str_name(""), None);
    }

    /// Converting a variant into `i32` yields its discriminant.
    #[test]
    fn from_filter_type_into_i32_yields_discriminant() {
        assert_eq!(i32::from(FilterType::Disc), 0);
        assert_eq!(i32::from(FilterType::Blade), 1);
    }

    /// The unknown-value error displays the offending integer.
    #[test]
    fn unknown_enum_value_display_includes_value() {
        assert_eq!(
            UnknownEnumValue(7).to_string(),
            "unknown enumeration value 7"
        );
    }
}
