//! Analytic scene cases and point-spread-function (PSF) cases.

use std::path::Path;

use anyhow::Result;

use super::{
    scene::{Dims, constant_scene, point_scene},
    write::{Mode, write_case},
};
use crate::fixtures::scenes::{
    analytic::{HEIGHT, WIDTH, scene},
    json::Json,
};

/// Resolution of the PSF filter kernel. It sets the expected centroid shift of near PSFs.
pub const FILTER_RESOLUTION: f64 = 256.0;
/// Image size of the analytic scene cases.
const SCENE_DIMS: Dims = (WIDTH, HEIGHT);
/// Image size of the large PSF cases, which must fit the largest radii.
const LARGE_DIMS: Dims = (320, 256);
/// Analytic scenes that each get a real-depth, an inverse-depth and a direct-CoC case.
const SCENE_NAMES: [&str; 4] = ["occlusion", "focus_ramp", "hdr_points", "alpha_edges"];
/// Bright points of the `hdr_points` scene as `(x, y, label)`, where the label records
/// whether the point is near, at focus or far.
const BRIGHT_POINTS: [(i64, i64, &str); 6] = [
    (30, 30, "near"),
    (96, 30, "focus"),
    (158, 30, "far"),
    (30, 94, "far"),
    (96, 94, "far"),
    (188, 124, "far"),
];

/// Writes the real-depth, inverse-depth and direct-CoC cases for every analytic scene and
/// returns their manifest entries. Fails when a case cannot be written.
pub fn scene_cases(root: &Path) -> Result<Vec<Json>> {
    let mut cases = Vec::new();
    for name in SCENE_NAMES {
        let analytic = scene(name);
        let bright = point_metadata(name);
        let real_focal = [0.0, 0.01, 4.0, 10.0, 15.0];
        let inverse_depth: Vec<f64> = analytic.depth.iter().map(|z| 1.0 / z).collect();
        let source_coc: Vec<f64> = analytic
            .depth
            .iter()
            .map(|z| 24.0 * (1.0 - 4.0 / z))
            .collect();
        let direct: Vec<f64> = source_coc.iter().map(|coc| -coc).collect();
        let mut real_extra = vec![
            ("focus_planes", Json::floats(&real_focal)),
            ("sizes", sizes()),
        ];
        real_extra.extend(bright.clone());
        cases.push(write_case(
            root,
            &format!("real-depth-{name}"),
            (&analytic.color, &analytic.depth),
            Mode::RealDepth,
            SCENE_DIMS,
            real_extra,
        )?);

        let mut inverse_extra = vec![
            (
                "focus_planes",
                Json::floats(&[0.0, 0.01, 0.25, 0.1, 1.0 / 15.0]),
            ),
            ("sizes", sizes()),
            ("equivalent_real_focal_planes", Json::floats(&real_focal)),
        ];
        inverse_extra.extend(bright.clone());
        cases.push(write_case(
            root,
            &format!("inverse-depth-{name}"),
            (&analytic.color, &inverse_depth),
            Mode::InverseDepth,
            SCENE_DIMS,
            inverse_extra,
        )?);

        let mut direct_extra = vec![(
            "signed_coc",
            Json::str("positive foreground/near, negative background/far"),
        )];
        direct_extra.extend(bright);
        cases.push(write_case(
            root,
            &format!("direct-{name}"),
            (&analytic.color, &direct),
            Mode::DirectCoc,
            SCENE_DIMS,
            direct_extra,
        )?);
    }
    Ok(cases)
}

/// Circle-of-confusion sizes that the scene cases are rendered at.
fn sizes() -> Json {
    Json::floats(&[1.0, 8.0, 24.0, 56.0])
}

/// Returns the bright-point list for the `hdr_points` scene and nothing for the other
/// scenes.
fn point_metadata(name: &str) -> Vec<(&'static str, Json)> {
    if name != "hdr_points" {
        return Vec::new();
    }
    let points = BRIGHT_POINTS
        .iter()
        .map(|&(x, y, sign)| {
            Json::Object(vec![("xy", Json::ints(&[x, y])), ("sign", Json::str(sign))])
        })
        .collect();
    vec![("bright_points", Json::List(points))]
}

/// Writes a constant-depth case at the 4 m focal plane, which must pass through unchanged.
/// Fails when the case cannot be written.
pub fn focused_constant_case(root: &Path) -> Result<Json> {
    let (color, depth) = constant_scene();
    write_case(
        root,
        "focused-constant",
        (&color, &depth),
        Mode::RealDepth,
        SCENE_DIMS,
        vec![
            ("focus_plane", Json::Float(4.0)),
            ("expected_rgba", Json::floats(&[0.2, 0.1, 0.05, 0.5])),
            ("gate", Json::str("focused_identity")),
        ],
    )
}

