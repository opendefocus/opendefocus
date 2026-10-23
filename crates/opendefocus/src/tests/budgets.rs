//! Checks the Fast and Accurate sample budgets and that their offsets address the matching patterns in the gather table.
use super::*;
use crate::datamodel::render::Quality;
use crate::kernel::adaptive_gather_sample_offsets;

#[test]
/// Verifies each render quality maps to its fixed sample budget and table offsets.
fn adaptive_sample_budgets_follow_render_quality() {
    assert_eq!(
        AdaptiveSampleBudget::for_quality(Quality::Fast).kernel_params(),
        [64, 256, 0, 128]
    );
    assert_eq!(
        AdaptiveSampleBudget::for_quality(Quality::Accurate).kernel_params(),
        [128, 320, 640, 896]
    );
}

/// Returns sample `index` of a `count`-point sunflower pattern whose angle index starts at `angle_base`, computed independently of the table generator.
fn expected_sample(count: u32, index: u32, angle_base: u32) -> [f32; 2] {
    let radius = ((index as f32 + 0.5) / count as f32).sqrt();
    let angle = (index + angle_base) as f32 * 2.399_963_1_f32;
    [radius * angle.cos(), radius * angle.sin()]
}

/// Asserts that the table holds the `count`-point pattern whose first float is at `offset`.
fn assert_pattern_at(offsets: &[f32], offset: u32, count: u32) {
    let start = offset as usize;
    assert!(start + count as usize * 2 <= offsets.len());
    for index in 0..count {
        let at = start + index as usize * 2;
        assert_eq!(
            [offsets[at], offsets[at + 1]],
            expected_sample(count, index, 0),
            "pattern of {count} points at float {offset}, sample {index}"
        );
    }
}

#[test]
/// Verifies each budget's smooth and detail offsets address the sunflower pattern of its own sample count.
fn adaptive_sample_offsets_point_each_budget_at_its_pattern() {
    let offsets = adaptive_gather_sample_offsets();
    for quality in [Quality::Fast, Quality::Accurate] {
        let [smooth, detail, smooth_offset, detail_offset] =
            AdaptiveSampleBudget::for_quality(quality).kernel_params();
        assert_pattern_at(&offsets, smooth_offset, smooth);
        assert_pattern_at(&offsets, detail_offset, detail);
    }
}

#[test]
/// Verifies the table holds exactly the Fast, Accurate and edge patterns, with no gaps or extra patterns.
fn adaptive_sample_offset_table_holds_only_the_used_patterns() {
    let offsets = adaptive_gather_sample_offsets();
    assert_eq!(offsets.len(), (64 + 256 + 128 + 320 + 192) * 2);
    let [_, _, fast_smooth, fast_detail] =
        AdaptiveSampleBudget::for_quality(Quality::Fast).kernel_params();
    let [_, _, accurate_smooth, accurate_detail] =
        AdaptiveSampleBudget::for_quality(Quality::Accurate).kernel_params();
    assert_eq!(fast_detail, fast_smooth + 64 * 2);
    assert_eq!(accurate_smooth, fast_detail + 256 * 2);
    assert_eq!(accurate_detail, accurate_smooth + 128 * 2);
}
