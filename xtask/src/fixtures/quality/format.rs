//! Python-compatible `format(value, "g")` formatting, so that run labels match the names
//! written by the Python reference.
/// Format like Python's `format(value, "g")` (six significant digits, trailing zeros removed).
pub fn format_g(value: f64) -> String {
    const PRECISION: i32 = 6;
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    let scientific = format!("{:.*e}", (PRECISION - 1) as usize, value);
    let Some((mantissa, exponent)) = scientific
        .split_once('e')
        .and_then(|(mantissa, exponent)| Some((mantissa, exponent.parse::<i32>().ok()?)))
    else {
        return scientific;
    };
    if (-4..PRECISION).contains(&exponent) {
        let fixed_digits = (PRECISION - 1 - exponent) as usize;
        trim_fraction(&format!("{value:.fixed_digits$}"))
    } else {
        let mantissa = trim_fraction(mantissa);
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    }
}

/// Removes trailing zeros and then a trailing decimal point. Text without a decimal point is
/// returned unchanged.
fn trim_fraction(text: &str) -> String {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text.to_owned()
    }
}

/// Tests for `format_g`.
#[cfg(test)]
mod tests {
    use super::format_g;

    /// Verifies that `format_g` matches Python for the values used in run labels.
    #[test]
    fn test_format_g_matches_python_for_run_labels() {
        assert_eq!(format_g(0.0), "0");
        assert_eq!(format_g(24.0), "24");
        assert_eq!(format_g(102.0), "102");
        assert_eq!(format_g(0.5), "0.5");
        assert_eq!(format_g(-2.5), "-2.5");
        assert_eq!(format_g(1.0e-5), "1e-05");
        assert_eq!(format_g(1.0e6), "1e+06");
        assert_eq!(format_g(123_456.7), "123457");
        assert_eq!(format_g(0.000_123_456_7), "0.000123457");
    }
}
