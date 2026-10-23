//! Reuses Legacy reference renders from a prior QA run. A cached render is only used after
//! its source, settings, device and dimensions have been checked against the current run.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::{
    case::Setting,
    context::Context,
    exr::read_exr,
    fields::{field, opt_str},
    image::Rgba,
    output::safe_name,
    settings::overlay,
};
use crate::util::{crate_root, sha256_hex};

/// Identity of one cached Legacy row: case name, focal plane, size, max size and lens.
type CacheKey = (String, f64, f64, f64, String);

/// Legacy rows of a prior run's `summary.json`, looked up by case, setting and lens.
#[derive(Default)]
pub struct CachedRows {
    /// Pairs of row key and row, in file order.
    entries: Vec<(CacheKey, Value)>,
}

/// Result of looking up a cached Legacy reference.
pub enum Lookup {
    /// A usable cached render: its report and its decoded RGBA image.
    Hit {
        /// Report of the cached Legacy render.
        report: Value,
        /// Decoded RGBA image of the cached render.
        rgba: Rgba,
    },
    /// The cache cannot supply the reference; the text says why.
    Miss(String),
}

impl CachedRows {
    /// Stores `row` under `key`, replacing any earlier row with the same key.
    fn insert(&mut self, key: CacheKey, row: Value) {
        match self
            .entries
            .iter_mut()
            .find(|(existing, _)| *existing == key)
        {
            Some(entry) => entry.1 = row,
            None => self.entries.push((key, row)),
        }
    }

    /// Returns the row stored under `key`, if there is one.
    fn find(&self, key: &CacheKey) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, row)| row)
    }
}

