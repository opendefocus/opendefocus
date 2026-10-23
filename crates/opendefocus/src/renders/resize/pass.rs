//! Separable resampling passes over interleaved f32 buffers, plus the per-channel range clamp applied to their output.
use super::taps::TapTable;

/// Resamples every row of `src` horizontally and returns the output with the `(min, max)` of each channel over all of `src`.
/// Rows hold `src_width` samples of `channels` interleaved values; output rows hold one sample per output in `taps`.
/// The range comes from the same sweep over the source, so the clamp costs no extra pass.
pub(super) fn resample_horizontal(
    src: &[f32],
    src_width: usize,
    channels: usize,
    taps: &TapTable,
) -> (Vec<f32>, Vec<(f32, f32)>) {
    match channels {
        1 => horizontal_fixed::<1>(src, src_width, taps),
        2 => horizontal_fixed::<2>(src, src_width, taps),
        3 => horizontal_fixed::<3>(src, src_width, taps),
        4 => horizontal_fixed::<4>(src, src_width, taps),
        _ => horizontal_planar(src, src_width, channels, taps),
    }
}

/// Horizontal pass with a compile-time channel count. Each output pixel accumulates into a fixed-size array the compiler can unroll.
fn horizontal_fixed<const CHANNELS: usize>(
    src: &[f32],
    src_width: usize,
    taps: &TapTable,
) -> (Vec<f32>, Vec<(f32, f32)>) {
    let row_len = src_width * CHANNELS;
    let dst_width = taps.weights.len() / taps.stride;
    let mut dst = vec![0.0_f32; src.len() / row_len * dst_width * CHANNELS];
    let mut lows = [f32::INFINITY; CHANNELS];
    let mut highs = [f32::NEG_INFINITY; CHANNELS];
    for (src_row, dst_row) in src
        .chunks_exact(row_len)
        .zip(dst.chunks_exact_mut(dst_width * CHANNELS))
    {
        let pixels = src_row.as_chunks::<CHANNELS>().0;
        for pixel in pixels {
            for channel in 0..CHANNELS {
                let value = pixel[channel];
                lows[channel] = if value < lows[channel] {
                    value
                } else {
                    lows[channel]
                };
                highs[channel] = if value > highs[channel] {
                    value
                } else {
                    highs[channel]
                };
            }
        }
        for (out, (indices, weights)) in dst_row.as_chunks_mut::<CHANNELS>().0.iter_mut().zip(
            taps.indices
                .chunks_exact(taps.stride)
                .zip(taps.weights.chunks_exact(taps.stride)),
        ) {
            let mut accumulator = [0.0_f32; CHANNELS];
            for (&index, &weight) in indices.iter().zip(weights) {
                let sample = &pixels[index];
                for (total, &input) in accumulator.iter_mut().zip(sample) {
                    *total += weight * input;
                }
            }
            *out = accumulator;
        }
    }
    (dst, lows.into_iter().zip(highs).collect())
}

/// Horizontal pass for channel counts without a fixed-size kernel: each channel is gathered into its own plane and resampled alone.
fn horizontal_planar(
    src: &[f32],
    src_width: usize,
    channels: usize,
    taps: &TapTable,
) -> (Vec<f32>, Vec<(f32, f32)>) {
    let rows = src.len() / (src_width * channels);
    let dst_width = taps.weights.len() / taps.stride;
    let mut dst = vec![0.0_f32; rows * dst_width * channels];
    for channel in 0..channels {
        let plane: Vec<f32> = src
            .iter()
            .skip(channel)
            .step_by(channels)
            .copied()
            .collect();
        let (resampled, _) = horizontal_fixed::<1>(&plane, src_width, taps);
        for (pixel, value) in resampled.into_iter().enumerate() {
            dst[pixel * channels + channel] = value;
        }
    }
    (dst, channel_ranges(src, channels))
}

/// Resamples the rows of `src` vertically. Each row is `row_len` values long; `taps` selects source rows per output row.
pub(super) fn resample_vertical(src: &[f32], row_len: usize, taps: &TapTable) -> Vec<f32> {
    let dst_rows = taps.weights.len() / taps.stride;
    let mut dst = vec![0.0_f32; dst_rows * row_len];
    for (out, (indices, weights)) in dst.chunks_exact_mut(row_len).zip(
        taps.indices
            .chunks_exact(taps.stride)
            .zip(taps.weights.chunks_exact(taps.stride)),
    ) {
        for (&index, &weight) in indices.iter().zip(weights) {
            let source_row = &src[index * row_len..][..row_len];
            for (value, &input) in out.iter_mut().zip(source_row) {
                *value += weight * input;
            }
        }
    }
    dst
}

