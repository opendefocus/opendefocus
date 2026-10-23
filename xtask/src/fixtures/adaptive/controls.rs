//! Lens sweeps, silhouette, border and focused-identity controls.

use std::path::Path;

use anyhow::Result;

use super::{
    cases::{FAR, NEAR, Sign},
    scene::{constant_scene, silhouette_scene},
    write::{Mode, write_case},
};
use crate::fixtures::scenes::{
    analytic::{HEIGHT, WIDTH},
    exr::Rgba,
    json::Json,
};

/// Image size of the lens sweep cases. The size makes the spots fall on exact pixel
/// positions.
const LENS_DIMS: (usize, usize) = (257, 193);
/// Pixel positions of the nine bright spots in the lens sweep grid.
const LENS_SPOTS: [(i64, i64); 9] = [
    (36, 36),
    (128, 36),
    (220, 36),
    (36, 96),
    (128, 96),
    (220, 96),
    (36, 156),
    (128, 156),
    (220, 156),
];
/// Image size of the scene-based control cases.
const SCENE_DIMS: (usize, usize) = (WIDTH, HEIGHT);

/// Writes the lens sweep cases: one spot case per sign, a transparent-background case and a
/// focused control. Returns their manifest entries. Fails when a case cannot be written.
pub fn lens_cases(root: &Path) -> Result<Vec<Json>> {
    let mut cases = Vec::new();
    for sign in [NEAR, FAR] {
        cases.push(lens_spot_case(root, sign)?);
    }
    cases.push(lens_transparent_case(root)?);
    cases.push(lens_focus_control_case(root)?);
    Ok(cases)
}

/// Writes a sweep case with a bright spot at each of `LENS_SPOTS`, with CoC `sign * 8`. Fails
/// when the case cannot be written.
fn lens_spot_case(root: &Path, sign: Sign) -> Result<Json> {
    let (width, _) = LENS_DIMS;
    let mut color = vec![[0.0, 0.0, 0.0, 1.0]; LENS_DIMS.0 * LENS_DIMS.1];
    for (x, y) in LENS_SPOTS {
        color[y as usize * width + x as usize] = [8.0, 4.0, 2.0, 1.0];
    }
    let coc = vec![sign.value * 8.0; color.len()];
    let bright: Vec<Json> = LENS_SPOTS
        .iter()
        .map(|&(x, y)| {
            Json::Object(vec![
                ("xy", Json::ints(&[x, y])),
                ("sign", Json::str(sign.name)),
            ])
        })
        .collect();
    write_case(
        root,
        &format!("lens-psf-{}-coc8-max102", sign.name),
        (&color, &coc),
        Mode::DirectCoc,
        LENS_DIMS,
        lens_extra(sign.name, None, vec![("bright_points", Json::List(bright))]),
    )
}

