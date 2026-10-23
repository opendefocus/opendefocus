//! Reads the fixture manifest and selects the cases of a QA run, resolving their input paths
//! and applying the `--case` filter.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::{args::Args, fields::field};

/// Cases chosen for one run and where they were read from.
pub struct Selection {
    /// Canonical path of the manifest the cases were read from.
    pub manifest_path: PathBuf,
    /// Name of the manifest array the selection was read from; also the motion summary key.
    pub sequence_key: &'static str,
    /// Manifest cases to run, with their input paths resolved.
    pub cases: Vec<Value>,
}

/// Read the fixture manifest and return its canonical path with the parsed document.
pub fn load(path: &Path) -> Result<(PathBuf, Value)> {
    let manifest_path = path
        .canonicalize()
        .with_context(|| format!("manifest not found: {}", path.display()))?;
    let document = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
    Ok((manifest_path, document))
}

/// Choose the manifest array for the requested mode, resolve its inputs and apply `--case`.
pub fn select(args: &Args, manifest: &Value, manifest_path: &Path) -> Result<Selection> {
    let sequence_key = sequence_name(args);
    let array = field(manifest, sequence_key)?
        .as_array()
        .with_context(|| format!("manifest `{sequence_key}` must be an array"))?;
    let base = manifest_path
        .parent()
        .context("manifest path has no parent directory")?;
    let mut cases = Vec::with_capacity(array.len());
    for case in array {
        let mut case = case.clone();
        for key in ["color", "depth"] {
            let relative = field(&case, key)?
                .as_str()
                .context("manifest input path must be a string")?;
            let resolved = base.join(relative);
            let resolved = resolved.canonicalize().unwrap_or(resolved);
            case[key] = Value::String(resolved.display().to_string());
        }
        cases.push(case);
    }
    let cases = if args.case.is_empty() {
        cases
    } else {
        filter_by_name(cases, &args.case)?
    };
    Ok(Selection {
        manifest_path: manifest_path.to_path_buf(),
        sequence_key,
        cases,
    })
}

/// Manifest array that a temporal flag selects; `cases` for the static matrix.
pub fn sequence_name(args: &Args) -> &'static str {
    if args.motion {
        "motion_sequence"
    } else if args.motion_points {
        "moving_point_sequence"
    } else if args.motion_large_points {
        "large_point_motion_sequence"
    } else if args.motion_colored_grid {
        "colored_point_motion_sequence"
    } else if args.focus_occlusion {
        "focused_occlusion_sequence"
    } else if args.focus_ramp {
        "focus_ramp_sequence"
    } else {
        "cases"
    }
}

/// Keeps the cases whose `name` is in `selected`, in manifest order. Fails when none match,
/// and names the requested cases in the error.
fn filter_by_name(cases: Vec<Value>, selected: &[String]) -> Result<Vec<Value>> {
    let kept: Vec<Value> = cases
        .into_iter()
        .filter(|case| {
            case.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| selected.iter().any(|wanted| wanted == name))
        })
        .collect();
    if kept.is_empty() {
        let mut names = selected.to_vec();
        names.sort();
        bail!("No requested case in manifest: {names:?}");
    }
    Ok(kept)
}