/// Read the rows of a prior run's `summary.json` that can supply Legacy references.
pub fn load_rows(directory: &Path) -> Result<CachedRows> {
    let directory = directory.canonicalize()?;
    if directory.starts_with(crate_root().canonicalize()?) {
        bail!(
            "Cached reference directory must be outside the repository: {}",
            directory.display()
        );
    }
    let summary = directory.join("summary.json");
    if !summary.is_file() {
        bail!("Cached reference summary is missing: {}", summary.display());
    }
    let document: Value = serde_json::from_str(&fs::read_to_string(&summary)?)?;
    let mut rows = CachedRows::default();
    for row in document
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let focal = row.get("focal_plane").and_then(Value::as_f64);
        let size = row.get("size").and_then(Value::as_f64);
        let max_size = row.get("max_size").and_then(Value::as_f64);
        let (Some(focal), Some(size), Some(max_size)) = (focal, size, max_size) else {
            continue;
        };
        let name = row
            .get("name")
            .or_else(|| row.get("case"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let lens = opt_str(row, "lens").unwrap_or("plain").to_owned();
        rows.insert((name, focal, size, max_size, lens), row.clone());
    }
    Ok(rows)
}

/// Load a prior Legacy result only when its source, settings and device match this run.
pub fn cached_legacy_reference(
    ctx: &Context,
    case: &Value,
    setting: Setting,
    lens_name: &str,
    lens: Option<&Value>,
    rows: &CachedRows,
    expected_device: &str,
) -> Result<Lookup> {
    let name = opt_str(case, "name").unwrap_or_default().to_owned();
    let key = (
        name.clone(),
        setting.focal,
        setting.size,
        setting.max_size,
        lens_name.to_owned(),
    );
    let Some(row) = rows.find(&key) else {
        return Ok(Lookup::Miss(
            "no matching case/settings row in cached summary".to_owned(),
        ));
    };
    let Some(image_text) = row
        .get("legacy_report")
        .and_then(|report| report.get("output"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    else {
        return Ok(Lookup::Miss("cached row has no Legacy output".to_owned()));
    };
    let image_path = PathBuf::from(image_text);
    let report_path = image_path.with_extension("json");
    let settings_path = image_path.with_extension("settings.json");
    if !(image_path.is_file() && report_path.is_file() && settings_path.is_file()) {
        return Ok(Lookup::Miss(
            "cached Legacy image/report/settings are incomplete".to_owned(),
        ));
    }
    let report: Value = serde_json::from_str(&fs::read_to_string(&report_path)?)?;
    if report.get("algorithm") != Some(&json!("legacy"))
        || report.get("backend") != Some(&json!("gpu"))
    {
        return Ok(Lookup::Miss(
            "cached report is not a GPU Legacy render".to_owned(),
        ));
    }
    if report.get("device") != Some(&json!(expected_device)) {
        return Ok(Lookup::Miss(
            "cached Legacy device differs from the current Adaptive device".to_owned(),
        ));
    }
    let (width, height) = dimensions(case)?;
    if report.get("width") != Some(&json!(width)) || report.get("height") != Some(&json!(height)) {
        return Ok(Lookup::Miss("cached Legacy dimensions differ".to_owned()));
    }
    let quality = json!(ctx.args.quality.level());
    let render_settings = report
        .get("settings")
        .and_then(|settings| settings.get("render"));
    let quality_matches = render_settings.is_some_and(|render| {
        render.get("quality") == Some(&quality) && render.get("farm_quality") == Some(&quality)
    });
    if !quality_matches {
        return Ok(Lookup::Miss("cached Legacy quality differs".to_owned()));
    }
    let center = ctx
        .center_override()
        .unwrap_or((width as f64 / 2.0, height as f64 / 2.0));
    let expected_settings = overlay(
        opt_str(case, "mode").context("manifest case has no mode")?,
        setting.focal,
        setting.size,
        setting.max_size,
        lens,
        center,
        case.get("coc_math").and_then(Value::as_i64),
    );
    let cached_settings: Value = serde_json::from_str(&fs::read_to_string(&settings_path)?)?;
    if cached_settings != expected_settings {
        return Ok(Lookup::Miss(
            "cached Legacy settings overlay differs".to_owned(),
        ));
    }
    for (source_key, current_key) in [("input", "color"), ("depth", "depth")] {
        let cached_path = report
            .get(source_key)
            .and_then(Value::as_str)
            .map(PathBuf::from);
        let Some(cached_path) = cached_path.filter(|path| path.is_file()) else {
            return Ok(Lookup::Miss(format!(
                "cached Legacy {source_key} source is missing"
            )));
        };
        let current =
            PathBuf::from(opt_str(case, current_key).context("manifest case has no source path")?);
        if sha256_hex(&fs::read(&current)?) != sha256_hex(&fs::read(&cached_path)?) {
            return Ok(Lookup::Miss(format!(
                "cached Legacy {source_key} content differs"
            )));
        }
    }
    let channel_prefix = ctx
        .output
        .join(format!("cached-{}.channel", safe_name(&name)));
    let rgba = match read_exr(&ctx.args.exr_dump, &image_path, &channel_prefix) {
        Ok(rgba) => rgba,
        Err(error) => {
            return Ok(Lookup::Miss(format!(
                "cached Legacy EXR failed validation: {error}"
            )));
        }
    };
    if rgba.shape() != (height, width) {
        return Ok(Lookup::Miss(
            "cached Legacy EXR dimensions differ".to_owned(),
        ));
    }
    let mut report = report;
    let qa = report
        .as_object_mut()
        .map(|object| object.entry("qa").or_insert_with(|| json!({})));
    if let Some(Value::Object(qa)) = qa {
        qa.insert("cached_reference".to_owned(), json!(true));
        qa.insert(
            "cache_source".to_owned(),
            json!(image_path.display().to_string()),
        );
    }
    Ok(Lookup::Hit { report, rgba })
}

/// Reads the `dimensions` pair of a manifest case. Fails when the field is missing or the
/// width or height is not an unsigned integer.
fn dimensions(case: &Value) -> Result<(usize, usize)> {
    let dims = field(case, "dimensions")?
        .as_array()
        .context("dimensions must be an array")?;
    let width = dims.first().and_then(Value::as_u64).context("width")? as usize;
    let height = dims.get(1).and_then(Value::as_u64).context("height")? as usize;
    Ok((width, height))
}
