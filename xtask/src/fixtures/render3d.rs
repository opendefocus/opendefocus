//! A ray-traced 1080p scene (glossy spheres, emissive bokeh balls, checker floor) with a
//! matching real depth, for comparing renderer quality modes on a realistic image.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
};

use anyhow::{Context, Result, bail};

use exr::prelude::{Compression, Encoding, Image, SpecificChannels, WritableImage};

use crate::util::crate_root;

/// A 3-component vector: position, direction or linear RGB.
type V = [f32; 3];
/// One premultiplied RGBA pixel.
type Rgba = (f32, f32, f32, f32);

/// Image width in pixels.
const WIDTH: usize = 1920;
/// Image height in pixels.
const HEIGHT: usize = 1080;
/// Depth assigned to rays that hit nothing.
const FAR_Z: f32 = 60.0;
/// CLI settings overlay: real depth values, focus at depth 6, 80 px of blur per unit of relative depth error and a 64 px cap.
const SETTINGS: &str = r#"{"defocus":{"defocus_mode":1,"circle_of_confusion":{"size":80.0,"max_size":64.0,"focal_plane":6.0,"math":0}}}
"#;

/// Options of `cargo xtask fixtures render3d`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Directory that receives color.exr, depth.exr and settings.json
    /// (default: test/images/scenes/render3d)
    #[arg(long)]
    output_dir: Option<PathBuf>,
}

/// One sphere of the scene.
struct Sphere {
    /// Centre.
    center: V,
    /// Radius.
    radius: f32,
    /// Diffuse colour.
    color: V,
    /// Emission multiplier; above zero the ball is a light source.
    emit: f32,
    /// Mirror blend factor.
    reflect: f32,
}

/// A ray hit on a sphere or on the floor.
struct Hit {
    /// Ray distance.
    t: f32,
    /// World position.
    point: V,
    /// Surface normal.
    normal: V,
    /// Sphere index, or `None` for the floor.
    sphere: Option<usize>,
}

/// Component-wise sum.
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
/// Component-wise difference.
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
/// Scales a vector by `k`.
fn mul(a: V, k: f32) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
/// Dot product.
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// Unit-length copy of `a`.
fn norm(a: V) -> V {
    mul(a, 1.0 / dot(a, a).sqrt())
}

/// Six hero spheres plus fourteen small emissive balls that become bokeh discs.
fn scene() -> Vec<Sphere> {
    let hero = |center, radius, color, reflect| Sphere {
        center,
        radius,
        color,
        emit: 0.0,
        reflect,
    };
    let mut spheres = vec![
        hero([-1.6, -0.1, 4.0], 0.9, [0.9, 0.15, 0.1], 0.2),
        hero([0.2, -0.3, 6.0], 0.7, [0.2, 0.7, 0.2], 0.3),
        hero([1.8, 0.0, 8.0], 1.0, [0.15, 0.3, 0.9], 0.25),
        hero([-3.5, 0.3, 11.0], 1.3, [0.9, 0.8, 0.2], 0.2),
        hero([3.8, 0.2, 14.0], 1.2, [0.7, 0.3, 0.8], 0.2),
        hero([-0.8, 0.0, 3.0], 0.25, [1.0, 0.95, 0.9], 0.5),
    ];
    for k in 0..14_u8 {
        let k = f32::from(k);
        let a = k * 0.9;
        spheres.push(Sphere {
            center: [
                -6.0 + k * 0.9,
                -0.85 + 0.2 * a.sin(),
                9.0 + 2.0 * (a * 1.3).cos() + k * 0.8,
            ],
            radius: 0.1,
            color: [1.0, 0.8 + 0.2 * a.sin().abs(), 0.5],
            emit: 40.0,
            reflect: 0.0,
        });
    }
    spheres
}

/// Nearest sphere or floor (plane y = -1) hit along the ray, if any.
fn hit(origin: V, dir: V, spheres: &[Sphere]) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (index, sphere) in spheres.iter().enumerate() {
        let oc = sub(origin, sphere.center);
        let b = dot(oc, dir);
        let disc = b * b - (dot(oc, oc) - sphere.radius * sphere.radius);
        if disc > 0.0 {
            let t = -b - disc.sqrt();
            if t > 1e-3 && best.as_ref().is_none_or(|h| t < h.t) {
                let point = add(origin, mul(dir, t));
                let normal = norm(sub(point, sphere.center));
                best = Some(Hit {
                    t,
                    point,
                    normal,
                    sphere: Some(index),
                });
            }
        }
    }
    if dir[1] < -1e-4 {
        let t = (-1.0 - origin[1]) / dir[1];
        if t > 1e-3 && best.as_ref().is_none_or(|h| t < h.t) {
            best = Some(Hit {
                t,
                point: add(origin, mul(dir, t)),
                normal: [0.0, 1.0, 0.0],
                sphere: None,
            });
        }
    }
    best
}

