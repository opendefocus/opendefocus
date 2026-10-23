//! Analytic test scenes, their prepared layers and the probe pixels used by the oracle-free tests.
use crate::convolve_settings::ConvolveSettings;
use ndarray::Array3;

use crate::runners::adaptive::{AdaptiveLayerPixel, PreparedLayers};

use super::{TileFeatures, WgpuRunner};
use super::{feature, high};

/// A test scene: its source, prepared layers, tile features, maximum radius and optional lens.
pub(super) struct Case {
    pub(super) name: String,
    pub(super) source: Array3<f32>,
    pub(super) prepared: PreparedLayers,
    pub(super) features: Vec<TileFeatures>,
    pub(super) max_radius: f32,
    pub(super) lens: Option<ConvolveSettings>,
}

impl Case {
    /// Runs the Accurate gather for this scene through the lens and returns the output frame.
    pub(super) async fn gpu(&self, runner: &WgpuRunner) -> Array3<f32> {
        runner
            .gather_adaptive_frame_with_lens(
                &self.source,
                &self.prepared,
                &self.features,
                self.max_radius,
                high(),
                self.lens.as_ref(),
            )
            .await
            .unwrap()
    }
}

/// Builds the convolution uniform for a scene of `size`, using the full frame unless `specs_region` is given.
pub(super) fn lens_for(
    settings: &crate::datamodel::Settings,
    size: (usize, usize),
    specs_region: Option<crate::datamodel::IVector4>,
) -> ConvolveSettings {
    let region = specs_region.unwrap_or(crate::datamodel::IVector4 {
        x: 0,
        y: 0,
        z: size.0 as i32,
        w: size.1 as i32,
    });
    let specs = crate::datamodel::render::RenderSpecs {
        full_region: region,
        render_region: region,
    };
    crate::datamodel::settings_to_convolve_settings(
        settings,
        &specs,
        glam::UVec2::splat(256),
        (size.0 * size.1) as u32,
    )
}

/// Plan item 4: six lens effects over a bright blob, foreground and background.
pub(super) fn lens_effect_cases() -> Vec<Case> {
    use crate::datamodel::{Settings, UVector2};
    let (w, h) = (64usize, 48usize);
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, c)| {
        let bright = (x as i32 - 50).pow(2) + (y as i32 - 12).pow(2) < 16;
        if bright {
            [8.0, 4.0, 2.0, 1.0][c]
        } else {
            [0.05, 0.05, 0.05, 1.0][c]
        }
    });
    let mut base = Settings::default();
    base.render.resolution = UVector2 {
        x: w as u32,
        y: h as u32,
    };
    base.render.center.x = w as f32 * 0.5;
    base.render.center.y = h as f32 * 0.5;
    let features = vec![feature(-12.0, 12.0, 2.0); w.div_ceil(16) * h.div_ceil(16)];
    let mut cases = Vec::new();
    for foreground in [false, true] {
        let prepared = PreparedLayers {
            width: w,
            height: h,
            near_edge_distance: vec![-1.0; w * h],
            pixels: (0..w * h)
                .map(|i| {
                    let color = std::array::from_fn(|c| source[[i / w, i % w, c]]);
                    AdaptiveLayerPixel {
                        near_rgba: if foreground { color } else { [0.0; 4] },
                        far_rgba: if foreground { [0.0; 4] } else { color },
                        near_coc: if foreground { 12.0 } else { 0.0 },
                        far_coc: if foreground { 0.0 } else { -12.0 },
                        near_valid: if foreground { 1.0 } else { 0.0 },
                        far_valid: if foreground { 0.0 } else { 1.0 },
                    }
                })
                .collect(),
        };
        for effect in 0..6 {
            let mut settings = base.clone();
            match effect {
                0 => settings.non_uniform.catseye.enable = true,
                1 => settings.non_uniform.barndoors.enable = true,
                2 => settings.non_uniform.astigmatism.enable = true,
                _ => {
                    settings.non_uniform.axial_aberration.enable = true;
                    if effect >= 4 {
                        settings.non_uniform.axial_aberration.amount =
                            if effect == 4 { 1.5 } else { -1.5 };
                    }
                }
            }
            cases.push(Case {
                name: format!("effect{effect}-{}", if foreground { "fg" } else { "bg" }),
                source: source.clone(),
                prepared: prepared.clone(),
                features: features.clone(),
                max_radius: 12.0,
                lens: Some(lens_for(&settings, (w, h), None)),
            });
        }
    }
    cases
}

