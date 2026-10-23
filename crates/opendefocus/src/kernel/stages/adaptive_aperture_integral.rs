//! Host-side aperture bounds uploaded for exact WGPU gather queries.
use glam::{UVec2, Vec4};

use std::vec::Vec;

/// RGBA range over one image block. Rust and WGSL both store two aligned vec4s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C, align(16))]
pub struct ApertureBounds {
    /// Lower RGBA corner of the range.
    pub min: Vec4,
    /// Upper RGBA corner of the range.
    pub max: Vec4,
}

/// One caller-prepared aperture mip level, row-major, in the same linear color
/// space as the texture used by the gather.
pub struct ApertureMip<'a> {
    /// Size of this mip in pixels, as (width, height).
    pub resolution: UVec2,
    /// Row-major RGBA texels of this mip.
    pub pixels: &'a [Vec4],
}

/// Maximum number of mip levels one layout table entry can describe.
const APERTURE_LAYOUT_LEVELS: usize = 16;

/// Merges the up to four child bounds of the next-coarser texel `(x, y)`; children outside `width` by `height` are skipped.
fn merge_children(
    current: &[ApertureBounds],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
) -> ApertureBounds {
    let mut bounds = ApertureBounds {
        min: Vec4::splat(f32::MAX),
        max: Vec4::splat(-f32::MAX),
    };
    for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let sx = x * 2 + ox;
        let sy = y * 2 + oy;
        if sx < width && sy < height {
            let child = current[(sy * width + sx) as usize];
            bounds.min = bounds.min.min(child.min);
            bounds.max = bounds.max.max(child.max);
        }
    }
    bounds
}

