//! Helpers that read typed values from manifest JSON. Errors name the key that is missing or
//! has the wrong type.
use anyhow::{Context, Result};
use serde_json::Value;

/// Returns the value stored under `key`. Fails when the key is missing.
pub fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .with_context(|| format!("missing manifest key `{key}`"))
}

/// Returns the number stored under `key`. Fails when the key is missing or not a number.
pub fn f64_field(value: &Value, key: &str) -> Result<f64> {
    field(value, key)?
        .as_f64()
        .with_context(|| format!("manifest key `{key}` is not a number"))
}

/// Returns the string stored under `key`. Fails when the key is missing or not a string.
pub fn str_field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .with_context(|| format!("manifest key `{key}` is not a string"))
}

/// Python-style truthiness for an optional JSON field.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(map)) => !map.is_empty(),
    }
}

/// Returns the number stored under `key`, or `None` when it is absent or not a number.
pub fn opt_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

/// Returns the string stored under `key`, or `None` when it is absent or not a string.
pub fn opt_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Read a two-element numeric array such as `xy` or `center_xy`.
pub fn pair(value: &Value) -> Result<(f64, f64)> {
    let items = value.as_array().context("expected a two-element array")?;
    let [x, y] = items.as_slice() else {
        anyhow::bail!("expected a two-element array, got {} items", items.len());
    };
    Ok((
        x.as_f64().context("array element is not a number")?,
        y.as_f64().context("array element is not a number")?,
    ))
}

/// Flatten arbitrarily nested numeric arrays in row-major order, like `np.asarray(...)`.
pub fn flatten_numbers(value: &Value) -> Result<Vec<f64>> {
    let mut numbers = Vec::new();
    flatten_into(value, &mut numbers)?;
    Ok(numbers)
}

/// Appends every number in `value` to `numbers`, depth first. Fails on any leaf that is not
/// a number.
fn flatten_into(value: &Value, numbers: &mut Vec<f64>) -> Result<()> {
    match value {
        Value::Array(items) => {
            for item in items {
                flatten_into(item, numbers)?;
            }
        }
        Value::Number(number) => numbers.push(number.as_f64().context("number out of range")?),
        _ => anyhow::bail!("expected only numbers in nested array"),
    }
    Ok(())
}
