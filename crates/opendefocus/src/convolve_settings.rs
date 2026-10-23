//! GPU-facing convolution uniform (`ConvolveSettings`), its global and non-uniform flag sets,
//! and the sample-weight helpers. The struct layout must match the WGSL uniform; a size test guards it.
use bitflags::bitflags;
use bytemuck::{Pod, Zeroable};
use core::f32::consts::PI;
use glam::{IVec4, UVec2, Vec2};
/// Upper bound on one sample weight, 2/pi. `compute_sample_weight` never returns more than this.
const LARGEST_WEIGHT: f32 = 1.0 / (PI * 0.5);

/// Base number of sample points added to each ring when the base pattern is used.
const BASE_SAMPLES: u32 = 20;

#[must_use]
#[inline]
/// Calculate the base number of points to distribute along a ring
fn get_points_for_ring(ring_id: u32, samples: u32, use_base_points: bool) -> u32 {
    if ring_id == 0 {
        return 1;
    }
    let mut base_points = 0;
    if use_base_points {
        base_points += BASE_SAMPLES;
    }

    base_points += ring_id * 2;
    ((samples as f32) * 0.1 * (base_points as f32)) as u32
}

bitflags! {
    /// These are the most commonly used flags, to reduce memory reads they are together
    #[derive(Debug, Clone, Copy, Zeroable, Pod, PartialEq)]
    #[repr(C)]
    pub struct GlobalFlags: u32 {
        /// Processing in 2d or using depth map
        const IS_2D                                         = 0b00000001;
        /// Use a simple disc, this prevents reads to the filter kernel
        const SIMPLE_DISC                                   = 0b00000010;
        /// Merge low defocus size over the original image
        const HIGH_RESOLUTION_INTERPOLATION                 = 0b00000100;
        /// Use non uniform bokeh calculations
        const USE_NON_UNIFORM                               = 0b00001000;
        /// Inverse the bokeh shape in the foreground
        const INVERSE_FOREGROUND_BOKEH_SHAPE                = 0b00010000;
        /// Enable offset size of the filter kernel per channel
        const AXIAL_ABERRATION_ENABLE                       = 0b00100000;
        /// Interpret the y from bottom to top
        const INVERSE_Y                                     = 0b01000000;

    }

    #[derive(Debug, Clone, Copy, Zeroable, Pod, PartialEq)]
    #[repr(C)]
    /// Switches for the non-uniform effects (catseye, astigmatism and barndoors), stored in one word.
    pub struct NonUniformFlags: u32 {
        /// Use Catseye non uniform calculations
        const CATSEYE_ENABLED                               = 0b00000001;
        /// Flag indicating whether to invert catseye for foreground
        const CATSEYE_INVERSE                               = 0b00000010;
        /// Flag indicating whether to invert catseye for foreground
        const CATSEYE_INVERSE_FOREGROUND                    = 0b00000100;
        /// Flag indicating whether catseye is screen relative
        const CATSEYE_SCREEN_RELATIVE                       = 0b00001000;
        /// Enable the astigmatism artifact
        const ASTIGMATISM_ENABLED                           = 0b00010000;
        /// Enable the barndoors cutting of bokehs
        const BARNDOORS_ENABLED                             = 0b00100000;
        /// Inverse the entirity of barndoors
        const BARNDOORS_INVERSE                             = 0b01000000;
        /// Inverse barndoors only in foreground
        const BARNDOORS_INVERSE_FOREGROUND                  = 0b10000000;
    }

}

/// Artifact that is caused by the light rays entering the lens at a different
/// rate at different focal distances.
pub struct AxialAberration {
    /// Flag indicating whether axial aberration is enabled
    pub enable: bool,
    /// Amount of the axial aberration effect
    pub amount: f32,
    /// Color type for the aberration
    pub color_type: AxialAberrationType,
}

#[repr(u32)]
/// Colour pairing used by axial aberration. The discriminant is the value stored in `ConvolveSettings::axial_aberration_type`.
pub enum AxialAberrationType {
    /// Red and blue color combination
    RedBlue = 1,
    /// Blue and yellow color combination
    BlueYellow = 2,
    /// Green and purple color combination
    GreenPurple = 3,
}

