//! Renders the ray-traced 3D scene at a series of focal planes and turns the results into
//! looping animations, so a focus pull can be checked for flicker and artifacts.
use super::{compare, render::render_once, workload::Workload};
use anyhow::{Context, Result, ensure};
use image::{
    RgbaImage,
    imageops::{self, FilterType},
};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

/// Folder of the scene, relative to the repository root.
const SCENE_FOLDER: &str = "test/images/scenes/render3d";
/// Focal plane of the nearest hero sphere and a little in front of it.
const FOCAL_FIRST: f32 = 3.0;
/// Focal plane behind the farthest hero sphere.
const FOCAL_LAST: f32 = 14.0;
/// Number of focal planes rendered, evenly spaced from `FOCAL_FIRST` to `FOCAL_LAST`.
const FOCAL_STEPS: usize = 12;
/// Width in pixels of the animation frames; the render is scaled down to keep the files small.
const FRAME_WIDTH: u32 = 640;
/// Display time of each animation frame in hundredths of a second.
const FRAME_DELAY_CENTISECONDS: u16 = 15;
/// JSON pointer of the focal plane inside the scene settings.
const JSON_FOCAL_POINTER: &str = "/defocus/circle_of_confusion/focal_plane";
/// Path of the head animation below the output folder.
pub const HEAD_ANIMATION: &str = "sweep/head.png";
/// Path of the base animation below the output folder.
pub const BASE_ANIMATION: &str = "sweep/base.png";
/// Path of the amplified difference animation below the output folder.
pub const DIFFERENCE_ANIMATION: &str = "sweep/diff.png";

/// What the sweep produced.
#[derive(Debug)]
pub struct Sweep {
    /// Number of focal planes rendered.
    pub focal_steps: usize,
    /// Largest absolute difference between base and head over all focal planes, when a base
    /// build was rendered.
    pub max_difference: Option<f32>,
}

/// Returns the focal planes of the sweep, evenly spaced and inclusive of both ends.
fn focal_planes() -> Vec<f32> {
    let span = FOCAL_LAST - FOCAL_FIRST;
    (0..FOCAL_STEPS)
        .map(|step| FOCAL_FIRST + span * step as f32 / (FOCAL_STEPS - 1) as f32)
        .collect()
}

/// Returns the indices of a forward and back playback of `count` frames without repeating the
/// end frames, so the loop has no stall.
fn ping_pong(count: usize) -> Vec<usize> {
    (0..count)
        .chain((1..count.saturating_sub(1)).rev())
        .collect()
}

/// Builds the workload for one focal plane: the scene's own settings with the focal plane set.
///
/// # Errors
/// Fails when the settings file cannot be read or written.
fn workload(root: &Path, focal: f32, settings_folder: &Path, index: usize) -> Result<Workload> {
    let scene = root.join(SCENE_FOLDER);
    let source = scene.join("settings.json");
    let mut settings: Value = serde_json::from_slice(
        &fs::read(&source).with_context(|| format!("Reading {}", source.display()))?,
    )?;
    *settings
        .pointer_mut(JSON_FOCAL_POINTER)
        .context("The scene settings have no focal plane")? = Value::from(focal);
    fs::create_dir_all(settings_folder)?;
    let settings_path = settings_folder.join(format!("focal_{index:02}.json"));
    fs::write(&settings_path, serde_json::to_vec(&settings)?)?;
    let path_text = |path: PathBuf| path.to_string_lossy().into_owned();
    Ok(Workload {
        name: format!("focal_{index:02}"),
        args: vec![
            path_text(scene.join("color.exr")),
            "--depth".into(),
            path_text(scene.join("depth.exr")),
            "--settings".into(),
            path_text(settings_path),
            "--quality".into(),
            "fast".into(),
            "--render-mode".into(),
            "full".into(),
        ],
    })
}

/// Loads the PNG at `path` scaled to `FRAME_WIDTH` pixels wide.
///
/// # Errors
/// Fails when the image cannot be read.
fn load_frame(path: &Path) -> Result<RgbaImage> {
    let image = image::open(path)
        .with_context(|| format!("Reading {}", path.display()))?
        .to_rgba8();
    let height = (u64::from(image.height()) * u64::from(FRAME_WIDTH) / u64::from(image.width()))
        .max(1) as u32;
    Ok(imageops::resize(
        &image,
        FRAME_WIDTH,
        height,
        FilterType::Triangle,
    ))
}

