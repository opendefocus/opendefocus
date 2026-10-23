//! Per-case data of the QA matrix: the analytic and render settings, the rows with their
//! gates, the lens selection, and the image dimensions of each case.
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde_json::{Map, Value, json};

use super::{
    cache::{CachedRows, Lookup, cached_legacy_reference},
    context::Context,
    fields::{field, str_field},
    format::format_g,
    gates::{Gate, analytic::analytic_gate},
    image::Rgba,
    lens::lens_for_case,
    lens_checks::lens_gates,
    metrics::{delta::image_delta, seam::zero_depth_seam_metrics},
    output::safe_name,
    render::{Algorithm, RenderSpec, Rendered, run_render},
};

/// One setting of a case: focal plane, size and maximum circle-of-confusion size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setting {
    /// Focal plane of the render, in the units of the case's depth.
    pub focal: f64,
    /// Size value passed to the renderer for this setting.
    pub size: f64,
    /// Maximum circle-of-confusion radius in pixels for this setting.
    pub max_size: f64,
}

/// Plain adaptive renders kept for lens-variant comparisons, keyed by case and setting.
#[derive(Default)]
pub struct PlainCache {
    /// Pairs of case and setting key with the plain render image.
    entries: Vec<(PlainKey, Rgba)>,
}

/// Key of the plain cache: case name and setting.
type PlainKey = (String, Setting);

impl PlainCache {
    /// Builds the cache key for case `name` and `setting`.
    fn key(name: &str, setting: Setting) -> PlainKey {
        (name.to_owned(), setting)
    }

    /// Stores `image` for case `name` and `setting`, replacing any earlier image with the same key.
    fn insert(&mut self, name: &str, setting: Setting, image: Rgba) {
        let key = Self::key(name, setting);
        match self
            .entries
            .iter_mut()
            .find(|(existing, _)| *existing == key)
        {
            Some(entry) => entry.1 = image,
            None => self.entries.push((key, image)),
        }
    }

    /// Returns the image stored for case `name` and `setting`, if there is one.
    fn get(&self, name: &str, setting: Setting) -> Option<&Rgba> {
        let key = Self::key(name, setting);
        self.entries
            .iter()
            .find(|(existing, _)| *existing == key)
            .map(|(_, image)| image)
    }
}

/// One matrix row: the fields of the Python row dict and its gates in order.
#[derive(Default)]
pub struct Row {
    /// Fields of the row, in the order they are written to the summary.
    pub fields: Map<String, Value>,
    /// Gate results of the row, in evaluation order.
    pub gates: Vec<Gate>,
}

impl Row {
    /// Converts the row to a JSON object with its gates under the `gates` key.
    pub fn into_value(self) -> Value {
        let mut fields = self.fields;
        fields.insert(
            "gates".to_owned(),
            Value::Array(self.gates.iter().map(Gate::to_value).collect()),
        );
        Value::Object(fields)
    }
}

/// A lens variant of a case: its name and its lens settings.
pub struct LensChoice<'a> {
    /// Lens name recorded in the summary; `plain` when no lens variant is applied.
    pub name: &'a str,
    /// Lens settings applied to the case's render.
    pub settings: &'a Value,
}

/// Returns the `[width, height]` of a manifest case, or 192 by 128 when the case has no
/// `dimensions` field. Fails when the field is malformed.
pub fn case_dimensions(case: &Value) -> Result<(usize, usize)> {
    match case.get("dimensions").and_then(Value::as_array) {
        Some(dims) => {
            let width = dims
                .first()
                .and_then(Value::as_u64)
                .context("dimensions width")? as usize;
            let height = dims
                .get(1)
                .and_then(Value::as_u64)
                .context("dimensions height")? as usize;
            Ok((width, height))
        }
        None => Ok((192, 128)),
    }
}

