//! Converts the settings recorded in a parity entry into the parameter values the plugin reads.

use super::*;

/// Flattens nested objects into (dotted path, leaf) pairs.
pub(super) fn flatten(value: &Value, path: &str, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                flatten(child, &child_path, out);
            }
        }
        leaf => out.push((path.to_owned(), leaf.clone())),
    }
}

/// Converts a JSON leaf to the host representation of the named parameter.
pub(super) fn to_param(name: &str, value: &Value) -> Result<ParamValue, String> {
    let def = PARAMS
        .iter()
        .find(|def| def.name == name)
        .ok_or_else(|| format!("no parameter named {name}"))?;
    let converted = match def.kind {
        ParamKind::Double { .. } => value.as_f64().map(ParamValue::Double),
        ParamKind::Bool { .. } => value.as_bool().map(ParamValue::Bool),
        ParamKind::Int { .. } => value
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .map(ParamValue::Int),
        ParamKind::Choice { .. } => value
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .map(ParamValue::Choice),
    };
    converted.ok_or_else(|| format!("{name} cannot take the JSON value {value}"))
}

/// Parameter values for one parity entry, or the reasons the table cannot express it.
///
/// Every JSON leaf is either mapped through [`EXPOSED`] or must equal the value of
/// `Settings::default()`. A leaf that differs and is not mapped is an unexposed setting.
pub(super) fn entry_params(
    entry: &Entry,
    settings: &Value,
) -> Result<HashMap<&'static str, ParamValue>, Vec<String>> {
    let mut values: HashMap<&'static str, ParamValue> = PARAMS
        .iter()
        .map(|def| (def.name, def.default_value()))
        .collect();
    let defaults_value = serde_json::to_value(Settings::default()).expect("default settings");
    let mut defaults = Vec::new();
    flatten(&defaults_value, "", &mut defaults);
    let defaults: HashMap<String, Value> = defaults.into_iter().collect();

    let camera = settings
        .pointer("/defocus/circle_of_confusion/camera_data")
        .is_some_and(|camera| !camera.is_null());
    let mut leaves = Vec::new();
    flatten(settings, "", &mut leaves);

    let mut problems = Vec::new();
    if camera {
        values.insert("cameraEnabled", ParamValue::Bool(true));
    }
    let mut direct = false;
    for (path, leaf) in &leaves {
        if let Some((_, param)) = EXPOSED.iter().find(|(json, _)| *json == path.as_str()) {
            match to_param(param, leaf) {
                Ok(value) => {
                    values.insert(*param, value);
                }
                Err(reason) => problems.push(reason),
            }
            continue;
        }
        // Direct CoC is the third entry of the Math choice.
        if path == "defocus.use_direct_math" && leaf.as_bool() == Some(true) {
            direct = true;
            continue;
        }
        let default = defaults.get(path);
        let same_as_default = default == Some(leaf);
        let renders_the_same = match path.as_str() {
            // The renderer treats an unset proxy scale as 1.0.
            "defocus.proxy_scale" => leaf.as_f64() == Some(1.0),
            _ => same_as_default,
        };
        if !renders_the_same {
            problems.push(format!(
                "{path} = {leaf} is not exposed by the parameter table (default {})",
                default.map_or_else(|| "absent".to_owned(), ToString::to_string)
            ));
        }
    }
    if direct {
        values.insert("math", ParamValue::Choice(2));
    }
    // Same as the CLI: --radius overrides the JSON size, --quality sets both quality fields,
    // and --depth switches the defocus mode to Depth.
    values.insert("size", ParamValue::Double(f64::from(entry.radius)));
    values.insert("quality", ParamValue::Choice(entry.quality));
    if entry.depth.is_some() {
        values.insert("defocusMode", ParamValue::Choice(1));
    }
    if problems.is_empty() {
        Ok(values)
    } else {
        Err(problems)
    }
}