/// Writes `frames` to `path` as a looping animated PNG, which is lossless and plays inline in
/// a GitHub comment.
///
/// # Errors
/// Fails when the file cannot be written or the frames differ in size.
fn write_animation(frames: &[RgbaImage], path: &Path) -> Result<()> {
    let first = frames.first().context("No frames to animate")?;
    ensure!(
        frames
            .iter()
            .all(|frame| frame.dimensions() == first.dimensions()),
        "Animation frames differ in size"
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(file, first.width(), first.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_animated(u32::try_from(frames.len())?, 0)?;
    encoder.set_frame_delay(FRAME_DELAY_CENTISECONDS, 100)?;
    let mut writer = encoder.write_header()?;
    for frame in frames {
        writer.write_image_data(frame.as_raw())?;
    }
    writer.finish()?;
    Ok(())
}

/// Results of rendering every focal plane with one build.
struct Series {
    /// Float result of each focal plane.
    exrs: Vec<PathBuf>,
    /// Scaled preview of each focal plane.
    frames: Vec<RgbaImage>,
}

/// Renders every focal plane with the CLI at `cli` into `folder`.
///
/// # Errors
/// Fails when a render or a preview cannot be read.
fn render_series(cli: &Path, workloads: &[Workload], folder: &Path) -> Result<Series> {
    let mut series = Series {
        exrs: Vec::new(),
        frames: Vec::new(),
    };
    for workload in workloads {
        let (exr, png) = render_once(cli, workload, folder)?;
        series.exrs.push(exr);
        series.frames.push(load_frame(&png)?);
    }
    Ok(series)
}

/// Writes the frames at the ping-pong order of `count` frames to `out`/`name`.
///
/// # Errors
/// Fails when the animation cannot be written.
fn write_looped(frames: &[RgbaImage], out: &Path, name: &str) -> Result<()> {
    let ordered: Vec<RgbaImage> = ping_pong(frames.len())
        .into_iter()
        .map(|index| frames[index].clone())
        .collect();
    write_animation(&ordered, &out.join(name))
}

/// Renders the sweep with the head build, and with the base build when one is given, and
/// writes the animations below `out`.
///
/// # Errors
/// Fails when a render fails or the animations cannot be written.
pub fn run(root: &Path, head: &Path, base: Option<&Path>, out: &Path) -> Result<Sweep> {
    let work = out.join("sweep_work");
    let workloads = focal_planes()
        .into_iter()
        .enumerate()
        .map(|(index, focal)| workload(root, focal, &work.join("settings"), index))
        .collect::<Result<Vec<_>>>()?;
    println!("Rendering the focal sweep");
    let head_series = render_series(head, &workloads, &work.join("head"))?;
    write_looped(&head_series.frames, out, HEAD_ANIMATION)?;
    let mut max_difference = None;
    if let Some(base) = base {
        let base_series = render_series(base, &workloads, &work.join("base"))?;
        write_looped(&base_series.frames, out, BASE_ANIMATION)?;
        let mut differences = Vec::new();
        let mut largest = 0.0f32;
        for (index, (base_exr, head_exr)) in
            base_series.exrs.iter().zip(&head_series.exrs).enumerate()
        {
            let png = work.join("diff").join(format!("focal_{index:02}.png"));
            largest = largest.max(compare::compare(base_exr, head_exr, &png)?.max_abs);
            differences.push(load_frame(&png)?);
        }
        write_looped(&differences, out, DIFFERENCE_ANIMATION)?;
        max_difference = Some(largest);
    }
    fs::remove_dir_all(&work).ok(); // scratch renders only; leaving them behind is harmless
    Ok(Sweep {
        focal_steps: FOCAL_STEPS,
        max_difference,
    })
}

/// Tests for the sweep helpers.
#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// Verifies that the focal planes include both ends and rise evenly.
    #[test]
    fn test_focal_planes_span_the_range() {
        let planes = focal_planes();
        assert_eq!(planes.len(), FOCAL_STEPS);
        assert!((planes[0] - FOCAL_FIRST).abs() < 1e-6);
        assert!((planes[FOCAL_STEPS - 1] - FOCAL_LAST).abs() < 1e-5);
        assert!(planes.windows(2).all(|pair| pair[1] > pair[0]));
    }

    /// Verifies that playback goes forward and back without repeating the end frames.
    #[test]
    fn test_ping_pong_does_not_repeat_end_frames() {
        assert_eq!(ping_pong(4), vec![0, 1, 2, 3, 2, 1]);
        assert_eq!(ping_pong(1), vec![0]);
        assert_eq!(ping_pong(0), Vec::<usize>::new());
    }

    /// Verifies that an animation is written and mismatched frames are rejected.
    #[test]
    fn test_animation_is_written_and_checked() {
        let folder = tempfile::tempdir().unwrap();
        let frame = RgbaImage::from_pixel(4, 2, Rgba([1, 2, 3, 255]));
        let path = folder.path().join("a.png");
        write_animation(&[frame.clone(), frame.clone()], &path).unwrap();
        assert!(fs::metadata(&path).unwrap().len() > 0);
        let other = RgbaImage::new(2, 2);
        assert!(write_animation(&[frame, other], &path).is_err());
        assert!(write_animation(&[], &path).is_err());
    }
}
