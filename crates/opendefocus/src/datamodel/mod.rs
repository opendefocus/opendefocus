//! Public settings data model. Re-exports the generated settings types and adds conversions to
//! render regions, filter boxes, sample counts and the GPU convolution uniform.
use crate::convolve_settings::{ConvolveSettings, GlobalFlags, NonUniformFlags};
use circle_of_confusion::Math;
use glam::{IVec4, UVec2, Vec2};

pub use bokeh_creator;
pub use circle_of_confusion;

/// Plain Rust data types for the settings. See that module for the generated layout.
mod model;
pub use self::model::*;

use self::defocus::DefocusMode;
use self::render::{FilterMode, Quality};

impl IVector4 {
    #[must_use]
    /// Converts this vector into a glam `IVec4` with the same x, y, z and w components.
    pub fn to_ivec4(&self) -> IVec4 {
        IVec4::new(self.x, self.y, self.z, self.w)
    }
}

impl UVector2 {
    #[must_use]
    /// Converts this vector into a glam `UVec2` with the same x and y components.
    pub fn to_uvec2(&self) -> UVec2 {
        UVec2::new(self.x, self.y)
    }
}

impl Vector2 {
    #[must_use]
    /// Converts this vector into a glam `Vec2` with the same x and y components.
    pub fn to_vec2(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl render::Filter {
    #[must_use]
    /// Returns the filter sub-box `[x0, y0, x1, y1]` in filter pixels for a bokeh aspect ratio.
    /// The square filter of side `resolution` is trimmed on one axis, so the kept box matches the aspect.
    pub fn calculate_filter_box(&self, aspect_ratio: f32) -> [u32; 4] {
        let resolution = UVector2 {
            x: self.resolution,
            y: self.resolution,
        };
        let offset_multiplier = Vector2 {
            x: 3.0 - aspect_ratio.min(1.0) * 2.0,
            y: aspect_ratio.max(1.0) * 2.0 - 1.0,
        };
        let offset = UVector2 {
            x: ((resolution.x as f32 / offset_multiplier.x) * (1.0 - aspect_ratio).max(0.0)).floor()
                as u32,
            y: ((resolution.y as f32 / offset_multiplier.y) * (aspect_ratio - 1.0).max(0.0)).floor()
                as u32,
        };
        [
            offset.x,
            offset.y,
            resolution.x - offset.x,
            resolution.y - offset.y,
        ]
    }
}

impl render::RenderSpecs {
    /// Creates render specs from rectangles.
    #[must_use]
    pub fn from_rects(full_region: IVec4, render_region: IVec4) -> Self {
        Self {
            full_region: IVector4 {
                x: full_region.x,
                y: full_region.y,
                z: full_region.z,
                w: full_region.w,
            },
            render_region: IVector4 {
                x: render_region.x,
                y: render_region.y,
                z: render_region.z,
                w: render_region.w,
            },
        }
    }

    /// Scale the render specs by a specified factor.
    #[must_use]
    pub fn scale(&self, scale: f32) -> Self {
        let mut scaled_full_region = self.full_region;
        scaled_full_region.x = (scaled_full_region.x as f32 * scale).floor() as i32;
        scaled_full_region.y = (scaled_full_region.y as f32 * scale).floor() as i32;
        scaled_full_region.z = (scaled_full_region.z as f32 * scale).ceil() as i32;
        scaled_full_region.w = (scaled_full_region.w as f32 * scale).ceil() as i32;

        let mut scaled_render_region = self.render_region;
        scaled_render_region.x = (scaled_render_region.x as f32 * scale).floor() as i32;
        scaled_render_region.y = (scaled_render_region.y as f32 * scale).floor() as i32;
        scaled_render_region.z = (scaled_render_region.z as f32 * scale).ceil() as i32;
        scaled_render_region.w = (scaled_render_region.w as f32 * scale).ceil() as i32;
        Self {
            full_region: scaled_full_region,
            render_region: scaled_render_region,
        }
    }

    /// Get the resolution of the full region.
    #[must_use]
    pub fn get_resolution(&self) -> UVec2 {
        UVec2::new(
            (self.full_region.z - self.full_region.x) as u32,
            (self.full_region.w - self.full_region.y) as u32,
        )
    }

    /// Get the resolution of the render region.
    #[must_use]
    pub fn get_render_resolution(&self) -> UVec2 {
        UVec2::new(
            (self.render_region.z - self.render_region.x) as u32,
            (self.render_region.w - self.render_region.y) as u32,
        )
    }
}

impl defocus::Settings {
    /// Calculate the effective defocus size.
    #[must_use]
    pub fn get_size(&self) -> f32 {
        self.circle_of_confusion.size
            * self.circle_of_confusion.pixel_aspect
            * self.size_multiplier.max(0.0)
            * self.proxy_scale.unwrap_or(1.0)
    }

    #[must_use]
    /// Returns the maximum defocus radius before scaling. Uses the camera-derived maximum when camera data is set, otherwise the configured CoC maximum.
    pub fn get_raw_max_size(&self) -> f32 {
        match self.circle_of_confusion.camera_data {
            Some(_) => self.camera_max_size,
            _ => self.circle_of_confusion.max_size,
        }
    }

    /// Returns the size multiplier, clamped to be non-negative.
    fn get_size_multiplier(&self) -> f32 {
        self.size_multiplier.max(0.0)
    }

    /// Returns the proxy scale, or 1 when none is set.
    fn get_proxy_scale(&self) -> f32 {
        self.proxy_scale.unwrap_or(1.0)
    }

    /// Calculate the maximum defocus size based on the current mode.
    #[must_use]
    pub fn get_max_size(&self) -> f32 {
        let max_size = self.get_raw_max_size();
        max_size
            * self.circle_of_confusion.pixel_aspect
            * self.get_size_multiplier()
            * self.get_proxy_scale()
    }

    /// Determines the math interpretation of depth.
    ///
    /// # Panics
    /// Always panics: this is an unimplemented stub with no callers.
    #[must_use]
    #[allow(
        clippy::todo,
        reason = "unimplemented public stub with no callers; semantics not yet defined"
    )]
    pub fn get_math(&self) -> Math {
        todo!()
    }

