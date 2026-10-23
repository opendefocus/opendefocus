//! Hierarchical integral trees of prepared pixels, packed into the 16-word GPU node layout that the exact Accurate gather walks.
use glam::{UVec2, UVec4, Vec4};

use std::vec::Vec;

/// Tag bit set on a child reference that points to a base pixel instead of a node.
const LEAF_BIT: u32 = 1 << 31;
/// Child reference meaning no child (all bits set).
const EMPTY_REF: u32 = u32::MAX;
/// Coarse hierarchy node. Base pixels are referenced by tagged child indices
/// and loaded from the prepared source only when traversal reaches a boundary.
#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
pub struct IntegralNode {
    /// Sum over the node of premultiplied colour weighted by validity divided by CoC squared.
    pub weighted_premul: Vec4,
    /// Pixel bounds of the node as (x0, y0, x1, y1), with an exclusive upper corner.
    pub bounds: UVec4,
    /// Child indices; the high bit marks a base-pixel reference.
    pub children: UVec4,
    /// Smallest CoC radius among the node's contributing pixels.
    pub coc_min: f32,
    /// Largest CoC radius among the node's contributing pixels.
    pub coc_max: f32,
    /// Sum over the node of validity divided by CoC squared.
    pub weighted_valid: f32,
    /// Sum over the node of front occupancy, used by the potential-coverage integral.
    pub occupied_area: f32,
}

impl IntegralNode {
    /// Pack into the 16-u32 storage-buffer layout used by the renderer.
    #[allow(
        clippy::inline_always,
        reason = "keeps the existing forced inlining of the packing path"
    )]
    #[inline(always)]
    pub fn pack(self) -> [u32; 16] {
        let premul = self.weighted_premul.to_array();
        let bounds = self.bounds.to_array();
        let children = self.children.to_array();
        [
            premul[0].to_bits(),
            premul[1].to_bits(),
            premul[2].to_bits(),
            premul[3].to_bits(),
            bounds[0],
            bounds[1],
            bounds[2],
            bounds[3],
            children[0],
            children[1],
            children[2],
            children[3],
            self.coc_min.to_bits(),
            self.coc_max.to_bits(),
            self.weighted_valid.to_bits(),
            self.occupied_area.to_bits(),
        ]
    }
}

/// Prepared source pixel. `premul` has geometric validity factored out;
/// `valid` carries that coverage, and `occupied` carries the chosen front
/// occupancy for the separate potential-disk integral.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntegralPixel {
    /// Premultiplied linear colour with geometric validity removed.
    pub premul: Vec4,
    /// CoC radius of the pixel in pixels.
    pub coc: f32,
    /// Coverage weight of the pixel, in [0, 1].
    pub valid: f32,
    /// Geometric occupancy used by the potential-coverage integral.
    pub occupied: f32,
}

/// Destination for packed integral nodes. Slots are reserved first and filled later.
trait IntegralNodeStorage {
    /// Reserves an empty slot for a node with `bounds` and returns its index.
    fn push_empty(&mut self, bounds: UVec4) -> u32;
    /// Stores `node` at `index`, packed into the storage layout.
    fn write(&mut self, index: u32, node: IntegralNode);
}

impl IntegralNodeStorage for Vec<[u32; 16]> {
    fn push_empty(&mut self, _bounds: UVec4) -> u32 {
        let index = self.len() as u32;
        self.push([0; 16]);
        index
    }

    fn write(&mut self, index: u32, node: IntegralNode) {
        self[index as usize] = node.pack();
    }
}

#[derive(Clone, Copy)]
/// Accumulated statistics of a subtree: weighted colour, validity, CoC range and occupancy.
struct Summary {
    /// Sum of premultiplied colour, weighted by validity over CoC squared.
    premul: Vec4,
    /// Sum of weighted validity.
    valid: f32,
    /// Smallest CoC in the subtree; `f32::MAX` while the subtree is empty.
    coc_min: f32,
    /// Largest CoC in the subtree.
    coc_max: f32,
    /// Sum of front occupancy.
    occupied: f32,
}

