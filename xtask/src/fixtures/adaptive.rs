//! Float EXR adaptive-render fixtures and `manifest.json`, written outside the checkout.

mod cases;
mod controls;
mod scene;
mod sequences;
mod write;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{fixtures::scenes::json::Json, util::crate_root};

/// Options of `cargo xtask fixtures adaptive`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Empty directory outside the repository for the EXR fixtures and manifest.json
    #[arg(long)]
    output_dir: PathBuf,
}

/// Writes the adaptive fixtures and `manifest.json` into `output_dir`, which must be absent
/// or empty and outside the repository. Prints the manifest path. Fails when the directory
/// is inside the repository or not empty, or when a file cannot be written.
pub fn run(args: Args) -> Result<()> {
    let Args { output_dir } = args;
    let checkout = crate_root()
        .canonicalize()
        .context("resolving repository root")?;
    let output = resolve_output(&output_dir)?;
    if output == checkout || output.starts_with(&checkout) {
        bail!(
            "Fixture output must be outside the repository: {}",
            output.display()
        );
    }
    if output.exists() && fs::read_dir(&output)?.next().is_some() {
        bail!(
            "Fixture output directory must be empty: {}",
            output.display()
        );
    }
    fs::create_dir_all(&output).with_context(|| format!("creating {}", output.display()))?;
    let manifest = manifest(&output, &checkout)?;
    let manifest_path = output.join("manifest.json");
    fs::write(&manifest_path, format!("{}\n", manifest.to_pretty_string()))
        .with_context(|| format!("writing {}", manifest_path.display()))?;
    println!("{}", manifest_path.display());
    Ok(())
}

/// Resolve symlinks in the existing prefix of `requested`, as Python's `Path.resolve()` does.
fn resolve_output(requested: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(requested)
        .with_context(|| format!("resolving {}", requested.display()))?;
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                existing = parent;
            }
            _ => break,
        }
    }
    let mut resolved = existing
        .canonicalize()
        .with_context(|| format!("resolving {}", existing.display()))?;
    resolved.extend(missing.iter().rev());
    Ok(resolved)
}

/// Builds the manifest: writes the EXR inputs of every case under `root` and records their
/// expected results, including the motion sequences and the bundled Nuke case under
/// `checkout`. Fails when a case cannot be written.
fn manifest(root: &Path, checkout: &Path) -> Result<Json> {
    let mut cases = cases::scene_cases(root)?;
    cases.push(cases::focused_constant_case(root)?);
    cases.extend(cases::psf_cases(root)?);
    cases.extend(controls::lens_cases(root)?);
    cases.extend(controls::focus_and_edge_cases(root)?);
    cases.push(bundled_nuke_case(checkout));
    Ok(Json::Object(vec![
        ("schema", Json::Int(2)),
        ("dimensions", Json::ints(&[192, 128])),
        (
            "storage",
            Json::str("OpenEXR FLOAT RGBA, linear sRGB, premultiplied alpha"),
        ),
        (
            "direct_coc_sign",
            Json::str("positive near/foreground, negative far/background"),
        ),
        (
            "provenance",
            Json::str(
                "Generated from standard-library analytic scenes shared with cargo xtask fixtures scenes; also references the bundled Nuke 1/Z preprocessor scene for a focal-plane seam regression.",
            ),
        ),
        ("cases", Json::List(cases)),
        ("motion_sequence", Json::List(sequences::motion(root)?)),
        (
            "focus_ramp_sequence",
            Json::List(sequences::focus_ramp(root)?),
        ),
        (
            "focused_occlusion_sequence",
            Json::List(sequences::focused_occlusion(root)?),
        ),
        (
            "moving_point_sequence",
            Json::List(sequences::moving_point(root)?),
        ),
        (
            "large_point_motion_sequence",
            Json::List(sequences::large_point_motion(root)?),
        ),
        (
            "colored_point_motion_sequence",
            Json::List(sequences::colored_point_motion(root)?),
        ),
    ]))
}

/// Manifest entry for the bundled Nuke 1/Z preprocessor scene in
/// `checkout/test/images/preprocessor`. It regresses the zero-depth column artifact at focal
/// planes 0, 10 and 15. The entry reuses the committed image and depth files.
fn bundled_nuke_case(checkout: &Path) -> Json {
    let preprocessor = checkout.join("test/images/preprocessor");
    Json::Object(vec![
        ("name", Json::str("bundled-nuke-1z")),
        (
            "color",
            Json::str(preprocessor.join("image.jpg").to_string_lossy()),
        ),
        (
            "depth",
            Json::str(preprocessor.join("depth.exr").to_string_lossy()),
        ),
        ("mode", Json::str("inverse_depth")),
        ("coc_math", Json::Int(1)),
        ("dimensions", Json::ints(&[1920, 1080])),
        ("focus_planes", Json::floats(&[0.0, 10.0, 15.0])),
        ("sizes", Json::floats(&[10.0])),
        ("zero_depth_seam_x", Json::Int(1379)),
        ("gate", Json::str("zero_depth_seam")),
        (
            "purpose",
            Json::str(
                "regress the bundled Nuke 1/Z zero-depth-column artifact at focal 0, 10, and 15",
            ),
        ),
    ])
}