    /// Gets the current maximum size for defocus.
    #[must_use]
    pub fn get_current_max_size(&self) -> f32 {
        if self.defocus_mode() == DefocusMode::Twod {
            return self.get_size();
        }
        self.get_max_size()
    }

    /// Calculate the padding required for rendering.
    #[must_use]
    pub fn get_padding(&self) -> u32 {
        self.get_current_max_size().ceil() as u32
    }
}

impl render::Settings {
    #[must_use]
    /// Returns the effective quality. The GUI quality is used when `gui` is set, otherwise the farm quality.
    pub fn get_quality(&self) -> Quality {
        if self.gui {
            self.quality()
        } else {
            self.farm_quality()
        }
    }
}

/// Builds the global flag set from the settings and the already computed non-uniform flags.
/// `USE_NON_UNIFORM` is set when any non-uniform effect is enabled.
fn settings_to_globalflags(settings: &Settings, non_uniform_flags: NonUniformFlags) -> GlobalFlags {
    let mut flags = GlobalFlags::empty();
    if settings.render.filter.mode() == FilterMode::Simple {
        flags |= GlobalFlags::SIMPLE_DISC;
    }
    if non_uniform_flags.intersects(
        NonUniformFlags::CATSEYE_ENABLED
            | NonUniformFlags::BARNDOORS_ENABLED
            | NonUniformFlags::ASTIGMATISM_ENABLED,
    ) {
        flags |= GlobalFlags::USE_NON_UNIFORM;
    }

    if settings.defocus.defocus_mode() == DefocusMode::Twod {
        flags |= GlobalFlags::IS_2D;
    }
    if settings.non_uniform.inverse_foreground {
        flags |= GlobalFlags::INVERSE_FOREGROUND_BOKEH_SHAPE;
    }
    if settings.render.inverse_y {
        flags |= GlobalFlags::INVERSE_Y;
    }
    if settings.non_uniform.axial_aberration.enable {
        flags |= GlobalFlags::AXIAL_ABERRATION_ENABLE;
    }
    flags
}

