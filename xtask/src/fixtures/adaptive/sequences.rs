//! Frame sequences for temporal gates: translating silhouettes, focus ramps and moving points.

use std::path::Path;

use anyhow::Result;

use super::{
    cases::FILTER_RESOLUTION,
    scene::{Point, fractional_points_scene, paint_coc_patch, point_scene, silhouette_scene},
    write::{Mode, write_case},
};
use crate::fixtures::scenes::{
    analytic::{HEIGHT, WIDTH},
    exr::Rgba,
    json::Json,
};

/// Opaque black background used by the point sequences.
const OPAQUE_BLACK: Rgba = [0.0, 0.0, 0.0, 1.0];
/// Image size of the sequences that use the analytic scene size.
const SCENE_DIMS: (usize, usize) = (WIDTH, HEIGHT);

/// Returns the expected kernel shift `[s, s]` of a PSF with `radius` pixels, where
/// `s = 2 * radius / FILTER_RESOLUTION`.
fn kernel_shift_pair(radius: f64) -> Json {
    Json::floats(&[2.0 * radius / FILTER_RESOLUTION; 2])
}

/// Writes seven frames of a silhouette that translates from -3 to +3 pixels. Fails when a
/// frame cannot be written.
pub fn motion(root: &Path) -> Result<Vec<Json>> {
    (0..7_i64)
        .map(|index| {
            let translation = index - 3;
            let (color, coc) = silhouette_scene(8.0, translation as f64);
            write_case(
                root,
                &format!("motion-{index:02}"),
                (&color, &coc),
                Mode::DirectCoc,
                SCENE_DIMS,
                vec![
                    ("frame", Json::Int(index)),
                    ("translation_px", Json::Int(translation)),
                    ("gate", Json::str("motion_disk")),
                ],
            )
        })
        .collect()
}

/// Writes nine frames whose point CoC runs from -4 to +4 pixels, with a zero-CoC frame in the
/// middle. The expected kernel shift is only set for positive radii. Fails when a frame cannot
/// be written.
pub fn focus_ramp(root: &Path) -> Result<Vec<Json>> {
    let signed_values: [f64; 9] = [-4.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0];
    signed_values
        .iter()
        .enumerate()
        .map(|(index, &signed)| {
            let sign = if signed >= 0.0 { 1.0 } else { -1.0 };
            let (color, mut coc) = point_scene(signed.abs(), sign, false, SCENE_DIMS);
            if index == 4 {
                coc.fill(0.0);
            }
            let shift = if signed > 0.0 {
                kernel_shift_pair(signed.abs())
            } else {
                Json::floats(&[0.0, 0.0])
            };
            write_case(
                root,
                &format!("focus-motion-{index:02}"),
                (&color, &coc),
                Mode::DirectCoc,
                SCENE_DIMS,
                vec![
                    ("frame", Json::Int(index as i64)),
                    ("signed_coc", Json::Float(signed)),
                    ("expected_radius", Json::Float(signed.abs())),
                    ("expected_kernel_shift_xy", shift),
                    ("gate", Json::str("focus_motion")),
                ],
            )
        })
        .collect()
}

/// Writes 17 frames of a far point with CoC -32 that moves 4 pixels across a focused edge, to
/// test occlusion at that discontinuity. Fails when a frame cannot be written.
pub fn focused_occlusion(root: &Path) -> Result<Vec<Json>> {
    (0..17_i64)
        .map(|index| {
            let center = (110.0 + index as f64 * 0.25, 78.0);
            let point = Point {
                center_xy: center,
                signed_radius: -32.0,
                rgba: [0.0, 24.0, 0.0, 1.0],
            };
            let (color, coc) = fractional_points_scene(&[point], SCENE_DIMS, OPAQUE_BLACK);
            write_case(
                root,
                &format!("motion-focused-occlusion-{index:02}"),
                (&color, &coc),
                Mode::DirectCoc,
                SCENE_DIMS,
                vec![
                    ("frame", Json::Int(index)),
                    ("translation_px", Json::Float(center.0 - 110.0)),
                    ("dimensions", Json::ints(&[WIDTH as i64, HEIGHT as i64])),
                    ("gate", Json::str("focus_occlusion")),
                    ("expected_center_xy", Json::floats(&[center.0, center.1])),
                    ("expected_green_energy", Json::Float(24.0)),
                    (
                        "purpose",
                        Json::str(
                            "a far sample at a focused opaque-plane discontinuity; test surrounding occlusion, diagnose source radiance",
                        ),
                    ),
                ],
            )
        })
        .collect()
}