/// Build the GPU layout table appended after the aperture bounds payload.
/// Each record stores the absolute payload offset, row width, spatial scale,
/// and level count as bit-cast u32 values in the existing 32-byte ABI.
///
/// # Panics
/// Panics when a mip has a zero-sized dimension or more than 16 levels.
pub fn build_aperture_bounds_layout(mips: &[ApertureMip<'_>]) -> Vec<ApertureBounds> {
    let mut output = Vec::with_capacity(mips.len() * APERTURE_LAYOUT_LEVELS);
    let mut mip_offset = 0u32;
    for mip in mips {
        assert!(mip.resolution.x > 0 && mip.resolution.y > 0);
        let mut width = mip.resolution.x;
        let mut height = mip.resolution.y;
        let mut levels = 1u32;
        let mut w = width;
        let mut h = height;
        while w > 1 || h > 1 {
            w = w.div_ceil(2).max(1);
            h = h.div_ceil(2).max(1);
            levels += 1;
        }
        assert!(levels <= APERTURE_LAYOUT_LEVELS as u32);
        let mut level_offset = mip_offset;
        let mut level = 0u32;
        while level < APERTURE_LAYOUT_LEVELS as u32 {
            let record = if level < levels {
                ApertureBounds {
                    min: Vec4::new(
                        f32::from_bits(level_offset),
                        f32::from_bits(width),
                        f32::from_bits(1u32 << level),
                        f32::from_bits(levels),
                    ),
                    max: Vec4::ZERO,
                }
            } else {
                ApertureBounds::default()
            };
            output.push(record);
            if level < levels {
                level_offset += width * height;
                width = width.div_ceil(2).max(1);
                height = height.div_ceil(2).max(1);
            }
            level += 1;
        }
        mip_offset = level_offset;
    }
    output
}

/// Build concatenated per-color-mip min/max pyramids. Each color mip has a
/// spatial hierarchy down to 1x1; levels are appended mip-major, then coarse
/// spatial levels. The runtime computes offsets from the base resolution.
///
/// # Panics
/// Panics when a mip has a zero-sized dimension or more than 16 levels.
pub fn build_aperture_bounds(mips: &[ApertureMip<'_>]) -> Vec<ApertureBounds> {
    let mut output = Vec::new();
    for mip in mips {
        assert!(mip.resolution.x > 0 && mip.resolution.y > 0);
        assert_eq!(
            mip.pixels.len(),
            (mip.resolution.x * mip.resolution.y) as usize
        );
        let start = output.len();
        let mut width = mip.resolution.x;
        let mut height = mip.resolution.y;
        let mut current = Vec::with_capacity(mip.pixels.len());
        for pixel in mip.pixels {
            current.push(ApertureBounds {
                min: *pixel,
                max: *pixel,
            });
        }
        output.extend_from_slice(&current);
        while width > 1 || height > 1 {
            let next_width = width.div_ceil(2).max(1);
            let next_height = height.div_ceil(2).max(1);
            let mut next = Vec::with_capacity((next_width * next_height) as usize);
            for y in 0..next_height {
                for x in 0..next_width {
                    next.push(merge_children(&current, width, height, x, y));
                }
            }
            output.extend_from_slice(&next);
            current = next;
            width = next_width;
            height = next_height;
        }
        debug_assert!(output.len() > start);
    }
    output
}

#[cfg(test)]
/// Tests for the aperture layout table.
mod tests {
    use super::*;
    #[test]
    /// Verifies layout records for odd-sized and single-axis mip pyramids.
    fn aperture_layout_records_match_odd_and_single_axis_pyramids() {
        let base = [Vec4::ZERO; 15];
        let lower = [Vec4::ONE; 2];
        let mips = [
            ApertureMip {
                resolution: UVec2::new(5, 3),
                pixels: &base,
            },
            ApertureMip {
                resolution: UVec2::new(2, 1),
                pixels: &lower,
            },
        ];
        let layouts = build_aperture_bounds_layout(&mips);
        let fields = |mip: usize, level: usize| {
            layouts[mip * APERTURE_LAYOUT_LEVELS + level]
                .min
                .to_array()
                .map(f32::to_bits)
        };
        assert_eq!(layouts.len(), 2 * APERTURE_LAYOUT_LEVELS);
        assert_eq!(fields(0, 0), [0, 5, 1, 4]);
        assert_eq!(fields(0, 1), [15, 3, 2, 4]);
        assert_eq!(fields(0, 2), [21, 2, 4, 4]);
        assert_eq!(fields(0, 3), [23, 1, 8, 4]);
        assert_eq!(fields(1, 0), [24, 2, 1, 2]);
        assert_eq!(fields(1, 1), [26, 1, 2, 2]);
        assert_eq!(fields(1, 2), [0, 0, 0, 0]);
    }

    #[test]
    /// Verifies a single-pixel mip has one level and that unused level records stay zero.
    fn single_pixel_mip_has_one_layout_level() {
        let mips = [ApertureMip {
            resolution: UVec2::ONE,
            pixels: &[],
        }];
        let layouts = build_aperture_bounds_layout(&mips);
        assert_eq!(layouts[0].min.to_array().map(f32::to_bits), [0, 1, 1, 1]);
        assert_eq!(layouts[1], ApertureBounds::default());
    }

    #[test]
    /// Verifies a 32768-pixel axis is the largest pyramid that fills all 16 layout levels.
    fn sixteen_level_mip_fills_the_layout_table() {
        let mips = [ApertureMip {
            resolution: UVec2::new(32_768, 1),
            pixels: &[],
        }];
        let layouts = build_aperture_bounds_layout(&mips);
        assert_eq!(layouts.len(), APERTURE_LAYOUT_LEVELS);
        assert_eq!(layouts[15].min.to_array().map(f32::to_bits)[2], 1 << 15);
        assert_eq!(layouts[15].min.w.to_bits(), 16);
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    /// Verifies a pyramid needing more than 16 levels is rejected.
    fn layout_rejects_more_than_sixteen_levels() {
        let mips = [ApertureMip {
            resolution: UVec2::new(65_536, 1),
            pixels: &[],
        }];
        let _ = build_aperture_bounds_layout(&mips);
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    /// Verifies a zero-sized mip is rejected by the layout builder.
    fn layout_rejects_zero_sized_mip() {
        let mips = [ApertureMip {
            resolution: UVec2::new(0, 1),
            pixels: &[],
        }];
        let _ = build_aperture_bounds_layout(&mips);
    }

    #[test]
    /// Verifies each level reduces the 2x2 children into the min and max of their block, and the root covers the whole mip.
    fn aperture_bounds_pyramid_reduces_children_to_block_min_and_max() {
        let pixels = [
            Vec4::new(1.0, -1.0, 0.0, 0.0),
            Vec4::new(3.0, 2.0, 0.0, 0.0),
            Vec4::new(-2.0, 4.0, 0.0, 0.0),
            Vec4::new(0.5, 0.0, 0.0, 0.0),
        ];
        let mips = [ApertureMip {
            resolution: UVec2::new(2, 2),
            pixels: &pixels,
        }];
        let bounds = build_aperture_bounds(&mips);
        assert_eq!(bounds.len(), 5);
        assert_eq!(
            bounds[0],
            ApertureBounds {
                min: pixels[0],
                max: pixels[0],
            }
        );
        assert_eq!(
            bounds[4],
            ApertureBounds {
                min: Vec4::new(-2.0, -1.0, 0.0, 0.0),
                max: Vec4::new(3.0, 4.0, 0.0, 0.0),
            }
        );
    }

    #[test]
    /// Verifies an odd single-row pyramid pairs the leftover column with no out-of-range sample.
    fn aperture_bounds_pyramid_handles_odd_single_row() {
        let pixels = [Vec4::splat(1.0), Vec4::splat(5.0), Vec4::splat(-3.0)];
        let mips = [ApertureMip {
            resolution: UVec2::new(3, 1),
            pixels: &pixels,
        }];
        let bounds = build_aperture_bounds(&mips);
        assert_eq!(bounds.len(), 6);
        assert_eq!(
            bounds[3],
            ApertureBounds {
                min: Vec4::splat(1.0),
                max: Vec4::splat(5.0),
            }
        );
        assert_eq!(
            bounds[4],
            ApertureBounds {
                min: Vec4::splat(-3.0),
                max: Vec4::splat(-3.0),
            }
        );
        assert_eq!(
            bounds[5],
            ApertureBounds {
                min: Vec4::splat(-3.0),
                max: Vec4::splat(5.0),
            }
        );
    }

    #[test]
    #[should_panic(expected = "assertion `left == right` failed")]
    /// Verifies a pixel slice that does not match the mip resolution is rejected.
    fn aperture_bounds_rejects_pixel_count_mismatch() {
        let pixels = [Vec4::ZERO; 3];
        let mips = [ApertureMip {
            resolution: UVec2::new(2, 2),
            pixels: &pixels,
        }];
        let _ = build_aperture_bounds(&mips);
    }
}