/// Plan item 5: eleven Accurate lens-integral cases, including an exact zero centre.
pub(super) fn high_lens_cases() -> Vec<Case> {
    use crate::datamodel::{Settings, UVector2};
    let (w, h) = (33usize, 25usize);
    let in_disc = |x: usize, y: usize| (x as i32 - 15).pow(2) + (y as i32 - 12).pow(2) <= 81;
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, channel)| {
        let base = if in_disc(x, y) {
            [6.0, 0.75, 1.5, 1.0]
        } else {
            [0.2 + x as f32 * 0.03, 0.35 + y as f32 * 0.02, 0.6, 1.0]
        };
        base[channel]
    });
    let prepared = PreparedLayers {
        width: w,
        height: h,
        near_edge_distance: vec![-1.0; w * h],
        pixels: (0..w * h)
            .map(|index| {
                let x = index % w;
                let y = index / w;
                let foreground = in_disc(x, y);
                AdaptiveLayerPixel {
                    near_rgba: if foreground {
                        [6.0, 0.75, 1.5, 1.0]
                    } else {
                        [0.0; 4]
                    },
                    far_rgba: [0.2 + x as f32 * 0.03, 0.35 + y as f32 * 0.02, 0.6, 1.0],
                    near_coc: if foreground {
                        if x == 15 { 1.25 } else { 6.0 }
                    } else {
                        0.0
                    },
                    far_coc: -5.5,
                    near_valid: if foreground { 1.0 } else { 0.0 },
                    far_valid: 1.0,
                }
            })
            .collect(),
    };
    let features = vec![feature(-5.5, 6.0, 11.5); w.div_ceil(16) * h.div_ceil(16)];
    let cases = [
        ("none", 0, 1.0, UVector2 { x: 0, y: 0 }),
        ("catseye", 1, 1.0, UVector2 { x: 0, y: 0 }),
        ("barndoors", 2, 1.0, UVector2 { x: 0, y: 0 }),
        ("astigmatism", 4, 1.0, UVector2 { x: 0, y: 0 }),
        ("astigmatism-gamma", 4, 1.3, UVector2 { x: 0, y: 0 }),
        ("axial", 8, 1.0, UVector2 { x: 0, y: 0 }),
        ("combined", 15, 1.3, UVector2 { x: 16, y: 12 }),
        ("combined-tight", 15, 0.4, UVector2 { x: 3, y: 20 }),
        ("combined-wide", 15, 3.0, UVector2 { x: 29, y: 3 }),
        ("catseye-inverse-aspect", 1, 1.0, UVector2 { x: 3, y: 20 }),
        ("barndoors-inverse-aspect", 2, 1.0, UVector2 { x: 29, y: 3 }),
    ];
    cases
        .into_iter()
        .map(|(name, mask, astig_gamma, center)| {
            let mut settings = Settings::default();
            settings.render.resolution = UVector2 {
                x: w as u32,
                y: h as u32,
            };
            settings.render.center.x = center.x as f32;
            settings.render.center.y = center.y as f32;
            settings.non_uniform.catseye.enable = mask & 1 != 0;
            settings.non_uniform.catseye.amount = 0.4;
            settings.non_uniform.catseye.gamma = if name == "combined-tight" {
                0.4
            } else if name == "combined-wide" {
                3.0
            } else {
                1.2
            };
            settings.non_uniform.catseye.softness = 0.2;
            settings.non_uniform.barndoors.enable = mask & 2 != 0;
            settings.non_uniform.barndoors.amount = 0.8;
            settings.non_uniform.barndoors.gamma = settings.non_uniform.catseye.gamma;
            if name.ends_with("inverse-aspect") {
                settings.non_uniform.catseye.inverse = true;
                settings.non_uniform.barndoors.inverse = true;
                settings.defocus.circle_of_confusion.pixel_aspect = 1.8;
            }
            settings.non_uniform.barndoors.top = 55.0;
            settings.non_uniform.barndoors.bottom = 100.0;
            settings.non_uniform.barndoors.left = 100.0;
            settings.non_uniform.barndoors.right = 100.0;
            settings.non_uniform.astigmatism.enable = mask & 4 != 0;
            settings.non_uniform.astigmatism.amount = 0.7;
            settings.non_uniform.astigmatism.gamma = astig_gamma;
            settings.non_uniform.axial_aberration.enable = mask & 8 != 0;
            settings.non_uniform.axial_aberration.amount = 8.0;
            settings.non_uniform.axial_aberration.color_type = 1;
            Case {
                name: name.to_string(),
                source: source.clone(),
                prepared: prepared.clone(),
                features: features.clone(),
                max_radius: 6.0,
                lens: Some(lens_for(&settings, (w, h), None)),
            }
        })
        .collect()
}

