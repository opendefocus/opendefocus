//! Tests for the block planner: thin frames, texture limits, ROI halos and storage budgets.
use super::blocks::{align_down, max_axis_context, plan_adaptive_blocks};
use super::*;
use crate::datamodel::IVector4;
use crate::error::Error;

#[test]
/// Verifies block planning for thin frames stays within their bounds.
fn block_planner_uses_thin_frame_bounds() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 20_000,
        w: 200,
    };
    let plan = plan_adaptive_blocks(&full, &full, 1_016, CPU_REFERENCE_FRAME_PIXELS, usize::MAX)
        .expect("thin panoramic frame should fit by using a rectangular core");

    assert!(!plan.single_pass);
    let source_width = max_axis_context(full.x, full.z, full.x, full.z, plan.core_width, 1_016);
    let source_height = max_axis_context(full.y, full.w, full.y, full.w, plan.core_height, 1_016);
    assert!(source_width as usize * source_height as usize <= CPU_REFERENCE_FRAME_PIXELS);
    assert_eq!(plan.core_height, 208);
}

#[test]
/// Verifies block planning respects the texture dimension limit even when the pixel count fits.
fn block_planner_respects_texture_dimensions_even_when_pixels_fit() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 20_000,
        w: 64,
    };
    let plan = plan_adaptive_blocks(&full, &full, 48, CPU_REFERENCE_FRAME_PIXELS, 8_192)
        .expect("wide frames need multiple texture-sized blocks");
    assert!(!plan.single_pass);
    assert!(max_axis_context(0, 20_000, 0, 20_000, plan.core_width, 48) <= 8_192);
    assert!(max_axis_context(0, 64, 0, 64, plan.core_height, 48) <= 8_192);
}

#[test]
/// Verifies the halo is clipped at the frame edges for a small ROI.
fn block_planner_clips_halo_at_frame_edges_for_small_roi() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 4_000,
        w: 4_000,
    };
    let requested = IVector4 {
        x: 0,
        y: 0,
        z: 1,
        w: 1,
    };
    let plan = plan_adaptive_blocks(
        &full,
        &requested,
        816,
        CPU_REFERENCE_FRAME_PIXELS,
        usize::MAX,
    )
    .expect("edge ROI needs only the available frame-side halo");

    let source_width = max_axis_context(
        full.x,
        full.z,
        requested.x,
        requested.z,
        plan.core_width,
        816,
    );
    let source_height = max_axis_context(
        full.y,
        full.w,
        requested.y,
        requested.w,
        plan.core_height,
        816,
    );
    assert!(source_width as usize * source_height as usize <= CPU_REFERENCE_FRAME_PIXELS);
}

#[test]
/// Verifies an error is reported when an interior halo exceeds the storage budget.
fn block_planner_reports_when_interior_halo_exceeds_storage_budget() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 4_000,
        w: 4_000,
    };
    let requested = IVector4 {
        x: 2_000,
        y: 2_000,
        z: 2_001,
        w: 2_001,
    };

    assert!(matches!(
        plan_adaptive_blocks(
            &full,
            &requested,
            816,
            CPU_REFERENCE_FRAME_PIXELS,
            usize::MAX
        ),
        Err(Error::SizeLimitExceeded)
    ));
}

#[test]
/// Verifies alignment rounds toward negative infinity and leaves values unchanged for the `i32::MAX` sentinel.
fn align_down_rounds_toward_negative_infinity_and_keeps_sentinel() {
    assert_eq!(align_down(37, 16), 32);
    assert_eq!(align_down(-1, 16), -16);
    assert_eq!(align_down(32, 16), 32);
    assert_eq!(align_down(37, i32::MAX), 37);
}

#[test]
/// Verifies a frame that fits one pass reports a single pass with unbounded cores.
fn block_planner_reports_single_pass_when_frame_fits() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 64,
        w: 64,
    };
    let plan = plan_adaptive_blocks(&full, &full, 8, 4_096, 64).unwrap();
    assert!(plan.single_pass);
    assert_eq!((plan.core_width, plan.core_height), (i32::MAX, i32::MAX));
}

#[test]
/// Verifies degenerate planner inputs are rejected before any block is sized.
fn block_planner_rejects_degenerate_inputs() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 1_000,
        w: 1_000,
    };
    let empty = IVector4 {
        x: 5,
        y: 0,
        z: 5,
        w: 1_000,
    };
    assert!(matches!(
        plan_adaptive_blocks(&full, &full, 8, 0, 64),
        Err(Error::InvalidParameter)
    ));
    assert!(matches!(
        plan_adaptive_blocks(&full, &full, -1, 4_096, 64),
        Err(Error::InvalidParameter)
    ));
    assert!(matches!(
        plan_adaptive_blocks(&empty, &full, 8, 4_096, 64),
        Err(Error::InvalidDimensions)
    ));
    assert!(matches!(
        plan_adaptive_blocks(&full, &empty, 8, 4_096, 64),
        Err(Error::InvalidDimensions)
    ));
}

#[test]
/// Verifies a halo wider than the budget allows at every core size fails instead of planning an oversized block.
fn block_planner_fails_when_no_core_fits_the_pixel_budget() {
    let full = IVector4 {
        x: 0,
        y: 0,
        z: 1_000,
        w: 1_000,
    };
    assert!(matches!(
        plan_adaptive_blocks(&full, &full, 1_000, 100, usize::MAX),
        Err(Error::SizeLimitExceeded)
    ));
}

#[test]
/// Verifies the largest context over a request is the core plus halo on both sides, clipped at the frame.
fn max_axis_context_adds_halo_and_clips_to_frame() {
    assert_eq!(max_axis_context(0, 100, 0, 100, 16, 4), 24);
    assert_eq!(max_axis_context(0, 100, 0, 10, 16, 4), 14);
    assert_eq!(max_axis_context(0, 100, 0, 100, 16, 0), 16);
    assert_eq!(max_axis_context(0, 8, 0, 8, 16, 4), 8);
}
