//! Highlight scatter of the half-resolution Fast path: small, much brighter than their surroundings pixels are drawn as exact bokeh discs, because a sampled gather only hits them with a few samples and shows spiral ribs.
use super::half::smoothstep;
use crate::datamodel::{Settings, render::FilterMode};
use ndarray::{Array2, Array3};

/// Smallest disc radius in half-size pixels that is scattered; smaller discs are gathered well.
const MIN_RADIUS: f32 = 2.0;
/// Luma difference to the neighbour average at which scattering starts, and the one at which a pixel is scattered entirely.
const START_DIFF: f32 = 1.0;
/// CoC difference in half-size pixels from which a nearer surface starts to hide a highlight.
const OCCLUDE_FROM: f32 = 1.0;
/// CoC difference over which the hiding grows from none to complete.
const OCCLUDE_RAMP: f32 = 2.0;
/// Background CoC in half-size pixels at which a surface is blurred too much to hide a highlight at all.
const FOCUS_BLUR_LIMIT: f32 = 2.0;
/// CoC range below which a background surface stops hiding a highlight.
const FOCUS_BLUR_RAMP: f32 = 1.5;
/// Far probe radius of a far surface as a multiple of its CoC, as in the gather's far occlusion (`FAR_PROBE_SCALE`).
const FAR_PROBE_SCALE: f32 = 2.0;
/// Smallest CoC magnitude in render pixels the far probe is scaled from, the gather's `GATHER_MIN_COC`.
const FAR_PROBE_MIN_COC: f32 = 0.5;
/// Multiple of the probe radius from which a far highlight lies wholly behind the probed surface (`FAR_BEHIND_SCALE`).
const FAR_BEHIND_SCALE: f32 = 1.25;
/// Multiple of the probe radius up to which a far pixel blocks fully; its blocking fades out at the probe radius (`FAR_BLOCKED_SCALE`).
const FAR_BLOCKED_SCALE: f32 = 0.8;

/// Returns whether the aperture is a plain disc: a flat disc is all this scatter can draw, so any filter image, non-uniform effect or axial aberration leaves the highlights to the gather.
pub(super) fn plain_disc(settings: &Settings) -> bool {
    let lens = &settings.non_uniform;
    settings.render.filter.mode() == FilterMode::Simple
        && !(lens.catseye.enable
            || lens.astigmatism.enable
            || lens.barndoors.enable
            || lens.axial_aberration.enable)
}

/// One scattered highlight.
pub(super) struct Highlight {
    /// Pixel row.
    y: usize,
    /// Pixel column.
    x: usize,
    /// Disc radius in half-size pixels.
    radius: f32,
    /// Signed CoC of the source pixel in half-size pixels.
    coc: f32,
    /// Premultiplied RGBA to spread over the disc.
    rgba: [f32; 4],
}

/// Returns the luma of one pixel.
fn luma(rgba: &Array3<f32>, y: usize, x: usize) -> f32 {
    0.2126 * rgba[[y, x, 0]] + 0.7152 * rgba[[y, x, 1]] + 0.0722 * rgba[[y, x, 2]]
}

/// Removes the scattered share of every highlight pixel from `rgba` and returns the highlights. A pixel whose luma exceeds the average of its four neighbours by [`START_DIFF`] or more is scattered, with a ramp up to twice that, when its disc is at least [`MIN_RADIUS`] wide.
pub(super) fn extract(
    rgba: &mut Array3<f32>,
    coc: &Array2<f32>,
    max_radius: f32,
) -> Vec<Highlight> {
    let (height, width, _) = rgba.dim();
    let mut highlights = Vec::new();
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let radius = coc[[y, x]].abs().min(max_radius);
            // Discs that cross the frame edge stay with the gather, which extends the edge pixels.
            let reach = radius + 1.0;
            let inside = y as f32 >= reach
                && x as f32 >= reach
                && (y as f32) + reach < height as f32
                && (x as f32) + reach < width as f32;
            if radius < MIN_RADIUS || !inside {
                continue;
            }
            let here = luma(rgba, y, x);
            let around = 0.25
                * (luma(rgba, y - 1, x)
                    + luma(rgba, y + 1, x)
                    + luma(rgba, y, x - 1)
                    + luma(rgba, y, x + 1));
            let share = ((here - around) / START_DIFF - 1.0).clamp(0.0, 1.0);
            if share <= 0.0 {
                continue;
            }
            let mut taken = [0.0; 4];
            for (channel, value) in taken.iter_mut().enumerate() {
                *value = rgba[[y, x, channel]] * share;
                rgba[[y, x, channel]] -= *value;
            }
            highlights.push(Highlight {
                y,
                x,
                radius,
                coc: coc[[y, x]],
                rgba: taken,
            });
        }
    }
    highlights
}