/// Sign of a circle of confusion: `NEAR` for foreground and `FAR` for background.
#[derive(Clone, Copy)]
pub struct Sign {
    /// Label used in case names and manifest entries.
    pub name: &'static str,
    /// Multiplier applied to a radius: `1.0` for near and `-1.0` for far.
    pub value: f64,
}

/// Near (foreground) sign.
pub const NEAR: Sign = Sign {
    name: "near",
    value: 1.0,
};
/// Far (background) sign.
pub const FAR: Sign = Sign {
    name: "far",
    value: -1.0,
};

/// Shift of the expected centroid for the near kernel; far kernels are not shifted.
fn kernel_shift(radius: f64, sign: Sign) -> f64 {
    if sign.value > 0.0 {
        2.0 * radius / FILTER_RESOLUTION
    } else {
        0.0
    }
}

/// Writes the point-spread-function cases at four radii for each sign, with and without a
/// transparent background, and returns their manifest entries. Fails when a case cannot be
/// written.
pub fn psf_cases(root: &Path) -> Result<Vec<Json>> {
    let mut cases = Vec::new();
    for sign in [NEAR, FAR] {
        for radius in [2.0, 4.0, 8.0, 12.0] {
            let shift = kernel_shift(radius, sign);
            let centre = [96.0 + shift, 64.0 + shift];
            for transparent in [false, true] {
                let (color, coc) = point_scene(radius, sign.value, transparent, SCENE_DIMS);
                let prefix = if transparent {
                    "psf-transparent"
                } else {
                    "psf"
                };
                cases.push(write_case(
                    root,
                    &format!("{prefix}-{}-r{}", sign.name, radius as i64),
                    (&color, &coc),
                    Mode::DirectCoc,
                    SCENE_DIMS,
                    psf_extra(radius, sign, transparent.then_some(true), centre, shift),
                )?);
            }
        }
    }
    cases.extend(large_psf_cases(root)?);
    Ok(cases)
}

/// `transparent` is `None` when the case does not record the background flag.
fn psf_extra(
    radius: f64,
    sign: Sign,
    transparent: Option<bool>,
    centre: [f64; 2],
    shift: f64,
) -> Vec<(&'static str, Json)> {
    let mut extra = vec![
        ("radius", Json::Float(radius)),
        ("sign", Json::str(sign.name)),
        ("gate", Json::str("disk_psf")),
    ];
    if let Some(transparent) = transparent {
        extra.push(("transparent_background", Json::Bool(transparent)));
    }
    extra.extend([
        ("expected_centroid_xy", Json::floats(&centre)),
        ("expected_kernel_shift_xy", Json::floats(&[shift, shift])),
        ("filter_resolution", Json::Float(FILTER_RESOLUTION)),
    ]);
    extra
}

/// Writes the large PSF cases: radii up to 98 pixels for both signs, with and without a
/// transparent background, plus centred near cases as a control. Fails when a case cannot
/// be written.
fn large_psf_cases(root: &Path) -> Result<Vec<Json>> {
    let mut cases = Vec::new();
    for sign in [NEAR, FAR] {
        for radius in [16.0, 32.0, 64.0, 98.0] {
            let shift = kernel_shift(radius, sign);
            let centre = [160.0 + shift, 128.0 + shift];
            for transparent in [false, true] {
                let (color, coc) = point_scene(radius, sign.value, transparent, LARGE_DIMS);
                let mut extra = psf_extra(radius, sign, Some(transparent), centre, shift);
                extra.push((
                    "large_radius_grid",
                    Json::str("integer offsets and radial footprint are independent checks"),
                ));
                let infix = if transparent { "transparent-" } else { "" };
                cases.push(write_case(
                    root,
                    &format!("psf-large-{infix}{}-r{}", sign.name, radius as i64),
                    (&color, &coc),
                    Mode::DirectCoc,
                    LARGE_DIMS,
                    extra,
                )?);
            }
        }
    }
    for radius in [16.0, 32.0, 64.0, 98.0] {
        for transparent in [false, true] {
            let (color, coc) = point_scene(radius, NEAR.value, transparent, LARGE_DIMS);
            let infix = if transparent { "transparent-" } else { "" };
            cases.push(write_case(
                root,
                &format!("psf-large-centered-near-{infix}r{}", radius as i64),
                (&color, &coc),
                Mode::DirectCoc,
                LARGE_DIMS,
                vec![
                    ("radius", Json::Float(radius)),
                    ("sign", Json::str("near")),
                    ("gate", Json::str("disk_psf")),
                    ("transparent_background", Json::Bool(transparent)),
                    ("expected_centroid_xy", Json::ints(&[160, 128])),
                    ("expected_kernel_shift_xy", Json::floats(&[0.0, 0.0])),
                    ("inverse_foreground", Json::Bool(false)),
                    ("centered_psf_control", Json::Bool(true)),
                    ("filter_resolution", Json::Float(FILTER_RESOLUTION)),
                ],
            )?);
        }
    }
    Ok(cases)
}