/// Converts the non-uniform settings into their bit flags. Sub-options such as inverse are set only when their effect is enabled.
fn non_uniform_to_flags(settings: &non_uniform::Settings) -> NonUniformFlags {
    let mut non_uniform_flags = NonUniformFlags::empty();
    if settings.catseye.enable {
        non_uniform_flags |= NonUniformFlags::CATSEYE_ENABLED;
        if settings.catseye.inverse {
            non_uniform_flags |= NonUniformFlags::CATSEYE_INVERSE;
        }
        if settings.catseye.inverse_foreground {
            non_uniform_flags |= NonUniformFlags::CATSEYE_INVERSE_FOREGROUND;
        }
        if settings.catseye.relative_to_screen {
            non_uniform_flags |= NonUniformFlags::CATSEYE_SCREEN_RELATIVE;
        }
    }

    if settings.astigmatism.enable {
        non_uniform_flags |= NonUniformFlags::ASTIGMATISM_ENABLED;
    }

    if settings.barndoors.enable {
        non_uniform_flags |= NonUniformFlags::BARNDOORS_ENABLED;
        if settings.barndoors.inverse {
            non_uniform_flags |= NonUniformFlags::BARNDOORS_INVERSE;
        }
        if settings.barndoors.inverse_foreground {
            non_uniform_flags |= NonUniformFlags::BARNDOORS_INVERSE_FOREGROUND;
        }
    }
    non_uniform_flags
}

/// Calculates the number of samples to use for convolution based on render settings and size.
///
/// # Arguments
///
/// * `settings` - Reference to the render settings
/// * `size` - The size parameter used to calculate samples
///
/// # Returns
///
/// The calculated number of samples as an i32 value
#[must_use]
pub fn get_samples(settings: &render::Settings, size: i32) -> u32 {
    let quality = settings.get_quality();
    match quality {
        render::Quality::Fast => (size as f32 * 0.1).clamp(8.0, 20.0) as u32,
        render::Quality::Accurate => (size as f32 * 0.5).clamp(30.0, 100.0) as u32,
    }
}

#[must_use]
/// Builds the GPU convolution uniform from user settings.
/// `render_specs` gives the full and render regions as (x0, y0, x1, y1) in full-frame pixels, `filter_resolution` the filter size,
/// and `image_elements` is stored unchanged. In 2D mode the maximum radius is the constant size; otherwise it is the raw maximum.
pub fn settings_to_convolve_settings(
    settings: &Settings,
    render_specs: &render::RenderSpecs,
    filter_resolution: UVec2,
    image_elements: u32,
) -> ConvolveSettings {
    let max_size = if settings.defocus.defocus_mode() == DefocusMode::Twod {
        settings.defocus.get_size()
    } else {
        settings.defocus.get_raw_max_size()
    };
    let samples = get_samples(&settings.render, max_size.ceil() as i32);
    let filter_aspect_ratio = filter_resolution.x as f32 / filter_resolution.y as f32;
    let filter_aspect_ratio_normalizer = 1.0 / filter_aspect_ratio;
    let ring_distance = max_size / samples as f32;
    let pixel_aspect_normalizer = 1.0 / settings.defocus.circle_of_confusion.pixel_aspect;

    let non_uniform_flags = non_uniform_to_flags(&settings.non_uniform);
    let flags = settings_to_globalflags(settings, non_uniform_flags);

    ConvolveSettings {
        samples,
        center: settings.render.center.to_vec2(),
        process_region: render_specs.render_region.to_ivec4(),
        full_region: render_specs.full_region.to_ivec4(),
        resolution: settings.render.resolution.to_uvec2(),
        pixel_aspect: settings.defocus.circle_of_confusion.pixel_aspect,
        size: settings.defocus.get_size(),
        max_size,
        flags: flags.bits(),
        non_uniform_flags: non_uniform_flags.bits(),
        catseye_amount: settings.non_uniform.catseye.amount,
        catseye_gamma: settings.non_uniform.catseye.gamma,
        catseye_softness: settings.non_uniform.catseye.softness,
        astigmatism_amount: settings.non_uniform.astigmatism.amount,
        astigmatism_gamma: settings.non_uniform.astigmatism.gamma,
        barndoors_amount: settings.non_uniform.barndoors.amount,
        barndoors_gamma: settings.non_uniform.barndoors.gamma,
        barndoors_top: settings.non_uniform.barndoors.top,
        barndoors_bottom: settings.non_uniform.barndoors.bottom,
        barndoors_left: settings.non_uniform.barndoors.left,
        barndoors_right: settings.non_uniform.barndoors.right,
        axial_aberration_amount: settings.non_uniform.axial_aberration.amount,
        axial_aberration_type: match settings.non_uniform.axial_aberration.color_type {
            // The settings enum is zero-based while AxialAberrationType in
            // ConvolveSettings reserves zero as its invalid/default sentinel.
            1 => 2,
            2 => 3,
            // Zero and unknown values both map to 1.
            _ => 1,
        },
        filter_aspect_ratio_normalizer,
        filter_aspect_ratio,
        ring_distance,
        filter_resolution,
        pixel_aspect_normalizer,
        render_scale: 1_u32,
        image_elements,
        _padding: 0,
    }
}

