//! Parameter value and type representations shared by the parameter table and its
//! accessors. Converts host-side values into the numeric forms the settings use.

/// Value of one parameter, in the representation the host uses for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamValue {
    /// Floating-point value.
    Double(f64),
    /// Integer value.
    Int(i32),
    /// Checkbox state.
    Bool(bool),
    /// Zero-based index into the choice options.
    Choice(i32),
}

/// Host-side type and range of a parameter.
#[derive(Clone, Copy, Debug)]
pub enum ParamKind {
    /// Floating-point parameter with inclusive bounds.
    Double {
        /// Smallest accepted value.
        min: f64,
        /// Largest accepted value.
        max: f64,
        /// Value used before the user changes the parameter.
        default: f64,
    },
    /// Integer parameter with inclusive bounds.
    Int {
        /// Smallest accepted value.
        min: i32,
        /// Largest accepted value.
        max: i32,
        /// Value used before the user changes the parameter.
        default: i32,
    },
    /// Checkbox parameter.
    Bool {
        /// Value used before the user changes the parameter.
        default: bool,
    },
    /// Drop-down parameter whose value is an index into `options`.
    Choice {
        /// Labels shown to the user, in index order.
        options: &'static [&'static str],
        /// Index selected before the user changes the parameter.
        default: i32,
    },
}

/// Index of the "Direct" entry in the Math choice.
pub(super) const DIRECT_MATH: i32 = 2;

/// Reads a choice index. Integers are taken as the same index; other kinds yield `0`.
pub(super) fn choice(value: ParamValue) -> i32 {
    match value {
        ParamValue::Choice(v) | ParamValue::Int(v) => v,
        _ => 0,
    }
}

/// Converts a value to `i32`. Booleans become `0` or `1` and doubles truncate toward zero.
pub(super) fn int(value: ParamValue) -> i32 {
    match value {
        ParamValue::Int(v) | ParamValue::Choice(v) => v,
        ParamValue::Bool(b) => b as i32,
        ParamValue::Double(d) => d as i32,
    }
}

/// Converts a value to `f32`. Booleans become `0.0` or `1.0`.
pub(super) fn float(value: ParamValue) -> f32 {
    match value {
        ParamValue::Double(d) => d as f32,
        ParamValue::Int(v) | ParamValue::Choice(v) => v as f32,
        ParamValue::Bool(b) => b as i32 as f32,
    }
}

/// Converts a value to `bool`. Non-zero numbers are `true`.
pub(super) fn boolean(value: ParamValue) -> bool {
    match value {
        ParamValue::Bool(b) => b,
        ParamValue::Int(v) | ParamValue::Choice(v) => v != 0,
        ParamValue::Double(d) => d != 0.0,
    }
}

/// Unit tests for the value conversions, which read every kind of value the table can hold.
#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that a choice reads its index from an integer or choice value, and is 0 otherwise.
    #[test]
    fn choice_reads_the_index_of_integers_and_choices() {
        assert_eq!(choice(ParamValue::Choice(3)), 3);
        assert_eq!(choice(ParamValue::Int(2)), 2);
        assert_eq!(choice(ParamValue::Bool(true)), 0);
        assert_eq!(choice(ParamValue::Double(1.5)), 0);
    }

    /// Checks that integers convert from booleans and truncate doubles toward zero.
    #[test]
    fn int_converts_booleans_and_truncates_doubles() {
        assert_eq!(int(ParamValue::Bool(true)), 1);
        assert_eq!(int(ParamValue::Bool(false)), 0);
        assert_eq!(int(ParamValue::Double(-2.7)), -2);
        assert_eq!(int(ParamValue::Choice(4)), 4);
    }

    /// Checks that floats convert from integers, choices and booleans.
    #[test]
    fn float_converts_integers_choices_and_booleans() {
        assert_eq!(float(ParamValue::Int(4)), 4.0);
        assert_eq!(float(ParamValue::Choice(5)), 5.0);
        assert_eq!(float(ParamValue::Bool(true)), 1.0);
        assert_eq!(float(ParamValue::Double(0.25)), 0.25);
    }

    /// Checks that a boolean is true for any non-zero number and false for zero.
    #[test]
    fn boolean_is_true_for_any_non_zero_number() {
        assert!(boolean(ParamValue::Bool(true)));
        assert!(boolean(ParamValue::Int(-1)));
        assert!(!boolean(ParamValue::Choice(0)));
        assert!(boolean(ParamValue::Double(0.5)));
        assert!(!boolean(ParamValue::Double(0.0)));
    }
}