/// Colour of the layer-path scenes at `(x, y)`; also the near/far layer colour.
pub(super) fn layer_path_color(x: usize, y: usize) -> [f32; 4] {
    [0.3 + x as f32 * 0.01, 0.2 + y as f32 * 0.015, 0.5, 1.0]
}

/// Frame size (width, height) of the layer path scene.
pub(super) const LAYER_PATH_SIZE: (usize, usize) = (13, 9);

/// Returns true for pixels inside the near blob of the layer path scene.
pub(super) fn layer_path_near_blob(x: usize, y: usize) -> bool {
    (x as i32 - 5).pow(2) + (y as i32 - 4).pow(2) <= 4
}

/// Plan item 6: far-only, near-only, local-only and forced-descend layer paths.
pub(super) fn layer_path_cases() -> Vec<Case> {
    let (w, h) = LAYER_PATH_SIZE;
    let source = Array3::from_shape_fn((h, w, 4), |(y, x, channel)| {
        [0.15 + x as f32 * 0.025, 0.25 + y as f32 * 0.02, 0.4, 1.0][channel]
    });
    [
        ("far-only", 0u8, 6.0f32),
        ("near-only", 1, 6.0),
        ("local-only", 2, 6.0),
        ("far-forced-descend", 3, 100.0),
    ]
    .into_iter()
    .map(|(name, mode, max_radius)| Case {
        name: name.to_string(),
        source: source.clone(),
        features: vec![feature(-5.5, 6.0, 11.5)],
        max_radius,
        lens: None,
        prepared: PreparedLayers {
            width: w,
            height: h,
            near_edge_distance: vec![-1.0; w * h],
            pixels: (0..w * h)
                .map(|index| {
                    let x = index % w;
                    let y = index / w;
                    let color = layer_path_color(x, y);
                    let foreground = layer_path_near_blob(x, y);
                    match mode {
                        0 => AdaptiveLayerPixel {
                            near_rgba: [0.0; 4],
                            far_rgba: color,
                            near_coc: 0.0,
                            far_coc: -5.5,
                            near_valid: 0.0,
                            far_valid: 1.0,
                        },
                        1 => AdaptiveLayerPixel {
                            near_rgba: if foreground { color } else { [0.0; 4] },
                            far_rgba: [0.0; 4],
                            near_coc: if foreground { 6.0 } else { 0.0 },
                            far_coc: 0.0,
                            near_valid: if foreground { 1.0 } else { 0.0 },
                            far_valid: 0.0,
                        },
                        2 => AdaptiveLayerPixel {
                            near_rgba: color,
                            far_rgba: [0.0; 4],
                            near_coc: 1.0,
                            far_coc: 0.0,
                            near_valid: 1.0,
                            far_valid: 0.0,
                        },
                        _ => AdaptiveLayerPixel {
                            near_rgba: [0.0; 4],
                            far_rgba: color,
                            near_coc: 0.0,
                            far_coc: if (x + y) % 2 == 0 { -1.1 } else { -100.0 },
                            near_valid: 0.0,
                            far_valid: 1.0,
                        },
                    }
                })
                .collect(),
        },
    })
    .collect()
}

