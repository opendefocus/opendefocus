//! Hardware tests of the Accurate layer paths against brute-force references.
use super::far_occlusion_reference::far_occluded_reference;
use super::frozen;
use super::references::{
    assert_close, assert_probes, far_reference, local_reference, near_reference,
};
use super::runner;
use super::scenes::{
    LAYER_PATH_SIZE, forced_descend_radius, layer_path_cases, layer_path_color,
    layer_path_near_blob, layer_path_probe_pixels,
};

/// Regression guard (frozen at 9fe44e0): the four Accurate layer-path scenes.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_frozen_high_layer_path_probes() {
    let runner = runner().await;
    let cases = layer_path_cases();
    assert_eq!(cases.len(), frozen::LAYER_PATH.len());
    for case in &cases {
        let output = case.gpu(&runner).await;
        let pixels = layer_path_probe_pixels();
        assert_probes(
            "layer-path",
            case,
            &output,
            &pixels,
            frozen::LAYER_PATH,
            1e-5,
        );
    }
}

/// Plan item 6 (and the near/far/local parts of item 5): every Accurate layer
/// path against a brute-force reference written in this file. The references
/// share only the documented one-pixel anti-aliased disc rule with the shader.
/// Tolerance 1e-5 relative: measured worst error was 3e-7 (f64 reference
/// against f32 GPU arithmetic).
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_high_layer_paths_match_brute_force_references() {
    let runner = runner().await;
    let size = LAYER_PATH_SIZE;
    for case in layer_path_cases() {
        let expected = match case.name.as_str() {
            "far-only" => far_reference(size, layer_path_color, |_, _| 5.5),
            "near-only" => near_reference(
                size,
                &case.source,
                layer_path_color,
                layer_path_near_blob,
                6.0,
            ),
            "local-only" => local_reference(size, layer_path_color),
            // Alternating 1.1 / 100 px radii force the traversal to descend; the 100 px neighbours lie behind the
            // 1.1 px destinations across a depth gap, so the far occlusion applies there.
            "far-forced-descend" => {
                far_occluded_reference(size, layer_path_color, forced_descend_radius)
            }
            other => panic!("unknown layer path {other}"),
        };
        assert_close(&case.gpu(&runner).await, &expected, 1e-5, &case.name);
    }
}

/// Near-only scene with the blob at `CoC` 2.0 (the smallest radius at which the
/// near layer is fully occupied and the 5x5 local gather has faded out). The
/// blob covers the disc partially, so the coverage blend (occupancy between 0.5
/// and 0.99) is exercised. Brute-force reference, tolerance 1e-5 relative.
#[tokio::test]
#[ignore = "requires an available hardware GPU"]
async fn hardware_high_near_blob_at_coc_two_matches_brute_force_reference() {
    let runner = runner().await;
    let size = LAYER_PATH_SIZE;
    let mut case = layer_path_cases().remove(1);
    for pixel in &mut case.prepared.pixels {
        if pixel.near_valid > 0.0 {
            pixel.near_coc = 2.0;
        }
    }
    case.max_radius = 2.0;
    let expected = near_reference(
        size,
        &case.source,
        layer_path_color,
        layer_path_near_blob,
        2.0,
    );
    assert_close(&case.gpu(&runner).await, &expected, 1e-5, "near coc 2");
}