/// Settings structure for convolution operations.
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
#[repr(C, align(16))]
pub struct ConvolveSettings {
    /// Region to process as (x0, y0, x1, y1) in full-frame pixels; only this region is gathered
    pub process_region: IVec4,
    /// Full frame as (x0, y0, x1, y1) in full-frame pixels
    pub full_region: IVec4,
    /// Resolution of the render (width, height)
    pub resolution: UVec2,
    /// Resolution of the filter (width, height)
    pub filter_resolution: UVec2,
    /// Center point of the convolution
    pub center: Vec2,
    /// Global operation flags
    pub flags: u32,
    /// Bit set of non-uniform effect flags
    pub non_uniform_flags: u32,
    /// Number of samples to use for convolution
    pub samples: u32,
    /// Pixel aspect ratio
    pub pixel_aspect: f32,
    /// Base size for convolution
    pub size: f32,
    /// Maximum size for convolution
    pub max_size: f32,
    /// Amount of catseye effect
    pub catseye_amount: f32,
    /// Gamma correction for catseye
    pub catseye_gamma: f32,
    /// Softness of catseye edges
    pub catseye_softness: f32,
    /// Amount of astigmatism
    pub astigmatism_amount: f32,
    /// Gamma correction for astigmatism
    pub astigmatism_gamma: f32,
    /// Amount of barndoors effect
    pub barndoors_amount: f32,
    /// Gamma correction for barndoors
    pub barndoors_gamma: f32,
    /// Top position of barndoors
    pub barndoors_top: f32,
    /// Bottom position of barndoors
    pub barndoors_bottom: f32,
    /// Left position of barndoors
    pub barndoors_left: f32,
    /// Right position of barndoors
    pub barndoors_right: f32,
    /// Normalization factor for filter aspect ratio
    pub filter_aspect_ratio_normalizer: f32,
    /// Number of image elements supplied by the caller
    pub image_elements: u32,
    /// Distance between rings in the filter
    pub ring_distance: f32,
    /// Aspect ratio of the filter
    pub filter_aspect_ratio: f32,
    /// Normalization factor for pixel aspect
    pub pixel_aspect_normalizer: f32,
    /// Render scale factor
    pub render_scale: u32,
    /// Amount of axial aberration applied to the colour channels
    pub axial_aberration_amount: f32,
    /// Colour pairing: 1 is red-blue, 2 is blue-yellow and 3 is green-purple. Other values are read as red-blue.
    pub axial_aberration_type: i32,

    #[allow(
        clippy::pub_underscore_fields,
        reason = "explicit GPU uniform padding that must stay public to keep the layout"
    )]
    /// Explicit padding that keeps the struct at the 160-byte size the WGSL uniform expects.
    pub _padding: u32,
}
impl ConvolveSettings {
    /// Returns the weight of one sample for a CoC radius in pixels.
    /// The ring point counts are summed until the radius is covered, and the result is `1 / total_points`, capped at `LARGEST_WEIGHT`.
    pub fn compute_sample_weight(&self, coc: f32) -> f32 {
        let mut total_points = 1;
        let mut ring_id = 0;
        let mut remaining_coc = coc;

        while remaining_coc > 0.0 {
            total_points += get_points_for_ring(ring_id, self.samples, true);
            remaining_coc -= self.ring_distance;
            ring_id += 1;
        }
        (1.0 / (total_points as f32)).min(LARGEST_WEIGHT)
    }

    /// Precomputes `compute_sample_weight` for every whole CoC radius from 0 up to `max_size`, rounded up.
    /// Entries beyond that range stay zero.
    pub fn get_sample_weights(&self) -> [f32; 2048] {
        let mut weights = [0.0; 2048];
        for coc in 0..=(self.max_size.ceil() as i32) {
            weights[coc as usize] = self.compute_sample_weight(coc as f32);
        }
        weights
    }

    /// Returns the full frame size in pixels, computed as `full_region` (z - x) by (w - y).
    pub fn get_image_resolution(&self) -> UVec2 {
        UVec2::new(
            (self.full_region.z - self.full_region.x) as u32,
            (self.full_region.w - self.full_region.y) as u32,
        )
    }

    /// Get the coordinates in screen-space based on the full region
    pub fn get_real_coordinates(&self, coordinates: UVec2) -> UVec2 {
        UVec2::new(
            (self.full_region.x as u32) + coordinates.x,
            (self.full_region.y as u32) + coordinates.y,
        )
    }