/// Run one manifest case at one setting, with an optional lens variant.
pub fn run_case(
    ctx: &Context,
    case: &Value,
    setting: Setting,
    lens: Option<&LensChoice>,
    cached: Option<&CachedRows>,
    plain_cache: &mut PlainCache,
) -> Result<Row> {
    let (lens_name, lens_settings) = match &lens {
        Some(choice) => (choice.name, Some(choice.settings)),
        None => ("plain", None),
    };
    let name = str_field(case, "name")?;
    let mode = str_field(case, "mode")?;
    let label = format!(
        "{}-f{}-s{}-m{}-{lens_name}",
        safe_name(name),
        format_g(setting.focal),
        format_g(setting.size),
        format_g(setting.max_size),
    );
    let image = PathBuf::from(str_field(case, "color")?);
    let depth = PathBuf::from(str_field(case, "depth")?);
    let dimensions = case_dimensions(case)?;
    let coc_math = case.get("coc_math").and_then(Value::as_i64);
    let effective_lens = lens_for_case(case, lens_settings);
    let adaptive_spec = RenderSpec {
        mode,
        focal: setting.focal,
        size: setting.size,
        max_size: setting.max_size,
        label: format!("adaptive-{label}"),
        lens: effective_lens.as_ref(),
        algorithm: Algorithm::Adaptive,
        dimensions,
        coc_math,
    };
    let adaptive = run_render(ctx, &image, &depth, &adaptive_spec)?;
    let device = adaptive
        .report
        .get("device")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let mut legacy: Option<Rendered> = None;
    let mut legacy_error = None;
    let mut cache_miss = None;
    if let Some(cached) = cached {
        match cached_legacy_reference(
            ctx,
            case,
            setting,
            lens_name,
            effective_lens.as_ref(),
            cached,
            device,
        )? {
            Lookup::Hit { report, rgba } => legacy = Some(Rendered { report, rgba }),
            Lookup::Miss(reason) => cache_miss = Some(reason),
        }
    }
    if ctx.legacy_cli().is_some() && legacy.is_none() {
        let legacy_spec = RenderSpec {
            label: format!("legacy-{label}"),
            algorithm: Algorithm::Legacy,
            coc_math,
            ..adaptive_spec
        };
        match run_render(ctx, &image, &depth, &legacy_spec) {
            Ok(rendered) => legacy = Some(rendered),
            Err(error) => legacy_error = Some(format!("{error:#}")),
        }
    }

    let analysis = analytic_gate(case, &adaptive.rgba, setting.max_size)?;
    if lens_name == "plain" {
        plain_cache.insert(name, setting, adaptive.rgba.clone());
    }
    let paired = legacy
        .as_ref()
        .map(|rendered| image_delta(&rendered.rgba, &adaptive.rgba))
        .transpose()?;

    let mut row = Row::default();
    let gate = case.get("gate").and_then(Value::as_str);
    let fields = &mut row.fields;
    fields.insert("case".to_owned(), json!(name));
    fields.insert("mode".to_owned(), json!(mode));
    fields.insert(
        "coc_math".to_owned(),
        json!(coc_math.unwrap_or(i64::from(mode != "real_depth"))),
    );
    fields.insert("focal_plane".to_owned(), json!(setting.focal));
    fields.insert("size".to_owned(), json!(setting.size));
    fields.insert("max_size".to_owned(), json!(setting.max_size));
    fields.insert("lens".to_owned(), json!(lens_name));
    fields.insert(
        "legacy_report".to_owned(),
        legacy
            .as_ref()
            .map_or(Value::Null, |rendered| rendered.report.clone()),
    );
    fields.insert("legacy_error".to_owned(), json!(legacy_error));
    fields.insert("legacy_cache_miss".to_owned(), json!(cache_miss));
    fields.insert("adaptive_report".to_owned(), adaptive.report.clone());
    fields.insert(
        "legacy_vs_adaptive".to_owned(),
        paired
            .as_ref()
            .map_or(Value::Null, super::metrics::delta::Delta::to_value),
    );
    fields.insert(
        "analytic_metrics".to_owned(),
        Value::Object(analysis.metrics.clone()),
    );
    row.gates = analysis.gates;

    if gate == Some("zero_depth_seam")
        && let Some(rendered) = &legacy
    {
        add_seam_diagnostic(case, rendered, &mut row)?;
    }
    if ctx.args.determinism && gate.is_some() {
        let again_spec = RenderSpec {
            label: format!("adaptive-{label}-again"),
            coc_math: None,
            ..adaptive_spec
        };
        add_determinism_gate(ctx, &image, &depth, &again_spec, &adaptive.rgba, &mut row)?;
    }
    if lens.is_some() {
        let plain = if let Some(plain) = plain_cache.get(name, setting) {
            plain.clone()
        } else {
            let plain_spec = RenderSpec {
                label: format!("adaptive-{label}-plain-reference"),
                lens: None,
                coc_math: None,
                ..adaptive_spec
            };
            run_render(ctx, &image, &depth, &plain_spec)?.rgba
        };
        lens_gates(
            ctx,
            case,
            lens_name,
            &adaptive.rgba,
            legacy.as_ref().map(|rendered| &rendered.rgba),
            &plain,
            dimensions,
            &mut row,
        )?;
    }
    Ok(row)
}

/// Records the legacy renderer's gradient measurements across the zero-depth seam column of the case.
///
/// # Errors
/// Returns an error when the case has no seam column or the seam cannot be measured.
fn add_seam_diagnostic(case: &Value, legacy: &Rendered, row: &mut Row) -> Result<()> {
    let seam_x = field(case, "zero_depth_seam_x")?
        .as_f64()
        .unwrap_or_default() as i64;
    let seam = zero_depth_seam_metrics(&legacy.rgba, seam_x)?;
    row.fields.insert(
        "legacy_zero_depth_seam_diagnostic".to_owned(),
        json!({
            "seam_gradient_mean": seam.gradient_mean,
            "seam_gradient_max": seam.gradient_max,
            "neighbor_gradient_median": seam.neighbor_gradient_median,
            "gradient_ratio": seam.gradient_ratio,
        }),
    );
    Ok(())
}

/// Renders the case a second time and gates on the two adaptive renders being identical.
///
/// # Errors
/// Returns an error when the repeat render fails or the images cannot be compared.
fn add_determinism_gate(
    ctx: &Context,
    image: &Path,
    depth: &Path,
    again_spec: &RenderSpec<'_>,
    first: &Rgba,
    row: &mut Row,
) -> Result<()> {
    let again = run_render(ctx, image, depth, again_spec)?;
    let difference = image_delta(first, &again.rgba)?;
    row.gates.push(Gate::new(
        "static_determinism",
        json!(difference.max_abs_rgba),
        json!(0.0),
        difference.max_abs_rgba == 0.0,
    ));
    row.fields
        .insert("determinism".to_owned(), difference.to_value());
    Ok(())
}
