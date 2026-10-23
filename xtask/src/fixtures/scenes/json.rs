//! JSON text identical to Python's `json.dumps(value, indent=2)`, so committed metadata stays byte-stable.

/// JSON value tree printed with the layout of Python's `json.dumps(value, indent=2)`.
#[derive(Clone)]
pub enum Json {
    /// Integer number.
    Int(i64),
    /// Floating-point number, printed with `python_float`.
    Float(f64),
    /// Boolean.
    Bool(bool),
    /// String, escaped by `write_string`.
    Str(String),
    /// Array of values.
    List(Vec<Json>),
    /// Object as ordered key and value pairs; keys are written in the order given.
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    /// Array of floating-point numbers.
    pub fn floats(values: &[f64]) -> Self {
        Self::List(values.iter().copied().map(Self::Float).collect())
    }

    /// Array of integers.
    pub fn ints(values: &[i64]) -> Self {
        Self::List(values.iter().copied().map(Self::Int).collect())
    }

    /// String value built from anything convertible to `String`.
    pub fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    /// Returns the JSON text with two-space indentation and no trailing newline.
    pub fn to_pretty_string(&self) -> String {
        let mut out = String::new();
        write_value(self, 0, &mut out);
        out
    }
}

/// Appends `value` to `out`. `depth` is the indentation level of the enclosing container.
fn write_value(value: &Json, depth: usize, out: &mut String) {
    match value {
        Json::Int(number) => out.push_str(&number.to_string()),
        Json::Float(number) => out.push_str(&python_float(*number)),
        Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Json::Str(text) => write_string(text, out),
        Json::List(items) => {
            let parts: Vec<&Json> = items.iter().collect();
            write_container('[', ']', depth, out, parts.len(), |index, out| {
                write_value(parts[index], depth + 1, out);
            });
        }
        Json::Object(entries) => {
            write_container('{', '}', depth, out, entries.len(), |index, out| {
                let (key, entry) = &entries[index];
                write_string(key, out);
                out.push_str(": ");
                write_value(entry, depth + 1, out);
            });
        }
    }
}

/// Appends a bracketed container with one item per line. `write_item` writes item `index`
/// at the next indentation level. An empty container is written as `[]` or `{}`.
fn write_container(
    open: char,
    close: char,
    depth: usize,
    out: &mut String,
    count: usize,
    mut write_item: impl FnMut(usize, &mut String),
) {
    out.push(open);
    if count == 0 {
        out.push(close);
        return;
    }
    for index in 0..count {
        out.push_str(if index == 0 { "\n" } else { ",\n" });
        push_indent(depth + 1, out);
        write_item(index, out);
    }
    out.push('\n');
    push_indent(depth, out);
    out.push(close);
}

/// Appends two spaces for each level of `depth`.
fn push_indent(depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Appends `text` in double quotes, escaping only `"`, `\` and newline.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Python's `repr(float)`: shortest round-trip digits, fixed notation for exponents in [-4, 16).
pub fn python_float(value: f64) -> String {
    let scientific = format!("{value:e}");
    let Some((mantissa, exponent)) = scientific
        .split_once('e')
        .and_then(|(mantissa, exponent)| Some((mantissa, exponent.parse::<i32>().ok()?)))
    else {
        return scientific;
    };
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if (-4..16).contains(&exponent) {
        let body = if exponent < 0 {
            format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
        } else {
            let integer_len = exponent as usize + 1;
            if digits.len() <= integer_len {
                format!("{digits}{}.0", "0".repeat(integer_len - digits.len()))
            } else {
                format!("{}.{}", &digits[..integer_len], &digits[integer_len..])
            }
        };
        format!("{sign}{body}")
    } else {
        let fraction = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        format!(
            "{sign}{}{fraction}e{exponent_sign}{:02}",
            &digits[..1],
            exponent.abs()
        )
    }
}

/// Tests for the JSON writer.
#[cfg(test)]
mod tests {
    use super::{Json, python_float};

    /// Verifies that floats are printed exactly as Python's `repr` prints them.
    #[test]
    fn floats_match_python_repr() {
        assert_eq!(python_float(0.0), "0.0");
        assert_eq!(python_float(-0.0), "-0.0");
        assert_eq!(python_float(2.0), "2.0");
        assert_eq!(python_float(-12.5), "-12.5");
        assert_eq!(python_float(0.01), "0.01");
        assert_eq!(python_float(1.0 / 15.0), "0.06666666666666667");
        assert_eq!(python_float(1.0e-7), "1e-07");
        assert_eq!(python_float(1.5e-15), "1.5e-15");
        assert_eq!(python_float(1.0e16), "1e+16");
        assert_eq!(python_float(1234.0), "1234.0");
    }

    /// Verifies that pretty printing matches Python's two-space `json.dumps` output.
    #[test]
    fn pretty_print_matches_python_indent_two() {
        let value = Json::Object(vec![
            ("list", Json::ints(&[1, 2])),
            ("empty", Json::List(Vec::new())),
            ("nested", Json::Object(vec![("flag", Json::Bool(false))])),
        ]);
        let expected = "{\n  \"list\": [\n    1,\n    2\n  ],\n  \"empty\": [],\n  \"nested\": {\n    \"flag\": false\n  }\n}";
        assert_eq!(value.to_pretty_string(), expected);
    }
}
