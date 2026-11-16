//! Lens flags that select a specialized exact gather pipeline.
use crate::convolve_settings::{ConvolveSettings, GlobalFlags, NonUniformFlags};

/// Global flags that switch a whole lens feature; `SPECIALIZED_LENS_BITS` in the gather shader.
const LENS_BITS: GlobalFlags = GlobalFlags::SIMPLE_DISC
    .union(GlobalFlags::USE_NON_UNIFORM)
    .union(GlobalFlags::AXIAL_ABERRATION_ENABLE);
/// Non-uniform flags that switch a whole effect; `SPECIALIZED_NON_UNIFORM_BITS` in the gather shader.
const NON_UNIFORM_BITS: NonUniformFlags = NonUniformFlags::CATSEYE_ENABLED
    .union(NonUniformFlags::ASTIGMATISM_ENABLED)
    .union(NonUniformFlags::BARNDOORS_ENABLED);

/// Lens flags compiled into an exact gather pipeline as constants. The gather shader serves every aperture and lens
/// effect; with the feature switches known at pipeline creation the driver removes the paths of the features that are
/// off. Only those switches are specialized, so options within a feature do not compile another pipeline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(in crate::runners::wgpu) struct LensSpecialization {
    /// Feature switches of `ConvolveSettings::flags`.
    pub(super) flags: u32,
    /// Feature switches of `ConvolveSettings::non_uniform_flags`.
    pub(super) non_uniform_flags: u32,
}

impl LensSpecialization {
    /// Returns the specialization for the lens uniform the gather binds.
    pub(in crate::runners::wgpu) fn of(lens: &ConvolveSettings) -> Self {
        Self {
            flags: lens.flags & LENS_BITS.bits(),
            non_uniform_flags: lens.non_uniform_flags & NON_UNIFORM_BITS.bits(),
        }
    }
}

#[cfg(test)]
/// Tests for the specialization key.
mod tests {
    use super::*;

    #[test]
    /// Verifies the key keeps the feature switches, with the bit values the shader masks, and drops the options.
    fn key_keeps_only_feature_switches() {
        assert_eq!(LENS_BITS.bits(), 0x2a);
        assert_eq!(NON_UNIFORM_BITS.bits(), 0x31);
        let lens = ConvolveSettings {
            flags: (GlobalFlags::SIMPLE_DISC | GlobalFlags::INVERSE_Y).bits(),
            non_uniform_flags: (NonUniformFlags::CATSEYE_ENABLED
                | NonUniformFlags::CATSEYE_INVERSE)
                .bits(),
            ..Default::default()
        };
        assert_eq!(
            LensSpecialization::of(&lens),
            LensSpecialization {
                flags: GlobalFlags::SIMPLE_DISC.bits(),
                non_uniform_flags: NonUniformFlags::CATSEYE_ENABLED.bits(),
            }
        );
    }
}