/// Returns how much of `highlight` is seen at pixel `(y, x)`, following the gather. A foreground highlight belongs to the front layer, one normalised gather in which no near source hides another, so it is always seen. A background highlight is hidden by surfaces that are in focus or in front of it, when they are nearer (a larger CoC, positive is foreground), and by a less blurred far surface in front of it (`far_hidden`).
fn visibility(coc: &Array2<f32>, y: usize, x: usize, highlight: &Highlight) -> f32 {
    if highlight.coc >= 0.0 {
        return 1.0;
    }
    (1.0 - sharp_hidden(coc, y, x, highlight.coc)) * (1.0 - far_hidden(coc, y, x, highlight))
}

/// Returns how much of a background highlight with CoC `source_coc` a surface at pixel `(y, x)` that is in focus or in front of it hides. The CoC is averaged over 3x3 pixels to soften the silhouette.
fn sharp_hidden(coc: &Array2<f32>, y: usize, x: usize, source_coc: f32) -> f32 {
    let (height, width) = coc.dim();
    let rows = y.saturating_sub(1)..=(y + 1).min(height - 1);
    let columns = x.saturating_sub(1)..=(x + 1).min(width - 1);
    let count = rows.clone().count() * columns.clone().count();
    let mut sum = 0.0;
    for ny in rows {
        for nx in columns.clone() {
            sum += coc[[ny, nx]];
        }
    }
    let here = sum / count as f32;
    let nearer = ((here - source_coc - OCCLUDE_FROM) / OCCLUDE_RAMP).clamp(0.0, 1.0);
    let sharp = ((here + FOCUS_BLUR_LIMIT) / FOCUS_BLUR_RAMP).clamp(0.0, 1.0);
    nearer * sharp
}

/// Returns how much of a background `highlight` the far surface at pixel `(y, x)` hides, with the gather's far occlusion tested on the highlight's own ray: the surface probes a disc of [`FAR_PROBE_SCALE`] times its CoC, the highlight is behind it by its CoC (zero at the probe radius, one at [`FAR_BEHIND_SCALE`] times it), and its ray crosses the probe depth at `probe / radius` of the way from the pixel to the highlight, where the pixel there blocks it unless it is more blurred than the probe radius. Rays from highlights that do not lie behind the surface are never hidden.
fn far_hidden(coc: &Array2<f32>, y: usize, x: usize, highlight: &Highlight) -> f32 {
    let surface = coc[[y, x]];
    if surface >= 0.0 {
        return 0.0;
    }
    let probe = FAR_PROBE_SCALE * (-surface).max(FAR_PROBE_MIN_COC);
    let radius = -highlight.coc;
    let behind = smoothstep(probe, probe * FAR_BEHIND_SCALE, radius);
    if behind <= 0.0 {
        return 0.0;
    }
    let along = probe / radius;
    let crossing_y = y as f32 + along * (highlight.y as f32 - y as f32);
    let crossing_x = x as f32 + along * (highlight.x as f32 - x as f32);
    behind * bilinear_blocking(coc, crossing_y, crossing_x, probe)
}

/// Returns the far blocking of the probe disc of radius `probe` at the fractional pixel position, bilinear between the four nearest pixels: a pixel that is in focus or in front blocks fully, a far pixel fully up to [`FAR_BLOCKED_SCALE`] times the probe radius and not at all above it.
fn bilinear_blocking(coc: &Array2<f32>, y: f32, x: f32, probe: f32) -> f32 {
    let (height, width) = coc.dim();
    let blocking = |row: f32, column: f32| {
        let value = coc[[
            (row as usize).min(height - 1),
            (column as usize).min(width - 1),
        ]];
        if value >= 0.0 {
            1.0
        } else {
            1.0 - smoothstep(probe * FAR_BLOCKED_SCALE, probe, -value)
        }
    };
    let (top, left) = (y.floor().max(0.0), x.floor().max(0.0));
    let (fy, fx) = ((y - top).clamp(0.0, 1.0), (x - left).clamp(0.0, 1.0));
    let upper = blocking(top, left) * (1.0 - fx) + blocking(top, left + 1.0) * fx;
    let lower = blocking(top + 1.0, left) * (1.0 - fx) + blocking(top + 1.0, left + 1.0) * fx;
    upper * (1.0 - fy) + lower * fy
}

/// Adds every highlight to `render` as a disc with a one pixel feather whose total weight is one, hidden where a nearer surface in `coc` covers it.
pub(super) fn splat(render: &mut Array3<f32>, coc: &Array2<f32>, highlights: &[Highlight]) {
    let (height, width, _) = render.dim();
    for highlight in highlights {
        let reach = highlight.radius.ceil() as i64 + 1;
        let mut weights = Vec::new();
        let mut total = 0.0;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (y, x) = (highlight.y as i64 + dy, highlight.x as i64 + dx);
                let distance = ((dx * dx + dy * dy) as f32).sqrt();
                let weight = (highlight.radius - distance + 0.5).clamp(0.0, 1.0);
                if weight > 0.0 && y >= 0 && x >= 0 && y < height as i64 && x < width as i64 {
                    weights.push((y as usize, x as usize, weight));
                    total += weight;
                }
            }
        }
        for (y, x, weight) in weights {
            let seen = weight * visibility(coc, y, x, highlight) / total;
            for channel in 0..4 {
                render[[y, x, channel]] += highlight.rgba[channel] * seen;
            }
        }
    }
}