/// Phong-style colour with one shadow ray and `bounces` mirror bounces.
fn shade(origin: V, dir: V, spheres: &[Sphere], bounces: u32) -> V {
    let Some(h) = hit(origin, dir, spheres) else {
        let t = f32::midpoint(dir[1], 1.0);
        return add(mul([0.9, 0.55, 0.35], 1.0 - t), mul([0.15, 0.3, 0.7], t));
    };
    let (color, emit, reflect) = if let Some(i) = h.sphere {
        (spheres[i].color, spheres[i].emit, spheres[i].reflect)
    } else {
        let cell = (h.point[0].floor() + h.point[2].floor()).rem_euclid(2.0);
        (mul([0.8, 0.8, 0.75], 0.25 + 0.75 * cell), 0.0, 0.15)
    };
    if emit > 0.0 {
        return mul(color, emit);
    }
    let light = norm([-0.5, 0.8, -0.4]);
    let above = add(h.point, mul(h.normal, 1e-3));
    let lit = if hit(above, light, spheres).is_some() {
        0.0
    } else {
        1.0
    };
    let diffuse = dot(h.normal, light).max(0.0) * lit;
    let specular = dot(h.normal, norm(sub(light, dir))).max(0.0).powf(60.0) * lit;
    let base = add(mul(color, 0.12 + 0.95 * diffuse), [specular; 3]);
    if bounces == 0 || reflect <= 0.0 {
        return base;
    }
    let mirror = sub(dir, mul(h.normal, 2.0 * dot(dir, h.normal)));
    add(
        mul(base, 1.0 - reflect),
        mul(shade(above, mirror, spheres, bounces - 1), reflect),
    )
}

/// Renders one row with 2×2 supersampling: premultiplied colour and depth pixels.
fn render_row(y: usize, spheres: &[Sphere]) -> Vec<(Rgba, Rgba)> {
    let aspect = WIDTH as f32 / HEIGHT as f32;
    (0..WIDTH)
        .map(|x| {
            let mut color = [0.0; 3];
            let mut nearest = f32::MAX;
            for sub_y in 0..2 {
                for sub_x in 0..2 {
                    let u = ((x as f32 + 0.25 + 0.5 * sub_x as f32) / WIDTH as f32 * 2.0 - 1.0)
                        * aspect
                        * 0.5;
                    let v =
                        (1.0 - (y as f32 + 0.25 + 0.5 * sub_y as f32) / HEIGHT as f32 * 2.0) * 0.5;
                    let dir = norm([u, v - 0.05, 1.0]);
                    color = add(color, mul(shade([0.0; 3], dir, spheres, 2), 0.25));
                    let z = hit([0.0; 3], dir, spheres).map_or(FAR_Z, |h| h.t * dir[2]);
                    nearest = nearest.min(z);
                }
            }
            (
                (color[0], color[1], color[2], 1.0),
                (nearest, nearest, nearest, 1.0),
            )
        })
        .collect()
}

/// Writes `pixels` as a FLOAT RGBA EXR with the given compression.
fn write_exr(path: &Path, pixels: &[Rgba], compression: Compression) -> Result<()> {
    let channels = SpecificChannels::rgba(|position: exr::prelude::Vec2<usize>| {
        pixels[position.y() * WIDTH + position.x()]
    });
    let encoding = Encoding {
        compression,
        ..Encoding::FAST_LOSSLESS
    };
    Image::from_encoded_channels((WIDTH, HEIGHT), encoding, channels)
        .write()
        .to_file(path)
        .with_context(|| format!("writing {}", path.display()))
}

/// Writes the scene's `color.exr` (lossy DWAA, small enough to commit), `depth.exr` (lossless
/// ZIP so the depth stays exact) and `settings.json` into the output directory, replacing
/// existing files. Fails when a file cannot be written.
pub fn run(args: Args) -> Result<()> {
    let output_dir = args
        .output_dir
        .unwrap_or_else(|| crate_root().join("test/images/scenes/render3d"));
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating {}", output_dir.display()))?;
    let spheres = scene();
    let workers = thread::available_parallelism().map_or(4, usize::from);
    let rows: Vec<usize> = (0..HEIGHT).collect();
    let rendered: Vec<Vec<(Rgba, Rgba)>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let (spheres, rows) = (&spheres, &rows);
                scope.spawn(move || {
                    rows.iter()
                        .skip(worker)
                        .step_by(workers)
                        .map(|&y| (y, render_row(y, spheres)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut ordered = vec![Vec::new(); HEIGHT];
        for handle in handles {
            for (y, row) in handle.join().unwrap_or_default() {
                ordered[y] = row;
            }
        }
        ordered
    });
    if rendered.iter().any(Vec::is_empty) {
        bail!("A render worker failed");
    }
    let (color, depth): (Vec<_>, Vec<_>) = rendered.into_iter().flatten().unzip();
    write_exr(
        &output_dir.join("color.exr"),
        &color,
        Compression::DWAA(None),
    )?;
    write_exr(&output_dir.join("depth.exr"), &depth, Compression::ZIP16)?;
    let settings = output_dir.join("settings.json");
    fs::write(&settings, SETTINGS).with_context(|| format!("writing {}", settings.display()))?;
    println!("{}", output_dir.display());
    Ok(())
}
