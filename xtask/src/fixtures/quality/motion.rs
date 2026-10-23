//! Renders the frames of a temporal QA sequence, carries the state of each frame into the next,
//! and records every frame's fields and gates.
use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Map, Value, json};

use super::{
    case::case_dimensions,
    context::Context,
    fields::{field, str_field},
    gates::Gate,
    image::Rgba,
    motion_gates,
    output::safe_name,
    render::{Algorithm, RenderSpec, Rendered, run_render},
};

/// One rendered motion frame: its report fields plus the gates it produced.
#[derive(Default)]
pub struct Frame {
    /// Report fields of the frame, written to the summary.
    pub fields: Map<String, Value>,
    /// Present once a frame reaches its gate kind; absent when it only failed to render.
    pub gates: Option<Vec<Gate>>,
    /// Single gate of a frame that has one gate instead of a list, such as a frame whose render
    /// failed.
    pub gate: Option<Gate>,
}

impl Frame {
    /// Converts the frame to a JSON object. Its gate list and single gate are written under
    /// `gates` and `gate` when present.
    pub fn to_value(&self) -> Value {
        let mut fields = self.fields.clone();
        if let Some(gates) = &self.gates {
            fields.insert(
                "gates".to_owned(),
                Value::Array(gates.iter().map(Gate::to_value).collect()),
            );
        }
        if let Some(gate) = &self.gate {
            fields.insert("gate".to_owned(), gate.to_value());
        }
        Value::Object(fields)
    }
}

/// Images rendered for one frame, keyed by algorithm.
pub struct Images {
    /// Adaptive render of the frame.
    pub adaptive: Rgba,
    /// Legacy render of the frame, when the legacy CLI is available.
    pub legacy: Option<Rgba>,
}

/// State carried from the previous successfully rendered frame.
#[derive(Default)]
pub struct Previous {
    /// Legacy render of the previous frame.
    pub legacy: Option<Rgba>,
    /// Adaptive render of the previous frame.
    pub adaptive: Option<Rgba>,
    /// Whole-frame translation of the previous frame, in pixels.
    pub translation_px: f64,
    /// Energy of the previous frame's point, used by the continuity gate.
    pub point_energy: Option<f64>,
    /// Energy of each colour channel of the previous frame's tracks, by name.
    pub channel_energy: Vec<(String, f64)>,
}

/// Render every frame of the selected sequence and evaluate its temporal gates.
pub fn run(ctx: &Context, cases: &[Value]) -> Result<Vec<Frame>> {
    let mut frames = Vec::new();
    let mut previous = Previous::default();
    for case in cases {
        let label = safe_name(str_field(case, "name")?);
        let dimensions = case_dimensions(case)?;
        let translation = case
            .get("translation_px")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let color = PathBuf::from(str_field(case, "color")?);
        let depth = PathBuf::from(str_field(case, "depth")?);
        let mut frame = Frame::default();
        frame
            .fields
            .insert("frame".to_owned(), field(case, "frame")?.clone());
        frame
            .fields
            .insert("translation_px".to_owned(), json!(translation));
        let mut reports = Map::new();
        let mut errors = Map::new();
        let mut rendered: Vec<(&'static str, Rendered)> = Vec::new();
        let algorithms: &[(&str, Algorithm)] = if ctx.legacy_cli().is_some() {
            &[
                ("legacy", Algorithm::Legacy),
                ("adaptive", Algorithm::Adaptive),
            ]
        } else {
            &[("adaptive", Algorithm::Adaptive)]
        };
        for (name, algorithm) in algorithms {
            let spec = RenderSpec {
                mode: "direct_coc",
                focal: 0.0,
                size: 24.0,
                max_size: ctx.args.max_size,
                label: format!("{name}-{label}"),
                lens: None,
                algorithm: *algorithm,
                dimensions,
                coc_math: None,
            };
            match run_render(ctx, &color, &depth, &spec) {
                Ok(result) => {
                    reports.insert((*name).to_owned(), result.report.clone());
                    rendered.push((name, result));
                }
                Err(error) => {
                    errors.insert((*name).to_owned(), json!(format!("{error:#}")));
                }
            }
        }
        frame
            .fields
            .insert("reports".to_owned(), Value::Object(reports));
        frame
            .fields
            .insert("execution_errors".to_owned(), Value::Object(errors.clone()));
        let adaptive = rendered
            .iter()
            .position(|(name, _)| *name == "adaptive")
            .map(|index| rendered.swap_remove(index).1.rgba);
        let Some(adaptive) = adaptive else {
            frame.gate = Some(Gate::new(
                "adaptive_motion_frame_rendered",
                errors.get("adaptive").cloned().unwrap_or(Value::Null),
                json!("successful finite render"),
                false,
            ));
            frames.push(frame);
            continue;
        };
        let legacy = rendered
            .into_iter()
            .find(|(name, _)| *name == "legacy")
            .map(|(_, result)| result.rgba);
        let images = Images { adaptive, legacy };
        let kind = str_field(case, "gate").ok();
        let mut next = Previous {
            legacy: images.legacy.clone(),
            adaptive: Some(images.adaptive.clone()),
            translation_px: translation,
            ..Previous::default()
        };
        match kind {
            Some("focus_occlusion") => {
                motion_gates::focus_occlusion(ctx, case, &label, &images, &mut frame)?;
            }
            Some("focus_motion") => {
                motion_gates::focus_motion(ctx, case, &images, &previous, &mut frame)?;
            }
            Some("motion_point") => {
                let energy = motion_gates::motion_point(
                    ctx,
                    case,
                    &images,
                    &previous,
                    translation,
                    &mut frame,
                )?;
                next.point_energy = Some(energy);
            }
            Some("motion_channel_grid") => {
                let energies = motion_gates::motion_channel_grid(
                    case,
                    &images,
                    &previous,
                    translation,
                    &mut frame,
                )?;
                next.channel_energy = energies;
            }
            Some("motion_disk") => motion_gates::motion_disk(&images, &mut frame),
            _ => {}
        }
        let temporal = matches!(
            kind,
            Some("focus_motion" | "focus_occlusion" | "motion_point" | "motion_channel_grid")
        );
        if !temporal && previous.adaptive.is_some() {
            motion_gates::transport_boundary(&images, &previous, translation, &mut frame)?;
        }
        frames.push(frame);
        previous = next;
    }
    Ok(frames)
}