/// Returns the `(min, max)` of every interleaved channel of `data`, indexed by channel.
pub(super) fn channel_ranges(data: &[f32], channels: usize) -> Vec<(f32, f32)> {
    (0..channels)
        .map(|channel| {
            data.iter()
                .skip(channel)
                .step_by(channels)
                .fold((f32::INFINITY, f32::NEG_INFINITY), |range, &value| {
                    extend_range(range, value)
                })
        })
        .collect()
}

/// Widens `range` so it also contains `value`. NaN never widens the range.
fn extend_range((lo, hi): (f32, f32), value: f32) -> (f32, f32) {
    (
        if value < lo { value } else { lo },
        if value > hi { value } else { hi },
    )
}

/// Clamps every channel of `data` to its `(min, max)` range. Removes the overshoot that negative kernel lobes add to step edges.
/// Channels with no finite samples have an empty range and are left unchanged.
pub(super) fn clamp_channels(data: &mut [f32], ranges: &[(f32, f32)]) {
    match ranges.len() {
        1 => clamp_fixed::<1>(data, ranges),
        2 => clamp_fixed::<2>(data, ranges),
        3 => clamp_fixed::<3>(data, ranges),
        4 => clamp_fixed::<4>(data, ranges),
        channels => {
            for (channel, &(lo, hi)) in ranges.iter().enumerate() {
                if lo <= hi {
                    for value in data.iter_mut().skip(channel).step_by(channels) {
                        *value = clamp_value(*value, lo, hi);
                    }
                }
            }
        }
    }
}

/// Clamps interleaved data with a compile-time channel count, pixel by pixel.
fn clamp_fixed<const CHANNELS: usize>(data: &mut [f32], ranges: &[(f32, f32)]) {
    for pixel in data.as_chunks_mut::<CHANNELS>().0 {
        for (value, &(lo, hi)) in pixel.iter_mut().zip(ranges) {
            if lo <= hi {
                *value = clamp_value(*value, lo, hi);
            }
        }
    }
}

/// Clamps `value` to `[lo, hi]` with plain comparisons; NaN is returned unchanged.
fn clamp_value(value: f32, lo: f32, hi: f32) -> f32 {
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

#[cfg(test)]
/// Tests for the resampling passes and range clamp.
mod tests {
    use super::*;
    use crate::renders::resize::filter::ResizeFilter;

    #[test]
    /// Verifies a horizontal pass with the identity tap table returns its input unchanged and reports the source range per channel.
    fn identity_horizontal_pass_keeps_samples_and_reports_ranges() {
        let taps = TapTable::new(3, 3, ResizeFilter::Catrom);
        let src: Vec<f32> = (0..12).map(|v| v as f32).collect();
        let (dst, ranges) = resample_horizontal(&src, 3, 2, &taps);
        for (value, expected) in dst.iter().zip(&src) {
            assert!((value - expected).abs() < 1e-5);
        }
        assert_eq!(ranges, vec![(0.0, 10.0), (1.0, 11.0)]);
    }

    #[test]
    /// Verifies the clamp keeps each channel inside its own range.
    fn clamp_keeps_each_channel_inside_its_range() {
        let mut data = vec![0.0, 1.0, 0.5, 2.0];
        let ranges = channel_ranges(&[0.0, 1.0, 0.5, 0.25], 2);
        clamp_channels(&mut data, &ranges);
        assert_eq!(data, vec![0.0, 1.0, 0.5, 1.0]);
    }

    #[test]
    /// Verifies a channel whose range is empty, as for all-NaN data, is left unchanged instead of panicking.
    fn empty_range_leaves_channel_unchanged() {
        let mut data = vec![f32::NAN, 2.0];
        clamp_channels(&mut data, &[(f32::INFINITY, f32::NEG_INFINITY), (0.0, 1.0)]);
        assert!(data[0].is_nan());
        assert_eq!(data[1], 1.0);
    }
}