    /// Decodes the axial aberration fields. `enable` is read from the `AXIAL_ABERRATION_ENABLE` bit of `flags`,
    /// and unknown colour types map to red-blue.
    pub fn get_axial_aberration_settings(&self) -> AxialAberration {
        AxialAberration {
            enable: GlobalFlags::from_bits_retain(self.flags)
                .contains(GlobalFlags::AXIAL_ABERRATION_ENABLE),
            amount: self.axial_aberration_amount,
            color_type: match self.axial_aberration_type {
                2 => AxialAberrationType::BlueYellow,
                3 => AxialAberrationType::GreenPurple,
                _ => AxialAberrationType::RedBlue,
            },
        }
    }
}

#[cfg(test)]
/// Unit tests for the convolution uniform layout.
mod tests {
    use super::*;

    /// Struct size must stay aligned with the WGSL uniform layout
    #[test]
    fn test_struct_size() {
        assert_eq!(core::mem::size_of::<ConvolveSettings>(), 160);
    }

    #[test]
    /// Verifies ring point counts: the centre is one point and later rings scale with samples and base points.
    fn ring_point_counts_scale_with_samples_and_base_points() {
        assert_eq!(get_points_for_ring(0, 50, true), 1);
        assert_eq!(get_points_for_ring(0, 50, false), 1);
        assert_eq!(get_points_for_ring(1, 50, true), 110);
        assert_eq!(get_points_for_ring(2, 50, false), 20);
    }

    #[test]
    /// Verifies the sample weight steps down when a ring is added, with the zero-radius weight capped at the largest weight.
    fn sample_weight_counts_rings_covering_the_radius() {
        let settings = ConvolveSettings {
            samples: 50,
            ring_distance: 10.0,
            ..Default::default()
        };
        assert!((settings.compute_sample_weight(0.0) - LARGEST_WEIGHT).abs() < f32::EPSILON);
        assert!((settings.compute_sample_weight(5.0) - 0.5).abs() < f32::EPSILON);
        assert!((settings.compute_sample_weight(10.0) - 0.5).abs() < f32::EPSILON);
        assert!((settings.compute_sample_weight(10.5) - 1.0 / 112.0).abs() < 1e-7);
    }

    #[test]
    /// Verifies the precomputed table covers whole radii up to the rounded-up max size and leaves the rest zero.
    fn sample_weight_table_covers_rounded_up_max_size() {
        let settings = ConvolveSettings {
            samples: 50,
            ring_distance: 10.0,
            max_size: 10.2,
            ..Default::default()
        };
        let weights = settings.get_sample_weights();
        assert!((weights[0] - LARGEST_WEIGHT).abs() < f32::EPSILON);
        assert!((weights[5] - 0.5).abs() < f32::EPSILON);
        assert!((weights[11] - 1.0 / 112.0).abs() < 1e-7);
        assert!(weights[12].abs() < f32::EPSILON);
        assert_eq!(weights.len(), 2048);
    }

    #[test]
    /// Verifies the image size and coordinate mapping use the full region origin.
    fn image_size_and_coordinates_use_full_region_origin() {
        let settings = ConvolveSettings {
            full_region: IVec4::new(100, 200, 2020, 1280),
            ..Default::default()
        };
        assert_eq!(settings.get_image_resolution(), UVec2::new(1920, 1080));
        assert_eq!(
            settings.get_real_coordinates(UVec2::new(3, 4)),
            UVec2::new(103, 204)
        );
    }

    #[test]
    /// Verifies axial aberration decoding reads the enable bit from flags and maps unknown colour types to red-blue.
    fn axial_aberration_decodes_enable_bit_and_colour_type() {
        let mut settings = ConvolveSettings {
            flags: GlobalFlags::AXIAL_ABERRATION_ENABLE.bits(),
            axial_aberration_amount: 0.75,
            axial_aberration_type: 2,
            ..Default::default()
        };
        let decoded = settings.get_axial_aberration_settings();
        assert!(decoded.enable);
        assert!((decoded.amount - 0.75).abs() < f32::EPSILON);
        assert!(matches!(
            decoded.color_type,
            AxialAberrationType::BlueYellow
        ));

        settings.axial_aberration_type = 3;
        assert!(matches!(
            settings.get_axial_aberration_settings().color_type,
            AxialAberrationType::GreenPurple
        ));
        settings.axial_aberration_type = 1;
        assert!(matches!(
            settings.get_axial_aberration_settings().color_type,
            AxialAberrationType::RedBlue
        ));
        settings.axial_aberration_type = 99;
        assert!(matches!(
            settings.get_axial_aberration_settings().color_type,
            AxialAberrationType::RedBlue
        ));

        settings.flags = GlobalFlags::IS_2D.bits();
        assert!(!settings.get_axial_aberration_settings().enable);
    }
}
