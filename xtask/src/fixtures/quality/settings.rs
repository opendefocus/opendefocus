//! Builds the CLI settings overlay that configures one analytic render from a manifest case.
use serde_json::{Value, json};

/// Build the CLI settings overlay for one analytic render.
pub fn overlay(
    mode: &str,
    focal: f64,
    size: f64,
    max_size: f64,
    lens: Option<&Value>,
    center: (f64, f64),
    coc_math: Option<i64>,
) -> Value {
    let math = coc_math.unwrap_or(i64::from(mode != "real_depth"));
    let mut settings = json!({
        "defocus": {
            "use_direct_math": mode == "direct_coc",
            "use_camera_focal": false,
            "circle_of_confusion": {
                "math": math,
                "focal_plane": focal,
                "size": size,
                "max_size": max_size,
                "pixel_aspect": 1.0,
            },
        },
        "render": { "center": { "x": center.0, "y": center.1 } },
    });
    if let (Some(entries), Some(target)) =
        (lens.and_then(Value::as_object), settings.as_object_mut())
    {
        for (key, value) in entries {
            target.insert(key.clone(), value.clone());
        }
    }
    settings
}
