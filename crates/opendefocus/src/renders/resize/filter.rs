//! Reconstruction kernels used by the resampler and the filter selection they are named by.
/// Reconstruction filter used when resampling an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeFilter {
    /// Linear interpolation (tent kernel, support of one source sample).
    #[allow(
        dead_code,
        reason = "only tests construct Triangle; it is part of the public filter set"
    )]
    Triangle,
    /// Catmull-Rom cubic (B = 0, C = 1/2), sharp and interpolating.
    Catrom,
    /// Mitchell-Netravali cubic (B = C = 1/3), softer than Catmull-Rom.
    Mitchell,
}

impl ResizeFilter {
    /// Returns the kernel radius in filter units; the kernel is zero beyond it.
    pub(super) fn support(self) -> f64 {
        match self {
            Self::Triangle => 1.0,
            Self::Catrom | Self::Mitchell => 2.0,
        }
    }

    /// Returns the kernel value at signed distance `x` from the sample centre, in filter units.
    pub(super) fn weight(self, x: f64) -> f64 {
        match self {
            Self::Triangle => (1.0 - x.abs()).max(0.0),
            Self::Catrom => cubic(0.0, 0.5, x),
            Self::Mitchell => cubic(1.0 / 3.0, 1.0 / 3.0, x),
        }
    }
}

/// Evaluates the Mitchell-Netravali cubic family with parameters `b` and `c` at distance `x`.
fn cubic(b: f64, c: f64, x: f64) -> f64 {
    let a = x.abs();
    let value = if a < 1.0 {
        (12.0 - 9.0 * b - 6.0 * c) * a.powi(3)
            + (-18.0 + 12.0 * b + 6.0 * c) * a.powi(2)
            + (6.0 - 2.0 * b)
    } else if a < 2.0 {
        (-b - 6.0 * c) * a.powi(3)
            + (6.0 * b + 30.0 * c) * a.powi(2)
            + (-12.0 * b - 48.0 * c) * a
            + (8.0 * b + 24.0 * c)
    } else {
        0.0
    };
    value / 6.0
}

#[cfg(test)]
/// Tests for the reconstruction kernels.
mod tests {
    use super::ResizeFilter;

    #[test]
    /// Verifies Triangle and Catmull-Rom interpolate their samples, and Mitchell matches its published values at 0 and 1.
    fn kernels_match_their_definitions() {
        for filter in [ResizeFilter::Triangle, ResizeFilter::Catrom] {
            assert!((filter.weight(0.0) - 1.0).abs() < 1e-12, "{filter:?}");
            assert!(filter.weight(1.0).abs() < 1e-12, "{filter:?}");
        }
        assert!((ResizeFilter::Mitchell.weight(0.0) - 8.0 / 9.0).abs() < 1e-12);
        assert!((ResizeFilter::Mitchell.weight(1.0) - 1.0 / 18.0).abs() < 1e-12);
        assert_eq!(ResizeFilter::Catrom.weight(2.0), 0.0);
        assert_eq!(ResizeFilter::Mitchell.weight(2.5), 0.0);
    }
}
