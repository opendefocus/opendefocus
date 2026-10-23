//! Provenance of a QA run: the hashes of the CLIs, manifest, inputs and settings, and the
//! camera requirements of each case.
use std::{fs, path::Path};

use anyhow::{Context as _, Result};
use serde_json::{Map, Value, json};

use super::{context::Context, fields::opt_str, manifest::Selection, process::sha256_file};

/// Hashes and identities of every input that shaped the run.
pub fn build(ctx: &Context, manifest: &Value, selection: &Selection) -> Result<Value> {
    let args = &ctx.args;
    let mut input_hashes = Map::new();
    let mut per_case = Map::new();
    for case in &selection.cases {
        let name = opt_str(case, "name").unwrap_or_default().to_owned();
        let color = opt_str(case, "color").context("case color path")?;
        let depth = opt_str(case, "depth").context("case depth path")?;
        input_hashes.insert(
            name.clone(),
            json!({
                "color": sha256_file(Path::new(color))?,
                "depth": sha256_file(Path::new(depth))?,
            }),
        );
        per_case.insert(name, requirements(case));
    }
    Ok(json!({
        "adaptive_cli": canonical(&args.cli)?,
        "adaptive_cli_sha256": sha256_file(&args.cli)?,
        "source_revision": args.source_revision,
        "legacy_cli": match &args.legacy_cli {
            Some(path) => json!(canonical(path)?),
            None => Value::Null,
        },
        "legacy_cli_sha256": match &args.legacy_cli {
            Some(path) => json!(sha256_file(path)?),
            None => Value::Null,
        },
        "manifest_sha256": sha256_file(&selection.manifest_path)?,
        "input_sha256": input_hashes,
        "settings_sha256": settings_hashes(&ctx.output)?,
        "direct_coc_sign": manifest.get("direct_coc_sign").cloned().unwrap_or(Value::Null),
        "camera_requirements": {
            "manifest_provenance": manifest.get("provenance").cloned().unwrap_or_else(|| json!({})),
            "per_case": per_case,
        },
        "quality": args.quality.name(),
        "backend": "gpu",
    }))
}

/// Returns the absolute path of `path` with symbolic links resolved, as text. Fails when the
/// path does not exist.
fn canonical(path: &Path) -> Result<String> {
    Ok(path.canonicalize()?.display().to_string())
}

/// Returns the fields of `case` that define its render requirements (mode, CoC math, focal
/// planes, sizes, sign and radius). Missing fields are left out.
fn requirements(case: &Value) -> Value {
    const KEYS: [&str; 6] = [
        "mode",
        "coc_math",
        "focus_planes",
        "sizes",
        "sign",
        "radius",
    ];
    let mut selected = Map::new();
    for key in KEYS {
        if let Some(value) = case.get(key) {
            selected.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(selected)
}

/// SHA-256 of every `*.settings.json` written into the run directory, keyed by file name.
fn settings_hashes(output: &Path) -> Result<Map<String, Value>> {
    let mut paths: Vec<_> = fs::read_dir(output)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    paths.sort();
    let mut hashes = Map::new();
    for path in paths {
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if name.ends_with(".settings.json") {
            hashes.insert(name, json!(sha256_file(&path)?));
        }
    }
    Ok(hashes)
}
