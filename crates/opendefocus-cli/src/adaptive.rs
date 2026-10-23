//! GPU adaptive helpers for the CLI: the per-pixel CoC map used by the classifier
//! and the JSON summary of classified tiles.

use anyhow::{Context, Result, ensure};
use ndarray::Array2;
use opendefocus::datamodel::{Settings, defocus::DefocusMode};

/// Returns the signed CoC map for the classifier, one value per pixel.
///
/// In 2D mode every pixel gets the constant size from the settings. Otherwise `depth`
/// is required and must match `shape` (height, width), and the map is derived from it.
///
/// # Errors
///
/// Returns an error if depth mode is active without a depth map, or if the depth
/// dimensions differ from `shape`.
#[cfg(feature = "gpu")]
pub fn adaptive_coc(
    depth: Option<&Array2<f32>>,
    settings: &Settings,
    shape: (usize, usize),
) -> Result<Array2<f32>> {
    if settings.defocus.defocus_mode() == DefocusMode::Twod {
        return Ok(Array2::from_elem(shape, settings.defocus.get_size()));
    }
    let depth = depth.context("Adaptive depth classification requires --depth")?;
    ensure!(
        depth.dim() == shape,
        "Depth must have the same dimensions as color"
    );
    Ok(opendefocus::adaptive_coc_from_depth(depth, settings))
}

/// Summarizes classified tiles as JSON: counts and percentages per class, and how many
/// tiles each reason bit (depth, alpha, highlight, texture) flagged.
///
/// Each feature stores the class in `values[9]` (clamped to 0..=2) and the reason bits in
/// `values[8]`. An empty input reports zero percentages.
#[cfg(feature = "gpu")]
pub fn tile_summary(features: &[opendefocus::TileFeatures]) -> serde_json::Value {
    let mut counts = [0usize; 3];
    let mut reasons = [0usize; 4];
    for feature in features {
        counts[(feature.values[9] as usize).min(2)] += 1;
        let bits = feature.values[8] as u32;
        for (i, count) in reasons.iter_mut().enumerate() {
            if bits & (1 << i) != 0 {
                *count += 1;
            }
        }
    }
    let total = features.len().max(1) as f64;
    serde_json::json!({
        "tiles": features.len(),
        "sharp": counts[0], "smooth_blur": counts[1], "detail": counts[2],
        "sharp_percent": counts[0] as f64 * 100.0 / total,
        "smooth_blur_percent": counts[1] as f64 * 100.0 / total,
        "detail_percent": counts[2] as f64 * 100.0 / total,
        "reason_tiles": {"depth": reasons[0], "alpha": reasons[1], "highlight": reasons[2], "texture": reasons[3]}
    })
}

#[cfg(all(test, feature = "gpu"))]
mod adaptive_input_tests {
    use super::*;
    use crate::args::Args;
    use clap::Parser;
    use opendefocus::datamodel::{UVector2, circle_of_confusion::CameraData};

    /// Checks that the removed `--backend` option is rejected by argument parsing.
    #[test]
    fn cpu_backend_option_is_removed() {
        assert!(Args::try_parse_from(["opendefocus-cli", "--backend", "cpu"]).is_err());
    }

    /// Checks that direct-math depth values reach the classifier with their signs unchanged.
    #[test]
    fn direct_coc_signs_are_passed_to_classifier_without_inversion() {
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.use_direct_math = true;
        settings.defocus.circle_of_confusion.max_size = 12.0;
        let depth = Array2::from_shape_vec((1, 3), vec![-12.0, 0.0, 7.0]).unwrap();
        assert_eq!(
            adaptive_coc(Some(&depth), &settings, (1, 3)).unwrap(),
            depth
        );
    }

    /// Checks that camera-based CoC is positive in front of focus, negative behind it, and matches the direct map.
    #[test]
    fn camera_coc_signs_map_near_to_positive_foreground_and_far_to_negative_background() {
        use opendefocus::datamodel::circle_of_confusion::{Filmback, Math, Resolution, WorldUnit};

        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.use_direct_math = false;
        settings.defocus.use_camera_focal = true;
        settings.defocus.camera_max_size = 24.0;
        settings.render.resolution = UVector2 { x: 1920, y: 1080 };
        settings.defocus.circle_of_confusion =
            opendefocus::datamodel::circle_of_confusion::Settings {
                size: 10.0,
                max_size: 24.0,
                math: Math::Real.into(),
                focal_plane: 30.0,
                protect: 0.0,
                pixel_aspect: 1.0,
                camera_data: Some(CameraData {
                    focal_length: 100.0,
                    f_stop: 2.0,
                    filmback: Filmback {
                        width: 24.576,
                        height: 18.672,
                    },
                    near_field: 0.1,
                    far_field: 10000.0,
                    world_unit: WorldUnit::M.into(),
                    resolution: Resolution {
                        width: 1920,
                        height: 1080,
                    },
                }),
            };
        let depth = Array2::from_shape_vec((1, 2), vec![10.0, 100.0]).unwrap();
        let coc = adaptive_coc(Some(&depth), &settings, (1, 2)).unwrap();
        assert!(coc[[0, 0]] > 0.0, "near/foreground CoC should be positive");
        assert!(coc[[0, 1]] < 0.0, "far/background CoC should be negative");
        settings.defocus.use_camera_focal = false;
        settings.defocus.use_direct_math = true;
        let direct_coc = adaptive_coc(Some(&coc), &settings, (1, 2)).unwrap();
        assert_eq!(
            direct_coc, coc,
            "direct and camera CoC must agree pixel-for-pixel"
        );
    }
}