/// `transparent` is `None` when the case does not record the background flag.
fn lens_extra(
    sign: &'static str,
    transparent: Option<bool>,
    tail: Vec<(&'static str, Json)>,
) -> Vec<(&'static str, Json)> {
    let mut extra = vec![
        (
            "dimensions",
            Json::ints(&[LENS_DIMS.0 as i64, LENS_DIMS.1 as i64]),
        ),
        ("radius", Json::Float(8.0)),
        ("raw_coc_magnitude", Json::Float(8.0)),
        ("configured_max_size", Json::Float(102.0)),
        ("sign", Json::str(sign)),
        ("gate", Json::str("lens_psf")),
        ("lens_sweep", Json::Bool(true)),
    ];
    if let Some(transparent) = transparent {
        extra.push(("transparent_background", Json::Bool(transparent)));
    }
    extra.extend(tail);
    extra
}

/// Writes a transparent-background sweep case with one fractional bright point at
/// (128.25, 96.5), spread over four pixels. Fails when the case cannot be written.
fn lens_transparent_case(root: &Path) -> Result<Json> {
    let (width, height) = LENS_DIMS;
    let (center_x, center_y, alpha) = (128.25_f64, 96.5_f64, 0.35_f64);
    let mut color: Vec<Rgba> = vec![[0.0; 4]; width * height];
    let x0 = center_x.floor();
    let y0 = center_y.floor();
    let (fx, fy) = (center_x - x0, center_y - y0);
    let (x0, y0) = (x0 as usize, y0 as usize);
    for (y, wy) in [(y0, 1.0 - fy), (y0 + 1, fy)] {
        for (x, wx) in [(x0, 1.0 - fx), (x0 + 1, fx)] {
            let weight = wx * wy;
            color[y * width + x] = [
                16.0 * alpha * weight,
                8.0 * alpha * weight,
                4.0 * alpha * weight,
                alpha * weight,
            ];
        }
    }
    let coc = vec![8.0; color.len()];
    let extra = lens_extra(
        "near",
        Some(true),
        vec![(
            "bright_points",
            Json::List(vec![Json::Object(vec![
                ("xy", Json::floats(&[center_x, center_y])),
                ("sign", Json::str("near")),
            ])]),
        )],
    );
    write_case(
        root,
        "lens-psf-transparent-hdr-near-coc8-max102",
        (&color, &coc),
        Mode::DirectCoc,
        LENS_DIMS,
        extra,
    )
}

/// Writes a constant-colour case at zero depth that must pass through unchanged. Fails when
/// the case cannot be written.
fn lens_focus_control_case(root: &Path) -> Result<Json> {
    let (color, _) = constant_scene();
    let depth = vec![0.0; color.len()];
    write_case(
        root,
        "lens-focused-control",
        (&color, &depth),
        Mode::DirectCoc,
        SCENE_DIMS,
        vec![
            ("gate", Json::str("focused_identity")),
            ("expected_rgba", Json::floats(&[0.2, 0.1, 0.05, 0.5])),
            ("lens_sweep", Json::Bool(true)),
        ],
    )
}

/// Writes the foreground silhouette, odd-border, focused HDR and focused gradient controls
/// and returns their manifest entries. Fails when a case cannot be written.
pub fn focus_and_edge_cases(root: &Path) -> Result<Vec<Json>> {
    let (silhouette_color, silhouette_coc) = silhouette_scene(8.0, 0.0);
    let silhouette = write_case(
        root,
        "foreground-silhouette-r8",
        (&silhouette_color, &silhouette_coc),
        Mode::DirectCoc,
        SCENE_DIMS,
        vec![
            ("radius", Json::Float(8.0)),
            ("gate", Json::str("disk_edge")),
            ("foreground_shape", Json::str("disk")),
            ("foreground_radius", Json::Int(28)),
        ],
    )?;
    Ok(vec![
        silhouette,
        odd_border_case(root)?,
        focused_hdr_case(root)?,
        focused_gradient_case(root)?,
    ])
}

/// Writes a 193 by 129 case with one bright pixel near the corner and an 8-pixel CoC, to
/// check PSF clipping at the image border. Fails when the case cannot be written.
fn odd_border_case(root: &Path) -> Result<Json> {
    let (width, height) = (193, 129);
    let mut color = vec![[0.0, 0.0, 0.0, 1.0]; width * height];
    color[2 * width + 2] = [1.0; 4];
    let coc = vec![8.0; color.len()];
    write_case(
        root,
        "odd-border-psf-r8",
        (&color, &coc),
        Mode::DirectCoc,
        (width, height),
        vec![
            ("radius", Json::Float(8.0)),
            ("gate", Json::str("disk_psf")),
            ("expected_centroid_xy", Json::ints(&[2, 2])),
            ("edge_clipped", Json::Bool(true)),
            ("inverse_foreground", Json::Bool(false)),
        ],
    )
}

/// Writes a constant HDR case at zero depth that must pass through unchanged. Fails when the
/// case cannot be written.
fn focused_hdr_case(root: &Path) -> Result<Json> {
    let color = vec![[16.0, 4.0, 1.0, 1.0]; WIDTH * HEIGHT];
    let depth = vec![0.0; color.len()];
    write_case(
        root,
        "focused-constant-hdr",
        (&color, &depth),
        Mode::DirectCoc,
        SCENE_DIMS,
        vec![
            ("gate", Json::str("focused_identity")),
            ("expected_rgba", Json::floats(&[16.0, 4.0, 1.0, 1.0])),
        ],
    )
}

/// Writes a colour gradient across the frame at zero depth that must pass through unchanged.
/// Fails when the case cannot be written.
fn focused_gradient_case(root: &Path) -> Result<Json> {
    let mut color = Vec::with_capacity(WIDTH * HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            color.push([
                x as f64 / WIDTH as f64 * 4.0,
                y as f64 / HEIGHT as f64 * 2.0,
                (x + y) as f64 / (WIDTH + HEIGHT) as f64,
                1.0,
            ]);
        }
    }
    let depth = vec![0.0; color.len()];
    write_case(
        root,
        "focused-gradient",
        (&color, &depth),
        Mode::DirectCoc,
        SCENE_DIMS,
        vec![("gate", Json::str("focused_gradient_identity"))],
    )
}