/// Writes 17 frames of a near point with an 8-pixel CoC that moves 0.25 pixel per frame, to
/// check its centroid and energy. Fails when a frame cannot be written.
pub fn moving_point(root: &Path) -> Result<Vec<Json>> {
    (0..17_i64)
        .map(|index| {
            let center = (14.0 + index as f64 * 0.25, 64.0);
            let (color, coc) = moving_point_scene(center);
            write_case(
                root,
                &format!("motion-point-{index:02}"),
                (&color, &coc),
                Mode::DirectCoc,
                SCENE_DIMS,
                vec![
                    ("frame", Json::Int(index)),
                    ("translation_px", Json::Float(center.0 - 14.0)),
                    ("expected_center_xy", Json::floats(&[center.0, center.1])),
                    ("expected_radius", Json::Float(8.0)),
                    ("expected_kernel_shift_xy", kernel_shift_pair(8.0)),
                    (
                        "expected_signal_energy",
                        Json::Float(32.0 * 0.2126 + 10.0 * 0.7152 + 4.0 * 0.0722),
                    ),
                    ("gate", Json::str("motion_point")),
                ],
            )
        })
        .collect()
}

/// Returns a scene with one bilinear-splatted point at `center`, coloured `(32, 10, 4)` with
/// CoC 8.
fn moving_point_scene(center: (f64, f64)) -> (Vec<Rgba>, Vec<f64>) {
    let (width, height) = SCENE_DIMS;
    let mut color = vec![OPAQUE_BLACK; width * height];
    let mut coc = vec![0.0; width * height];
    let x0 = center.0.floor();
    let y0 = center.1.floor();
    let (fx, fy) = (center.0 - x0, center.1 - y0);
    let (x0, y0) = (x0 as usize, y0 as usize);
    for (y, wy) in [(y0, 1.0 - fy), (y0 + 1, fy)] {
        for (x, wx) in [(x0, 1.0 - fx), (x0 + 1, fx)] {
            let weight = wx * wy;
            if weight <= 0.0 {
                continue;
            }
            let pixel = y * width + x;
            color[pixel] = [32.0 * weight, 10.0 * weight, 4.0 * weight, 1.0];
            coc[pixel] = 8.0;
        }
    }
    (color, coc)
}

/// Writes 17 frames of a 64-pixel-radius HDR point that moves 4 pixels across a 320 by 256
/// frame, crossing a 16-pixel tile boundary. Fails when a frame cannot be written.
pub fn large_point_motion(root: &Path) -> Result<Vec<Json>> {
    let dims = (320, 256);
    (0..17_i64)
        .map(|index| {
            let center = (158.0 + index as f64 * 0.25, 128.0);
            let point = Point {
                center_xy: center,
                signed_radius: 64.0,
                rgba: [32.0, 32.0, 32.0, 1.0],
            };
            let (color, coc) = fractional_points_scene(&[point], dims, OPAQUE_BLACK);
            let channels: Vec<Json> = ["red", "green", "blue"]
                .iter()
                .enumerate()
                .map(|(channel, name)| {
                    Json::Object(vec![
                        ("name", Json::str(*name)),
                        ("index", Json::Int(channel as i64)),
                        ("center_xy", Json::floats(&[center.0, center.1])),
                        ("radius", Json::Float(64.0)),
                        ("kernel_shift_xy", kernel_shift_pair(64.0)),
                        ("expected_signal_energy", Json::Float(32.0)),
                    ])
                })
                .collect();
            write_case(
                root,
                &format!("motion-large-point-{index:02}"),
                (&color, &coc),
                Mode::DirectCoc,
                dims,
                vec![
                    ("frame", Json::Int(index)),
                    ("translation_px", Json::Float(center.0 - 158.0)),
                    ("dimensions", Json::ints(&[320, 256])),
                    ("gate", Json::str("motion_channel_grid")),
                    ("independent_channel_sources", Json::Bool(false)),
                    ("opaque_background", Json::Bool(true)),
                    ("channels", Json::List(channels)),
                    (
                        "purpose",
                        Json::str("HDR fractional point crosses a 16px tile boundary at radius 64"),
                    ),
                ],
            )
        })
        .collect()
}