impl Summary {
    /// Identity for `add`: an empty subtree.
    const EMPTY: Self = Self {
        premul: Vec4::ZERO,
        valid: 0.0,
        coc_min: f32::MAX,
        coc_max: 0.0,
        occupied: 0.0,
    };

    /// Summarizes one pixel. Invisible pixels and pixels with a CoC of 0.5 or less contribute only occupancy; others contribute weighted colour and validity.
    fn from_pixel(pixel: IntegralPixel) -> Self {
        let valid = pixel.valid.clamp(0.0, 1.0);
        let radius = pixel.coc.max(0.0);
        let occupied = pixel.occupied.max(0.0);
        if valid <= 0.0 || radius <= 0.5 {
            return Self {
                occupied,
                ..Self::EMPTY
            };
        }
        let weighted_valid = valid / (radius * radius);
        Self {
            premul: pixel.premul * weighted_valid,
            valid: weighted_valid,
            coc_min: radius,
            coc_max: radius,
            occupied,
        }
    }

    /// Adds `child` into this summary.
    fn add(&mut self, child: Self) {
        self.premul += child.premul;
        self.valid += child.valid;
        self.coc_min = self.coc_min.min(child.coc_min);
        self.coc_max = self.coc_max.max(child.coc_max);
        self.occupied += child.occupied;
    }
}

/// Build the same preorder hierarchy directly in the packed storage layout
/// consumed by GPU gather, without allocating an intermediate node vector.
///
/// # Panics
/// Panics when `width` or `height` is zero, or when `width * height` is not below 2^31.
pub fn build_integral_tree_packed<F>(width: u32, height: u32, load_pixel: F) -> Vec<[u32; 16]>
where
    F: Fn(UVec2) -> IntegralPixel,
{
    assert!(
        width > 0 && height > 0,
        "integral tree dimensions must be nonzero"
    );
    assert!((width as u64) * (height as u64) < LEAF_BIT as u64);
    let root_size = width.max(height).next_power_of_two();
    let mut nodes = Vec::with_capacity(dyadic_node_count(width, height, root_size));
    let _ = build_subtree(
        &mut nodes,
        &load_pixel,
        width,
        height,
        UVec2::ZERO,
        root_size,
    );
    nodes
}

/// Counts the nodes of the dyadic tree for a `width` by `height` frame with the given root size.
/// A block clipped to a single bottom-right pixel is a leaf and is not stored as a node.
fn dyadic_node_count(width: u32, height: u32, root_size: u32) -> usize {
    let mut count = 0usize;
    let mut block_size = root_size;
    while block_size > 1 {
        let columns = width.div_ceil(block_size);
        let rows = height.div_ceil(block_size);
        let last_width = width - (columns - 1) * block_size;
        let last_height = height - (rows - 1) * block_size;
        let leaf_corner = usize::from(last_width == 1 && last_height == 1);
        count += (columns as usize) * (rows as usize) - leaf_corner;
        block_size /= 2;
    }
    count
}

/// Builds the subtree for the block at `origin` with side `block_size`, writing its nodes in preorder.
/// Returns the child reference (node index, leaf or `EMPTY_REF`) and the subtree summary.
/// A single-pixel block becomes a leaf that refers to its pixel index.
fn build_subtree<F, N>(
    nodes: &mut N,
    load_pixel: &F,
    width: u32,
    height: u32,
    origin: UVec2,
    block_size: u32,
) -> (u32, Summary)
where
    F: Fn(UVec2) -> IntegralPixel,
    N: IntegralNodeStorage,
{
    if origin.x >= width || origin.y >= height {
        return (EMPTY_REF, Summary::EMPTY);
    }

    let bounds = UVec4::new(
        origin.x,
        origin.y,
        (origin.x + block_size).min(width),
        (origin.y + block_size).min(height),
    );
    if bounds.z - bounds.x == 1 && bounds.w - bounds.y == 1 {
        return (
            LEAF_BIT | (origin.y * width + origin.x),
            Summary::from_pixel(load_pixel(origin)),
        );
    }

    let index = nodes.push_empty(bounds);
    let half_size = block_size / 2;
    let mut summary = Summary::EMPTY;
    let mut children = UVec4::splat(EMPTY_REF);
    for child_slot in 0..4 {
        let child_origin = origin + UVec2::new(child_slot % 2, child_slot / 2) * half_size;
        let (child_ref, child_summary) =
            build_subtree(nodes, load_pixel, width, height, child_origin, half_size);
        children[child_slot as usize] = child_ref;
        if child_ref == EMPTY_REF {
            continue;
        }
        summary.add(child_summary);
    }
    nodes.write(
        index,
        IntegralNode {
            weighted_premul: summary.premul,
            bounds,
            children,
            coc_min: summary.coc_min,
            coc_max: summary.coc_max,
            weighted_valid: summary.valid,
            occupied_area: summary.occupied,
        },
    );
    (index, summary)
}

