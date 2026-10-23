//! Tests for the tile classifier, subtree ends and native shader validation.
use ndarray::{Array2, Array3};

use crate::runners::adaptive::classifier_overlay;

use super::WgpuRunner;

#[test]
/// Verifies subtree ends skip only the nodes contained in each subtree.
fn subtree_ends_skip_only_contained_preorder_nodes() {
    let packed =
        crate::kernel::build_integral_tree_packed(5, 3, |_| crate::kernel::IntegralPixel {
            coc: 4.0,
            valid: 1.0,
            ..Default::default()
        });
    // Packed words 4..8 hold the node bounds (x, y, z, w).
    let bounds = |node: &[u32; 16]| [node[4], node[5], node[6], node[7]];
    let ends = super::integral_subtree_ends(&packed).unwrap();
    assert_eq!(ends[0] as usize, packed.len());
    for (index, node) in packed.iter().enumerate() {
        let [x, y, z, w] = bounds(node);
        for (next, candidate) in packed.iter().enumerate().skip(index + 1) {
            let [cx, cy, cz, cw] = bounds(candidate);
            let contained = cx >= x && cy >= y && cz <= z && cw <= w;
            assert_eq!(contained, next < ends[index] as usize);
        }
    }
    let mut invalid = packed;
    invalid[0][8] = 0;
    assert!(super::integral_subtree_ends(&invalid).is_err());
}

#[test]
/// Verifies every native adaptive shader passes Naga validation.
fn native_adaptive_shaders_validate() {
    for (name, source) in [
        ("classifier", crate::kernel::ADAPTIVE_CLASSIFIER_WGSL),
        ("prepare", crate::kernel::ADAPTIVE_PREPARE_WGSL),
        ("pyramid", crate::kernel::ADAPTIVE_PYRAMID_WGSL),
        ("convolution", crate::kernel::ADAPTIVE_CONVOLUTION_WGSL),
        ("preview", crate::kernel::ADAPTIVE_PREVIEW_WGSL),
    ] {
        let module = wgpu::naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
        if name == "convolution" {
            assert!(
                module
                    .entry_points
                    .iter()
                    .any(|entry| entry.name == "adaptive_stream_compact")
            );
        }
    }
    assert_eq!(std::mem::size_of::<super::GatherParams>(), 64);
    assert_eq!(std::mem::size_of::<super::PreviewParams>(), 80);
    assert_eq!(
        std::mem::size_of::<crate::convolve_settings::ConvolveSettings>(),
        160
    );
}

#[tokio::test]
#[ignore = "requires an available hardware GPU"]
/// Hardware check that the classifier tracks offset tiles and blur reach.
async fn hardware_classifier_tracks_offset_tiles_and_blur_reach() {
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let mut rgba = Array3::from_elem((16, 32, 4), 0.1f32);
    rgba.slice_mut(ndarray::s![.., .., 3]).fill(1.0);
    rgba[[4, 25, 0]] = 8.0;
    rgba[[4, 25, 1]] = 8.0;
    rgba[[4, 25, 2]] = 8.0;
    let coc = Array2::from_shape_fn((16, 32), |(_, x)| {
        if x == 0 {
            4.0
        } else if x >= 17 {
            -8.0
        } else {
            0.0
        }
    });
    let features = runner
        .classify_tiles(&rgba, &coc, [15, 0], 8.0)
        .await
        .unwrap();
    assert_eq!(features.len(), 3);
    assert_eq!(features[0].values[0], 4.0);
    assert_eq!(features[2].values[1], -8.0);
    assert_eq!(features[2].values[8] as u32 & 4, 4);
    assert_eq!(features[0].values[9], 2.0);
    assert_eq!(features[0].values[10], 4.0);
    assert_eq!(features[1].values[9], 2.0);
    assert_eq!(features[2].values[9], 2.0);
    let overlay = classifier_overlay(&rgba, &features, [3, 1], [15, 0]);
    assert_eq!(overlay.dim(), (16, 64, 4));
    assert_eq!(overlay[[0, 32, 0]], 1.0);
    assert_eq!(overlay[[0, 33, 0]], 1.0);
}
