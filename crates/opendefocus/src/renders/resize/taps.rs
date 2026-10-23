//! Per-output-sample tap tables: which source samples each output sample reads and with which normalised weights.
use super::filter::ResizeFilter;

/// Fixed-stride tap table for one axis. Output sample `o` reads `stride` taps starting at `o * stride`.
pub(super) struct TapTable {
    /// Number of taps per output sample; shorter spans are padded with zero weights.
    pub(super) stride: usize,
    /// Source index for each tap, already clamped to the valid range.
    pub(super) indices: Vec<usize>,
    /// Normalised weight for each tap; each output sample's weights sum to one.
    pub(super) weights: Vec<f32>,
}

impl TapTable {
    /// Builds the taps that resample `src_len` samples to `dst_len` samples with `filter`.
    /// Sample centres sit at pixel centres, and the kernel is widened by the downscale factor when shrinking.
    /// Taps falling outside the source are dropped and the rest renormalised, so edges are clamped.
    pub(super) fn new(src_len: usize, dst_len: usize, filter: ResizeFilter) -> Self {
        let ratio = src_len as f64 / dst_len as f64;
        let kernel_scale = ratio.max(1.0);
        let radius = filter.support() * kernel_scale;
        let last = src_len as isize - 1;
        let spans: Vec<(usize, Vec<f64>)> = (0..dst_len)
            .map(|o| {
                let centre = (o as f64 + 0.5) * ratio - 0.5;
                let start = ((centre - radius).ceil() as isize).clamp(0, last);
                let end = ((centre + radius).floor() as isize)
                    .clamp(0, last)
                    .max(start);
                let raw: Vec<f64> = (start..=end)
                    .map(|i| filter.weight((i as f64 - centre) / kernel_scale))
                    .collect();
                let sum: f64 = raw.iter().sum();
                let weights = if sum > 0.0 && sum.is_finite() {
                    raw.iter().map(|w| w / sum).collect()
                } else {
                    nearest_tap_weights(start, end, centre)
                };
                (start as usize, weights)
            })
            .collect();
        let stride = spans.iter().map(|(_, w)| w.len()).max().unwrap_or(1);
        let mut indices = Vec::with_capacity(dst_len * stride);
        let mut weights = Vec::with_capacity(dst_len * stride);
        for (start, span) in spans {
            for k in 0..stride {
                indices.push((start + k).min(src_len - 1));
                weights.push(span.get(k).copied().unwrap_or(0.0) as f32);
            }
        }
        Self {
            stride,
            indices,
            weights,
        }
    }
}

/// Returns a weight vector over `start..=end` with weight one on the tap closest to `centre`, zero elsewhere.
/// Used only when the kernel sums to zero over the span, where normalising would divide by zero.
fn nearest_tap_weights(start: isize, end: isize, centre: f64) -> Vec<f64> {
    let nearest = (start..=end)
        .min_by(|a, b| {
            (*a as f64 - centre)
                .abs()
                .total_cmp(&(*b as f64 - centre).abs())
        })
        .unwrap_or(start);
    (start..=end)
        .map(|i| if i == nearest { 1.0 } else { 0.0 })
        .collect()
}

#[cfg(test)]
/// Tests for tap table construction.
mod tests {
    use super::TapTable;
    use crate::renders::resize::filter::ResizeFilter;

    #[test]
    /// Verifies every output sample's weights sum to one, including at the clamped edges.
    fn weights_are_normalised_per_output() {
        for filter in [
            ResizeFilter::Triangle,
            ResizeFilter::Catrom,
            ResizeFilter::Mitchell,
        ] {
            let table = TapTable::new(7, 3, filter);
            for weights in table.weights.chunks(table.stride) {
                let sum: f32 = weights.iter().sum();
                assert!((sum - 1.0).abs() < 1e-6, "{filter:?}: {sum}");
            }
        }
    }

    #[test]
    /// Verifies a 2 to 1 Triangle downscale averages both source samples with equal weight.
    fn triangle_halving_averages_the_pair() {
        let table = TapTable::new(2, 1, ResizeFilter::Triangle);
        assert_eq!(table.indices, vec![0, 1]);
        assert_eq!(table.weights, vec![0.5, 0.5]);
    }
}
