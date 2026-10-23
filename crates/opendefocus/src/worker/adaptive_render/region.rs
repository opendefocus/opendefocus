//! Rectangle maths for the adaptive renderer: region sizes, clamped growth, intersections and window copies between frames.
use crate::{
    datamodel::IVector4,
    error::{Error, Result},
};
use ndarray::{ArrayView3, ArrayViewMut3, s};
use std::ops::Range;

/// Returns the width and height of `region`.
///
/// # Errors
/// Returns `InvalidDimensions` when the region is empty or inverted on either axis.
pub(super) fn region_size(region: &IVector4) -> Result<(usize, usize)> {
    let width = region
        .z
        .checked_sub(region.x)
        .filter(|value| *value > 0)
        .ok_or(Error::InvalidDimensions)?;
    let height = region
        .w
        .checked_sub(region.y)
        .filter(|value| *value > 0)
        .ok_or(Error::InvalidDimensions)?;
    Ok((width as usize, height as usize))
}

/// Returns the top-left corner of `region` as `[x, y]`.
pub(super) fn origin(region: &IVector4) -> [i32; 2] {
    [region.x, region.y]
}

/// Grows `region` by `halo` pixels on every side without leaving `bounds`.
pub(super) fn grow_clamped(region: &IVector4, halo: i32, bounds: &IVector4) -> IVector4 {
    IVector4 {
        x: region.x.saturating_sub(halo).max(bounds.x),
        y: region.y.saturating_sub(halo).max(bounds.y),
        z: region.z.saturating_add(halo).min(bounds.z),
        w: region.w.saturating_add(halo).min(bounds.w),
    }
}

/// Returns the rectangle covered by both regions, or `None` when they do not overlap.
pub(super) fn intersection(a: &IVector4, b: &IVector4) -> Option<IVector4> {
    let overlap = IVector4 {
        x: a.x.max(b.x),
        y: a.y.max(b.y),
        z: a.z.min(b.z),
        w: a.w.min(b.w),
    };
    (overlap.x < overlap.z && overlap.y < overlap.w).then_some(overlap)
}

/// Returns true when the pixel at `(x, y)` lies inside `region`.
pub(super) fn contains(region: &IVector4, x: i32, y: i32) -> bool {
    x >= region.x && x < region.z && y >= region.y && y < region.w
}

/// Returns the row and column ranges `region` covers inside a frame whose top-left corner is at `frame_origin`.
pub(super) fn window(region: &IVector4, frame_origin: [i32; 2]) -> (Range<usize>, Range<usize>) {
    let [origin_x, origin_y] = frame_origin;
    (
        (region.y - origin_y) as usize..(region.w - origin_y) as usize,
        (region.x - origin_x) as usize..(region.z - origin_x) as usize,
    )
}

/// Copies the first `channels` channels of `region` from `source` into `destination`.
/// Both frames are addressed in the same global coordinates; each is described by the corner it starts at.
///
/// # Panics
/// Panics when `region` is not inside both frames or `channels` exceeds either frame's channel count.
pub(super) fn copy_region(
    destination: &mut ArrayViewMut3<'_, f32>,
    destination_origin: [i32; 2],
    source: &ArrayView3<'_, f32>,
    source_origin: [i32; 2],
    region: &IVector4,
    channels: usize,
) {
    let (destination_rows, destination_columns) = window(region, destination_origin);
    let (source_rows, source_columns) = window(region, source_origin);
    destination
        .slice_mut(s![destination_rows, destination_columns, ..channels])
        .assign(&source.slice(s![source_rows, source_columns, ..channels]));
}

#[cfg(test)]
/// Tests for the rectangle helpers and the window copy.
mod tests {
    use super::*;
    use ndarray::Array3;

    /// Builds a region from its corners.
    fn region(x: i32, y: i32, z: i32, w: i32) -> IVector4 {
        IVector4 { x, y, z, w }
    }

    #[test]
    /// Verifies sizes of valid regions and rejection of empty or inverted ones.
    fn region_size_rejects_empty_and_inverted_regions() {
        assert_eq!(region_size(&region(2, 3, 12, 8)).unwrap(), (10, 5));
        for bad in [region(0, 0, 0, 4), region(0, 0, 4, 0), region(5, 0, 1, 4)] {
            assert!(matches!(region_size(&bad), Err(Error::InvalidDimensions)));
        }
        assert!(matches!(
            region_size(&region(i32::MIN, 0, i32::MAX, 1)),
            Err(Error::InvalidDimensions)
        ));
    }

    #[test]
    /// Verifies growth stops at the bounds and does not overflow near the integer limits.
    fn grow_clamped_stays_inside_bounds() {
        let bounds = region(0, 0, 100, 50);
        assert_eq!(
            grow_clamped(&region(10, 10, 20, 20), 7, &bounds),
            region(3, 3, 27, 27)
        );
        assert_eq!(
            grow_clamped(&region(2, 1, 99, 49), 7, &bounds),
            region(0, 0, 100, 50)
        );
        assert_eq!(
            grow_clamped(&region(0, 0, 10, 10), i32::MAX, &bounds),
            bounds
        );
    }

    #[test]
    /// Verifies overlapping, touching and disjoint regions.
    fn intersection_requires_a_shared_area() {
        let a = region(0, 0, 10, 10);
        assert_eq!(
            intersection(&a, &region(5, 6, 20, 20)),
            Some(region(5, 6, 10, 10))
        );
        assert_eq!(intersection(&a, &region(10, 0, 20, 10)), None);
        assert_eq!(intersection(&a, &region(0, 12, 10, 20)), None);
    }

    #[test]
    /// Verifies the end of a region is exclusive.
    fn contains_excludes_the_far_edges() {
        let r = region(2, 3, 5, 6);
        assert!(contains(&r, 2, 3) && contains(&r, 4, 5));
        assert!(!contains(&r, 5, 3) && !contains(&r, 2, 6) && !contains(&r, 1, 3));
    }

    #[test]
    /// Verifies the copy moves only the requested window and channels between frames with different origins.
    fn copy_region_moves_the_window_and_leaves_the_rest() {
        let source = Array3::from_shape_fn((6, 6, 4), |(y, x, c)| (y * 100 + x * 10 + c) as f32);
        let mut destination = Array3::<f32>::zeros((4, 4, 4));
        copy_region(
            &mut destination.view_mut(),
            [10, 20],
            &source.view(),
            [8, 19],
            &region(11, 21, 13, 23),
            3,
        );
        for y in 0..4 {
            for x in 0..4 {
                for c in 0..4 {
                    let inside = (1..3).contains(&y) && (1..3).contains(&x) && c < 3;
                    let expected = if inside {
                        source[[y + 1, x + 2, c]]
                    } else {
                        0.0
                    };
                    assert_eq!(destination[[y, x, c]], expected, "({y}, {x}, {c})");
                }
            }
        }
    }
}