#[cfg(test)]
/// Tests for settings conversion.
mod tests {
    use super::*;
    use crate::convolve_settings::AxialAberrationType;

    #[test]
    /// Verifies every axial aberration colour type survives conversion and unknown values fall back to red-blue.
    fn settings_conversion_preserves_all_axial_aberration_enum_values() {
        let specs = render::RenderSpecs::from_rects(IVec4::ZERO, IVec4::ZERO);
        let mut settings = Settings::default();
        for (raw_value, expected) in [
            (0, AxialAberrationType::RedBlue),
            (1, AxialAberrationType::BlueYellow),
            (2, AxialAberrationType::GreenPurple),
        ] {
            settings.non_uniform.axial_aberration.color_type = raw_value;
            let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
            assert_eq!(
                convolve.get_axial_aberration_settings().color_type as u32,
                expected as u32
            );
        }

        settings.non_uniform.axial_aberration.color_type = 99;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert_eq!(
            convolve.get_axial_aberration_settings().color_type as u32,
            AxialAberrationType::RedBlue as u32
        );
    }

    /// The decoded axial aberration switch follows the axial setting, not barndoors.
    #[test]
    fn axial_aberration_enable_is_independent_of_barndoors() {
        let specs = render::RenderSpecs::from_rects(IVec4::ZERO, IVec4::ZERO);
        let mut settings = Settings::default();
        settings.non_uniform.axial_aberration.enable = true;
        settings.non_uniform.barndoors.enable = false;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert!(convolve.get_axial_aberration_settings().enable);

        settings.non_uniform.axial_aberration.enable = false;
        settings.non_uniform.barndoors.enable = true;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert!(!convolve.get_axial_aberration_settings().enable);
    }

    #[test]
    /// Verifies vector conversions keep every component in order.
    fn vector_types_convert_to_glam_components_in_order() {
        assert_eq!(
            IVector4 {
                x: 1,
                y: -2,
                z: 3,
                w: -4
            }
            .to_ivec4(),
            IVec4::new(1, -2, 3, -4)
        );
        assert_eq!(UVector2 { x: 7, y: 9 }.to_uvec2(), UVec2::new(7, 9));
        assert_eq!(Vector2 { x: 0.5, y: -1.5 }.to_vec2(), Vec2::new(0.5, -1.5));
    }

    #[test]
    /// Verifies the filter box keeps the square for aspect one and trims the opposite axis for other aspects.
    fn filter_box_is_trimmed_on_the_axis_that_matches_the_aspect() {
        let filter = render::Filter {
            resolution: 256,
            ..Default::default()
        };
        assert_eq!(filter.calculate_filter_box(1.0), [0, 0, 256, 256]);
        assert_eq!(filter.calculate_filter_box(0.5), [64, 0, 192, 256]);
        assert_eq!(filter.calculate_filter_box(0.0), [85, 0, 171, 256]);
        assert_eq!(filter.calculate_filter_box(2.0), [0, 85, 256, 171]);
    }

    #[test]
    /// Verifies render regions and their resolutions use the (x0, y0, x1, y1) layout.
    fn render_specs_report_region_resolutions() {
        let specs = render::RenderSpecs::from_rects(
            IVec4::new(0, 0, 1920, 1080),
            IVec4::new(10, 20, 110, 220),
        );
        assert_eq!(specs.full_region.to_ivec4(), IVec4::new(0, 0, 1920, 1080));
        assert_eq!(specs.get_resolution(), UVec2::new(1920, 1080));
        assert_eq!(specs.get_render_resolution(), UVec2::new(100, 200));
    }

    #[test]
    /// Verifies scaling floors the origin, ceils the far edge, and rounds negative origins outward.
    fn render_specs_scale_rounds_origins_down_and_far_edges_up() {
        let specs =
            render::RenderSpecs::from_rects(IVec4::new(-3, -3, 10, 10), IVec4::new(1, 1, 9, 9));
        let scaled = specs.scale(0.5);
        assert_eq!(scaled.full_region.to_ivec4(), IVec4::new(-2, -2, 5, 5));
        assert_eq!(scaled.render_region.to_ivec4(), IVec4::new(0, 0, 5, 5));
        assert_eq!(specs.scale(1.0), specs);
    }