/// A coloured point of the colour-grid sequence: name, the channel whose energy is checked,
/// base position, radius, signed CoC and linear colour.
struct Track {
    /// Colour name recorded in the manifest.
    name: &'static str,
    /// Index of the colour channel checked in `rgba` (0 red, 1 green, 2 blue).
    index: usize,
    /// Centre in the first frame; moves with the frame shift.
    base_xy: (f64, f64),
    /// Radius in pixels of the point's circle of confusion.
    radius: f64,
    /// Signed CoC written to the point's pixels: positive for near, negative for far.
    signed_radius: f64,
    /// Linear colour of the point.
    rgba: Rgba,
}

/// The red, green and blue point tracks of the colour-grid sequence.
const TRACKS: [Track; 3] = [
    Track {
        name: "red",
        index: 0,
        base_xy: (100.0, 100.0),
        radius: 12.0,
        signed_radius: 12.0,
        rgba: [32.0, 0.0, 0.0, 1.0],
    },
    Track {
        name: "green",
        index: 1,
        base_xy: (260.0, 140.0),
        radius: 32.0,
        signed_radius: -32.0,
        rgba: [0.0, 24.0, 0.0, 1.0],
    },
    Track {
        name: "blue",
        index: 2,
        base_xy: (540.0, 300.0),
        radius: 64.0,
        signed_radius: 64.0,
        rgba: [0.0, 0.0, 40.0, 1.0],
    },
];

/// Half-size of the depth patch that `paint_coc_patch` writes around a track: twice its
/// radius plus 8 pixels.
fn patch_half_extent(track: &Track) -> i64 {
    2 * track.radius as i64 + 8
}

/// Writes 17 frames of the red, green and blue points, each with its own radius and sign,
/// moving 4 pixels together, plus a depth patch per point. Fails when a frame cannot be
/// written.
pub fn colored_point_motion(root: &Path) -> Result<Vec<Json>> {
    let dims = (768, 512);
    (0..17_i64)
        .map(|index| {
            let shift = index as f64 * 0.25;
            let mut points = Vec::new();
            let mut channels = Vec::new();
            for track in &TRACKS {
                let center = (track.base_xy.0 + shift, track.base_xy.1);
                points.push(Point {
                    center_xy: center,
                    signed_radius: track.signed_radius,
                    rgba: track.rgba,
                });
                let kernel_shift = if track.signed_radius > 0.0 {
                    kernel_shift_pair(track.radius)
                } else {
                    Json::floats(&[0.0, 0.0])
                };
                channels.push(Json::Object(vec![
                    ("name", Json::str(track.name)),
                    ("index", Json::Int(track.index as i64)),
                    ("center_xy", Json::floats(&[center.0, center.1])),
                    ("radius", Json::Float(track.radius)),
                    ("kernel_shift_xy", kernel_shift),
                    (
                        "expected_signal_energy",
                        Json::Float(track.rgba[track.index]),
                    ),
                ]));
            }
            let (color, mut coc) = fractional_points_scene(&points, dims, OPAQUE_BLACK);
            for track in &TRACKS {
                paint_coc_patch(
                    &mut coc,
                    dims,
                    track.base_xy,
                    patch_half_extent(track),
                    track.signed_radius,
                );
            }
            let patch_extents = TRACKS
                .iter()
                .map(|track| (track.name, Json::Int(patch_half_extent(track))))
                .collect();
            write_case(
                root,
                &format!("motion-colored-grid-{index:02}"),
                (&color, &coc),
                Mode::DirectCoc,
                dims,
                vec![
                    ("frame", Json::Int(index)),
                    ("translation_px", Json::Float(shift)),
                    ("dimensions", Json::ints(&[768, 512])),
                    ("gate", Json::str("motion_channel_grid")),
                    ("independent_channel_sources", Json::Bool(true)),
                    ("opaque_background", Json::Bool(true)),
                    ("depth_patch_half_extents_px", Json::Object(patch_extents)),
                    ("channels", Json::List(channels)),
                    (
                        "purpose",
                        Json::str(
                            "independent HDR RGB tracks expose per-channel position and CoC bias",
                        ),
                    ),
                ],
            )
        })
        .collect()
}