#[cfg(test)]
/// Tests for the packed integral tree.
mod tests {
    use super::*;

    #[test]
    /// Verifies bounds, child references and node counts for odd, degenerate and small frames.
    fn dyadic_tree_layout_handles_odd_and_degenerate_frames() {
        for (width, height) in [(1, 1), (1, 13), (13, 1), (2, 2), (3, 5), (7, 9), (17, 13)] {
            let nodes = build_integral_tree_packed(width, height, |_| IntegralPixel {
                premul: Vec4::new(0.125, 0.5, 1.0, 0.75),
                coc: 2.25,
                valid: 1.0,
                occupied: 0.5,
            });
            let root_size = width.max(height).next_power_of_two();
            assert_eq!(
                nodes.len(),
                dyadic_node_count(width, height, root_size),
                "node count for {width}×{height}"
            );
            if width == 1 && height == 1 {
                assert_eq!(nodes.len(), 0);
                continue;
            }
            // The root is the first preorder node and covers the whole frame.
            assert_eq!(nodes[0][4..8], [0, 0, width, height]);

            // Packed words: bounds at 4..8, child references at 8..12.
            for (parent_index, words) in nodes.iter().enumerate() {
                let bounds = [words[4], words[5], words[6], words[7]];
                assert!(bounds[0] < bounds[2] && bounds[2] <= width);
                assert!(bounds[1] < bounds[3] && bounds[3] <= height);
                for &child in &words[8..12] {
                    if child == EMPTY_REF {
                        continue;
                    }
                    if child & LEAF_BIT != 0 {
                        let pixel_index = child & !LEAF_BIT;
                        let (x, y) = (pixel_index % width, pixel_index / width);
                        assert!(x >= bounds[0] && x < bounds[2]);
                        assert!(y >= bounds[1] && y < bounds[3]);
                    } else {
                        assert!(child as usize > parent_index);
                        let child_words = &nodes[child as usize];
                        assert!(child_words[4] >= bounds[0] && child_words[6] <= bounds[2]);
                        assert!(child_words[5] >= bounds[1] && child_words[7] <= bounds[3]);
                    }
                }
            }
        }
    }

    #[test]
    /// Verifies the node count of a full HD frame.
    fn full_hd_dyadic_node_count_uses_padded_quadrants() {
        assert_eq!(dyadic_node_count(1920, 1080, 2048), 691_302);
    }

    #[test]
    /// Verifies the packed node is 16 words and 64 bytes with the expected field order.
    fn node_storage_layout_is_exactly_sixteen_words() {
        let node = IntegralNode {
            weighted_premul: Vec4::new(1.0, 2.0, 3.0, 4.0),
            bounds: UVec4::new(5, 6, 7, 8),
            children: UVec4::new(9, 10, 11, 12),
            coc_min: 13.0,
            coc_max: 14.0,
            weighted_valid: 15.0,
            occupied_area: 16.0,
        };
        assert_eq!(core::mem::size_of::<IntegralNode>(), 64);
        assert_eq!(core::mem::align_of::<IntegralNode>(), 16);
        assert_eq!(
            node.pack(),
            [
                1.0_f32.to_bits(),
                2.0_f32.to_bits(),
                3.0_f32.to_bits(),
                4.0_f32.to_bits(),
                5,
                6,
                7,
                8,
                9,
                10,
                11,
                12,
                13.0_f32.to_bits(),
                14.0_f32.to_bits(),
                15.0_f32.to_bits(),
                16.0_f32.to_bits(),
            ]
        );
    }
}
