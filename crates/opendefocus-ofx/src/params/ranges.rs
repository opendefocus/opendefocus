//! Slider display ranges for parameters whose Nuke knobs had a narrower range than the hard limits.
//! Only the slider changes; the accepted range of each parameter is still defined in the parameter table.

/// Slider ranges (kOfxParamPropDisplayMin/Max) taken from the Nuke knobs
/// (`git show 4a36b73^:crates/opendefocus-nuke/src/consts.rs`, `with_range`). The hard range of
/// each row stays wider, so values outside the slider are still accepted. Parameters without a
/// Nuke range are not listed and keep their hard range as the slider.
pub const DISPLAY_RANGES: &[(&str, f64, f64)] = &[
    ("size", 0.0, 100.0),
    ("maxSize", 0.0, 100.0),
    ("cameraMaxSize", 0.0, 100.0),
    ("gamma", 0.2, 5.0),
    ("sizeMultiplier", 0.0, 2.0),
    ("bokehRingColor", 0.0, 1.0),
    ("bokehInnerColor", 0.001, 1.0),
    ("bokehBlades", 3.0, 16.0),
    ("bokehAngle", -180.0, 180.0),
    ("bokehAspectRatio", 0.0, 2.0),
    ("catseyeAmount", 0.0, 1.0),
    ("catseyeGamma", 0.2, 4.0),
    ("catseyeSoftness", 0.01, 1.0),
    ("barndoorsGamma", 0.2, 4.0),
    ("astigmatismGamma", 0.2, 4.0),
    // Nuke's range starts at -1, but the renderer rejects a negative amount, so the slider
    // starts at 0. The hard range accepts the renderer's values up to 10.
    ("axialAberrationAmount", -1.0, 1.0),
];

/// The display range listed for a parameter, if the Nuke plugin had one.
pub(super) fn display_range(name: &str) -> Option<(f64, f64)> {
    DISPLAY_RANGES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|&(_, min, max)| (min, max))
}