#[cfg(test)]
/// Tests for highlight extraction and the disc splat.
mod tests {
    use super::*;

    /// A dark 40x40 frame with one bright pixel at the centre and a constant CoC.
    fn frame(coc_radius: f32, peak: f32) -> (Array3<f32>, Array2<f32>) {
        let mut rgba = Array3::zeros((40, 40, 4));
        for channel in 0..4 {
            rgba[[20, 20, channel]] = peak;
        }
        (rgba, Array2::from_elem((40, 40), coc_radius))
    }

    /// Sums every value of the array.
    fn total(array: &Array3<f32>) -> f32 {
        array.iter().sum()
    }

    #[test]
    /// A bright pixel is moved entirely onto a disc and the energy is kept.
    fn highlight_becomes_a_disc_with_the_same_energy() {
        let (mut rgba, coc) = frame(6.0, 40.0);
        let before = total(&rgba);
        let highlights = extract(&mut rgba, &coc, 32.0);
        assert_eq!(highlights.len(), 1);
        assert!(total(&rgba).abs() < 1e-3);
        splat(&mut rgba, &coc, &highlights);
        assert!((total(&rgba) - before).abs() < 1e-2);
        assert!(rgba[[20, 20, 0]] > 0.0 && rgba[[20, 26, 0]] > 0.0 && rgba[[20, 30, 0]] == 0.0);
    }

    #[test]
    /// Pixels that are dim, have a small disc or a disc across the frame edge stay with the gather.
    fn other_pixels_are_left_alone() {
        for (radius, peak) in [(6.0, 0.5), (1.0, 40.0), (30.0, 40.0)] {
            let (mut rgba, coc) = frame(radius, peak);
            assert!(extract(&mut rgba, &coc, 32.0).is_empty());
            assert_eq!(rgba[[20, 20, 0]], peak);
        }
    }

    #[test]
    /// A foreground highlight is seen whole over a nearer, more blurred foreground surface, as in the front gather.
    fn a_foreground_highlight_is_never_hidden() {
        let (mut rgba, mut coc) = frame(6.0, 40.0);
        let highlights = extract(&mut rgba, &coc, 32.0);
        let mut open = Array3::zeros((40, 40, 4));
        splat(&mut open, &coc, &highlights);
        for y in 22..40 {
            for x in 0..40 {
                coc[[y, x]] = 10.0;
            }
        }
        let mut covered = Array3::zeros((40, 40, 4));
        splat(&mut covered, &coc, &highlights);
        assert!((total(&covered) - total(&open)).abs() < 1e-4);
    }

    #[test]
    /// A background highlight is hidden behind a far surface with a much smaller CoC, away from its silhouette, but
    /// not behind a far surface that is about as blurred as itself.
    fn a_less_blurred_far_surface_hides_a_background_highlight() {
        let (mut rgba, mut coc) = frame(-12.0, 40.0);
        let highlights = extract(&mut rgba, &coc, 32.0);
        let mut open = Array3::zeros((40, 40, 4));
        splat(&mut open, &coc, &highlights);
        for (surface, hidden) in [(-2.0, true), (-10.0, false)] {
            for y in 0..40 {
                for x in 0..20 {
                    coc[[y, x]] = surface;
                }
            }
            let mut render = Array3::zeros((40, 40, 4));
            splat(&mut render, &coc, &highlights);
            let left = |image: &Array3<f32>| -> f32 {
                (0..40)
                    .flat_map(|y| (8..16).map(move |x| (y, x)))
                    .map(|(y, x)| image[[y, x, 0]])
                    .sum()
            };
            assert!(left(&open) > 0.0);
            assert_eq!(left(&render) < 1e-3, hidden, "surface CoC {surface}");
        }
    }

    #[test]
    /// A sharp or foreground surface hides a background highlight, while another blurred background surface does not.
    fn only_sharp_or_foreground_surfaces_hide_a_highlight() {
        let (mut rgba, mut coc) = frame(-6.0, 40.0);
        let highlights = extract(&mut rgba, &coc, 32.0);
        let sum = |render: &Array3<f32>, columns: std::ops::Range<usize>| -> f32 {
            columns.map(|x| render[[20, x, 0]]).sum()
        };
        let mut open = Array3::zeros((40, 40, 4));
        splat(&mut open, &coc, &highlights);
        for x in 22..40 {
            for y in 0..40 {
                coc[[y, x]] = 0.0;
            }
        }
        let mut sharp = Array3::zeros((40, 40, 4));
        splat(&mut sharp, &coc, &highlights);
        assert!(sum(&sharp, 23..40) < 1e-3 && sum(&sharp, 14..22) > 0.0);
        for x in 22..40 {
            for y in 0..40 {
                coc[[y, x]] = -10.0;
            }
        }
        let mut blurred = Array3::zeros((40, 40, 4));
        splat(&mut blurred, &coc, &highlights);
        assert!((sum(&blurred, 14..27) - sum(&open, 14..27)).abs() < 1e-4);
    }
}