/// Probe pixels `(x, y)`: frame corners, centre and interior points.
fn probe_pixels(w: usize, h: usize, extra: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut pixels = vec![
        (0, 0),
        (w - 1, 0),
        (0, h - 1),
        (w - 1, h - 1),
        (w / 2, h / 2),
    ];
    pixels.extend_from_slice(extra);
    pixels
}

/// Returns the probe pixels for lens effects: the centre and flanks of the bright blob, the diagonal halo and the frame edges.
pub(super) fn lens_effect_probe_pixels() -> Vec<(usize, usize)> {
    // Around the bright blob at (50, 12): its centre, both flanks, the diagonal
    // halo, and the top and right frame edges the 12 px discs reach.
    vec![
        (50, 12),
        (46, 12),
        (53, 15),
        (44, 6),
        (58, 18),
        (50, 0),
        (63, 12),
        (40, 20),
    ]
}

/// Returns the probe pixels for Accurate lens integrals, including the exact lens centre and the edge columns.
pub(super) fn high_lens_probe_pixels() -> Vec<(usize, usize)> {
    // (16, 12) is the exact lens centre of the "combined" case and (15, 12)
    // is the single small-CoC column; (0, 12) and (32, 12) are edge columns.
    probe_pixels(33, 25, &[(16, 12), (15, 12), (0, 12), (32, 12)])
}

/// Returns the probe pixels for the layer path scene.
pub(super) fn layer_path_probe_pixels() -> Vec<(usize, usize)> {
    probe_pixels(13, 9, &[(5, 4), (3, 4), (10, 2)])
}

/// Lens settings for the 33x25 `high_lens_cases` scene with all effects off and
/// the inverse-foreground disc shift disabled, so that it is a plain circle.
pub(super) fn plain_lens_variant(
    f: impl FnOnce(&mut crate::datamodel::Settings),
) -> ConvolveSettings {
    let mut settings = crate::datamodel::Settings::default();
    settings.render.resolution = crate::datamodel::UVector2 { x: 33, y: 25 };
    settings.non_uniform.inverse_foreground = false;
    f(&mut settings);
    lens_for(&settings, (33, 25), None)
}

/// Builds prepared layers of `(w, h)` in which every pixel is produced by `pixel`.
pub(super) fn uniform_layers(
    (w, h): (usize, usize),
    pixel: impl Fn(usize, usize) -> AdaptiveLayerPixel,
) -> PreparedLayers {
    PreparedLayers {
        width: w,
        height: h,
        near_edge_distance: vec![-1.0; w * h],
        pixels: (0..w * h).map(|i| pixel(i % w, i / w)).collect(),
    }
}

/// Returns a layer pixel with only far-layer colour and CoC set, fully valid.
pub(super) fn far_pixel(color: [f32; 4], coc: f32) -> AdaptiveLayerPixel {
    AdaptiveLayerPixel {
        near_rgba: [0.0; 4],
        far_rgba: color,
        near_coc: 0.0,
        far_coc: coc,
        near_valid: 0.0,
        far_valid: 1.0,
    }
}

/// Returns the settings for one lens effect (`effect`) on a frame of `size`, centred at `center`.
pub(super) fn effect_settings(
    effect: usize,
    size: (usize, usize),
    center: (f32, f32),
) -> crate::datamodel::Settings {
    let mut settings = crate::datamodel::Settings::default();
    settings.render.resolution = crate::datamodel::UVector2 {
        x: size.0 as u32,
        y: size.1 as u32,
    };
    settings.render.center.x = center.0;
    settings.render.center.y = center.1;
    settings.non_uniform.inverse_foreground = false;
    match effect {
        0 => settings.non_uniform.catseye.enable = true,
        1 => settings.non_uniform.barndoors.enable = true,
        2 => settings.non_uniform.astigmatism.enable = true,
        3 => settings.non_uniform.axial_aberration.enable = true,
        4 => {
            settings.non_uniform.axial_aberration.enable = true;
            settings.non_uniform.axial_aberration.amount = 1.5;
        }
        5 => {
            settings.non_uniform.axial_aberration.enable = true;
            settings.non_uniform.axial_aberration.amount = -1.5;
        }
        _ => {}
    }
    settings
}