    #[test]
    /// Verifies the effective defocus size multiplies CoC size, pixel aspect and proxy scale, clamping the multiplier at zero.
    fn defocus_size_multiplies_coc_aspect_multiplier_and_proxy() {
        let mut settings = defocus::Settings::default();
        assert!((settings.get_size() - 5.0).abs() < f32::EPSILON);
        settings.circle_of_confusion.pixel_aspect = 2.0;
        settings.size_multiplier = 3.0;
        settings.proxy_scale = Some(0.5);
        assert!((settings.get_size() - 15.0).abs() < f32::EPSILON);
        settings.size_multiplier = -2.0;
        assert!(settings.get_size().abs() < f32::EPSILON);
    }

    #[test]
    /// Verifies the raw maximum comes from the camera when camera data is present and from the CoC otherwise.
    fn raw_max_size_prefers_camera_maximum_when_camera_is_set() {
        let mut settings = defocus::Settings::default();
        settings.circle_of_confusion.max_size = 12.0;
        settings.camera_max_size = 33.0;
        assert!((settings.get_raw_max_size() - 12.0).abs() < f32::EPSILON);

        settings.circle_of_confusion.camera_data = Some(circle_of_confusion::CameraData::default());
        assert!((settings.get_raw_max_size() - 33.0).abs() < f32::EPSILON);
    }

