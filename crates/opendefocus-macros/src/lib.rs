//! Convencience macros for branchless structure and compile-time optimization
//!
//! Basically sort of same as inline, but with the benefit of forcing the compiler to do so
//! With the cost of larger binary size, sorry! Performance is more important here imho.
#![warn(unused_extern_crates)]
#![no_std]

#[macro_export]
macro_rules! f32_from_bool {
    ($a:expr) => {{
        #[cfg(not(target_arch = "spirv"))]
        { f32::from($a) }
        #[cfg(target_arch = "spirv")]
        { $a as u32 as f32 }
    }};
}

/// Branchless if else expression
#[macro_export]
macro_rules! if_else {
    ($condition:expr, $then:expr, $otherwise:expr) => {{ (f32_from_bool!($condition) * ($then)) + ((!$condition as u32 as f32) * ($otherwise)) }};
}

#[macro_export]
macro_rules! max {
    ($a:expr, $b:expr) => {{ if_else!($a > $b, $a, $b) }};
}

#[macro_export]
macro_rules! min {
    ($a:expr, $b:expr) => {{ if_else!($a < $b, $a, $b) }};
}

#[macro_export]
macro_rules! clamp {
    ($value:expr, $low:expr, $high:expr) => {{ min!(max!($value, $low), $high) }};
}
