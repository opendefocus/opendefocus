//! Static lens-control variants of the QA sweep and the per-case override of the
//! inverse-foreground flag.
use serde_json::{Value, json};

/// Static lens-control variants swept with `--lens-sweep`, in reference order.
pub fn variants() -> Vec<(&'static str, Value)> {
    vec![
        (
            "catseye",
            json!({"non_uniform": {
                "inverse_foreground": true,
                "catseye": {
                    "enable": true,
                    "amount": 0.65,
                    "gamma": 1.0,
                    "softness": 0.2,
                    "inverse": false,
                    "inverse_foreground": true,
                    "relative_to_screen": false,
                },
            }}),
        ),
        (
            "barndoors",
            json!({"non_uniform": {
                "inverse_foreground": true,
                "barndoors": {
                    "enable": true,
                    "amount": 0.65,
                    "gamma": 1.0,
                    "inverse": false,
                    "inverse_foreground": true,
                    "top": 55.0,
                    "bottom": 100.0,
                    "left": 100.0,
                    "right": 100.0,
                },
            }}),
        ),
        (
            "astigmatism",
            json!({"non_uniform": {"astigmatism": {"enable": true, "amount": 0.65, "gamma": 1.0}}}),
        ),
        (
            "axial_aberration",
            json!({"non_uniform": {"axial_aberration": {"enable": true, "amount": 0.65, "color_type": 0}}}),
        ),
        (
            "inverse_foreground_off",
            json!({"non_uniform": {
                "inverse_foreground": false,
                "catseye": {
                    "enable": true,
                    "amount": 0.65,
                    "gamma": 1.0,
                    "softness": 0.2,
                    "inverse": false,
                    "inverse_foreground": false,
                    "relative_to_screen": false,
                },
            }}),
        ),
    ]
}

/// Apply a per-fixture `inverse_foreground` flag on top of the lens controls.
pub fn lens_for_case(case: &Value, lens: Option<&Value>) -> Option<Value> {
    let Some(inverse) = case.get("inverse_foreground") else {
        return lens.cloned();
    };
    let mut effective = lens.cloned().unwrap_or_else(|| json!({}));
    let flag = super::fields::truthy(Some(inverse));
    let non_uniform = effective
        .as_object_mut()
        .map(|controls| controls.entry("non_uniform").or_insert_with(|| json!({})));
    if let Some(controls) = non_uniform.and_then(Value::as_object_mut) {
        controls.insert("inverse_foreground".to_owned(), json!(flag));
    }
    Some(effective)
}