    #[test]
    /// Verifies the maximum size scales the raw maximum and that 2D and depth modes pick different radii.
    fn max_size_and_current_max_size_follow_the_mode() {
        let mut settings = defocus::Settings::default();
        settings.circle_of_confusion.max_size = 10.0;
        settings.circle_of_confusion.pixel_aspect = 2.0;
        settings.size_multiplier = 1.5;
        settings.proxy_scale = Some(0.5);
        assert!((settings.get_max_size() - 15.0).abs() < f32::EPSILON);

        settings.size_multiplier = -1.0;
        assert!(settings.get_max_size().abs() < f32::EPSILON);

        settings.size_multiplier = 1.0;
        settings.proxy_scale = None;
        settings.circle_of_confusion.pixel_aspect = 1.0;
        settings.set_defocus_mode(defocus::DefocusMode::Twod);
        assert!((settings.get_current_max_size() - 5.0).abs() < f32::EPSILON);
        settings.set_defocus_mode(defocus::DefocusMode::Depth);
        assert!((settings.get_current_max_size() - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    /// Verifies padding rounds the current maximum radius up to whole pixels.
    fn padding_rounds_the_current_radius_up() {
        let mut settings = defocus::Settings::default();
        settings.set_defocus_mode(defocus::DefocusMode::Depth);
        settings.circle_of_confusion.max_size = 10.2;
        assert_eq!(settings.get_padding(), 11);
        settings.set_defocus_mode(defocus::DefocusMode::Twod);
        settings.circle_of_confusion.size = 4.01;
        assert_eq!(settings.get_padding(), 5);
    }

    #[test]
    /// Verifies sample counts for each quality are clamped to their bounds.
    fn sample_counts_follow_quality_and_clamp() {
        let mut settings = render::Settings {
            gui: true,
            quality: render::Quality::Fast as i32,
            ..Default::default()
        };
        assert_eq!(get_samples(&settings, 10), 8);
        assert_eq!(get_samples(&settings, 100), 10);
        assert_eq!(get_samples(&settings, 1000), 20);

        settings.quality = render::Quality::Accurate as i32;
        assert_eq!(get_samples(&settings, 10), 30);
        assert_eq!(get_samples(&settings, 100), 50);
        assert_eq!(get_samples(&settings, 1000), 100);
    }

    #[test]
    /// Verifies the farm quality is used when the GUI flag is off.
    fn samples_use_farm_quality_when_not_in_gui() {
        let settings = render::Settings {
            gui: false,
            quality: render::Quality::Fast as i32,
            farm_quality: render::Quality::Accurate as i32,
            ..Default::default()
        };
        assert_eq!(get_samples(&settings, 100), 50);
    }

    #[test]
    /// Verifies the default 2D settings produce the expected global flags, sample count, and ring spacing.
    fn default_2d_conversion_sets_expected_flags_and_ring_distance() {
        let settings = Settings::default();
        let specs = render::RenderSpecs::from_rects(IVec4::ZERO, IVec4::ZERO);
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::new(256, 128), 4);
        assert_eq!(convolve.flags, 0b1_0011);
        assert_eq!(convolve.non_uniform_flags, 0);
        assert_eq!(convolve.samples, 8);
        assert!((convolve.max_size - 5.0).abs() < f32::EPSILON);
        assert!((convolve.size - 5.0).abs() < f32::EPSILON);
        assert!((convolve.ring_distance - 5.0 / 8.0).abs() < f32::EPSILON);
        assert!((convolve.filter_aspect_ratio - 2.0).abs() < f32::EPSILON);
        assert!((convolve.filter_aspect_ratio_normalizer - 0.5).abs() < f32::EPSILON);
        assert_eq!(convolve.image_elements, 4);
        assert_eq!(convolve.render_scale, 1);
    }

    #[test]
    /// Verifies depth mode uses the raw maximum radius and the pixel aspect scales only the normalizer and size.
    fn depth_conversion_uses_raw_max_and_pixel_aspect_normaliser() {
        let mut settings = Settings::default();
        settings
            .defocus
            .set_defocus_mode(defocus::DefocusMode::Depth);
        settings.defocus.circle_of_confusion.pixel_aspect = 2.0;
        let specs = render::RenderSpecs::from_rects(IVec4::ZERO, IVec4::ZERO);
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert!((convolve.max_size - 10.0).abs() < f32::EPSILON);
        assert!((convolve.size - 10.0).abs() < f32::EPSILON);
        assert!((convolve.pixel_aspect_normalizer - 0.5).abs() < f32::EPSILON);
        assert_eq!(convolve.flags & 1, 0, "IS_2D must be clear in depth mode");
    }

    #[test]
    /// Verifies each global flag is set only by its own setting.
    fn global_flags_follow_their_settings() {
        let specs = render::RenderSpecs::from_rects(IVec4::ZERO, IVec4::ZERO);
        let mut settings = Settings::default();
        settings
            .defocus
            .set_defocus_mode(defocus::DefocusMode::Depth);
        settings.render.filter.mode = render::FilterMode::Image as i32;
        settings.non_uniform.inverse_foreground = false;
        settings.render.inverse_y = true;
        settings.non_uniform.axial_aberration.enable = true;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert_eq!(convolve.flags, 0x20 | 0x40);
    }

    #[test]
    /// Verifies a non-uniform effect sets `USE_NON_UNIFORM` and its own bits, and sub-options need their effect enabled.
    fn non_uniform_flags_require_their_effect_to_be_enabled() {
        let specs = render::RenderSpecs::from_rects(IVec4::ZERO, IVec4::ZERO);
        let mut settings = Settings::default();
        settings.non_uniform.inverse_foreground = false;
        settings.non_uniform.catseye.inverse = true;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert_eq!(convolve.non_uniform_flags, 0);
        assert_eq!(convolve.flags & 0x08, 0);

        settings.non_uniform.catseye.enable = true;
        settings.non_uniform.catseye.inverse_foreground = true;
        settings.non_uniform.catseye.relative_to_screen = false;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert_eq!(convolve.non_uniform_flags, 0b0111);
        assert_eq!(convolve.flags & 0x08, 0x08);

        settings.non_uniform.catseye.enable = false;
        settings.non_uniform.astigmatism.enable = true;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert_eq!(convolve.non_uniform_flags, 0b1_0000);

        settings.non_uniform.astigmatism.enable = false;
        settings.non_uniform.barndoors.enable = true;
        settings.non_uniform.barndoors.inverse = true;
        settings.non_uniform.barndoors.inverse_foreground = false;
        let convolve = settings_to_convolve_settings(&settings, &specs, UVec2::ONE, 0);
        assert_eq!(convolve.non_uniform_flags, 0b10_0000 | 0b100_0000);
    }

    #[test]
    /// Verifies the catseye screen-relative option sets its own bit alongside the catseye enable bit.
    fn catseye_screen_relative_sets_its_flag() {
        let mut settings = non_uniform::Settings::default();
        settings.catseye.enable = true;
        settings.catseye.inverse = false;
        settings.catseye.inverse_foreground = false;
        settings.catseye.relative_to_screen = true;
        assert_eq!(non_uniform_to_flags(&settings).bits(), 0b1001);
    }
}
