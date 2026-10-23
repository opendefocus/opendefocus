//! Builds the flat sample offset table read by the adaptive GPU gather: the Fast and Accurate smooth and detail patterns, then the edge extension.
//! Offsets are float indices into this table; each sample occupies two floats.

/// Sample count of the Fast smooth pattern.
pub(crate) const FAST_SMOOTH_POINTS: u32 = 64;
/// Sample count of the Fast detail pattern.
pub(crate) const FAST_DETAIL_POINTS: u32 = 256;
/// Sample count of the Accurate smooth pattern.
pub(crate) const ACCURATE_SMOOTH_POINTS: u32 = 128;
/// Sample count of the Accurate detail pattern.
pub(crate) const ACCURATE_DETAIL_POINTS: u32 = 320;
/// Sample count of the edge extension pattern, which no budget reads directly.
const EDGE_POINTS: u32 = 192;

/// Float index of the Fast smooth pattern.
pub(crate) const FAST_SMOOTH_OFFSET: u32 = 0;
/// Float index of the Fast detail pattern, directly after the Fast smooth pattern.
pub(crate) const FAST_DETAIL_OFFSET: u32 = FAST_SMOOTH_OFFSET + FAST_SMOOTH_POINTS * 2;
/// Float index of the Accurate smooth pattern, directly after the Fast detail pattern.
pub(crate) const ACCURATE_SMOOTH_OFFSET: u32 = FAST_DETAIL_OFFSET + FAST_DETAIL_POINTS * 2;
/// Float index of the Accurate detail pattern, directly after the Accurate smooth pattern.
pub(crate) const ACCURATE_DETAIL_OFFSET: u32 = ACCURATE_SMOOTH_OFFSET + ACCURATE_SMOOTH_POINTS * 2;
/// Float index of the edge extension, directly after the Accurate detail pattern.
const EDGE_OFFSET: u32 = ACCURATE_DETAIL_OFFSET + ACCURATE_DETAIL_POINTS * 2;

/// Golden angle in radians, spacing successive pattern points around the circle.
const GOLDEN_ANGLE: f32 = 2.399_963_1_f32;
/// Angle index of the edge extension, offset so its points do not repeat the main patterns' angles.
const EDGE_ANGLE_BASE: u32 = 320;

/// Appends `count` points of a sunflower spiral whose angle index starts at `angle_base`.
fn push_spiral(offsets: &mut Vec<f32>, count: u32, angle_base: u32) {
    for index in 0..count {
        let radius = ((index as f32 + 0.5) / count as f32).sqrt();
        let angle = (index + angle_base) as f32 * GOLDEN_ANGLE;
        offsets.extend([radius * angle.cos(), radius * angle.sin()]);
    }
}

/// Generate the circular sample patterns consumed by the GPU gather, in the order of their float offset constants.
pub(crate) fn adaptive_gather_sample_offsets() -> Vec<f32> {
    let mut offsets = Vec::with_capacity((EDGE_OFFSET + EDGE_POINTS * 2) as usize);
    for count in [
        FAST_SMOOTH_POINTS,
        FAST_DETAIL_POINTS,
        ACCURATE_SMOOTH_POINTS,
        ACCURATE_DETAIL_POINTS,
    ] {
        push_spiral(&mut offsets, count, 0);
    }
    push_spiral(&mut offsets, EDGE_POINTS, EDGE_ANGLE_BASE);
    offsets
}
