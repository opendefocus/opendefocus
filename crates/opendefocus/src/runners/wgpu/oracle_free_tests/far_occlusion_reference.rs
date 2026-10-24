//! Brute-force CPU reference of the far gather with the far occlusion model, pixel by pixel in f64, for far-only
//! scenes with a plain disc aperture.
use ndarray::Array3;

use super::references::{disc_coverage, disc_weight};

/// Probe radius as a multiple of the destination's far `CoC` (the shader's `FAR_PROBE_SCALE`).
const PROBE_SCALE: f64 = 2.0;
/// Smallest `CoC` the probe radius is scaled from, and the gather's inclusion radius (`GATHER_MIN_COC`).
const GATHER_MIN_COC: f64 = 0.5;
/// Multiple of the probe radius from which a source is wholly behind (`FAR_BEHIND_SCALE`).
const BEHIND_SCALE: f64 = 1.25;
/// Multiple of the probe radius up to which a far pixel blocks fully (`FAR_BLOCKED_SCALE`).
const BLOCKED_SCALE: f64 = 0.8;
/// Open fraction at which the fitted edge reaches its full one-pixel softness (`FAR_SOFT_OPEN`).
const SOFT_OPEN: f64 = 0.05;
/// Visible fractions of the far validity between which the remainder hands over from the surface's own colour
/// (`FAR_HIDDEN_RESIDUE`, `FAR_HIDDEN_TRUSTED`).
const HIDDEN_TRUST: (f64, f64) = (0.01, 0.05);
/// Far `CoC` from which a destination shows its far gather instead of its own source (`FAR_CENTER_BLURRED_COC`).
const CENTER_BLURRED_COC: f64 = 1.0;

/// Smooth Hermite step of `x` between `low` and `high`, as WGSL's `smoothstep`.
fn smoothstep(low: f64, high: f64, x: f64) -> f64 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Height, as a fraction of the radius, of the chord that cuts the area fraction `open` off a disc: the exact
/// inverse of the circular-segment area, by bisection to f64 precision.
fn cap_height(open: f64) -> f64 {
    let (mut low, mut high) = (-1.0f64, 1.0f64);
    for _ in 0..60 {
        let middle = f64::midpoint(low, high);
        let area = (middle.acos() - middle * (1.0 - middle * middle).sqrt()) / std::f64::consts::PI;
        if area > open {
            low = middle;
        } else {
            high = middle;
        }
    }
    f64::midpoint(low, high)
}

/// Occluder fitted to a probe disc, as the shader's `FarOcclusion`.
enum Occluder {
    /// Nothing in the probe disc blocks.
    Open,
    /// The probe disc is fully blocked.
    Hidden,
    /// A partly open probe disc.
    Partial {
        /// Fitted edge along the normal, in probe radii.
        edge: f64,
        /// Unit normal towards the open side.
        normal: (f64, f64),
        /// How one-sided the open area is, from 0 (spread) to 1 (a half-plane).
        one_sided: f64,
        /// Open fraction of the probe disc.
        open: f64,
        /// Softening of the edge in pixels.
        softness: f64,
    },
}

/// Fits the occluder of the destination `(x, y)` from every far pixel inside its probe disc of radius `probe`: a
/// pixel blocks fully up to `BLOCKED_SCALE` times the radius and not at all above it.
fn occluder(
    (w, h): (usize, usize),
    radius: &impl Fn(usize, usize) -> f32,
    (x, y): (usize, usize),
    probe: f64,
) -> Occluder {
    let (mut blocked, mut area, mut moment) = (0.0f64, 0.0f64, (0.0f64, 0.0f64));
    for sy in 0..h {
        for sx in 0..w {
            let offset = (sx as f64 - x as f64, sy as f64 - y as f64);
            let weight = (probe - offset.0.hypot(offset.1) + 0.5).clamp(0.0, 1.0);
            let coc = f64::from(radius(sx, sy));
            let pixel_blocked = weight * (1.0 - smoothstep(probe * BLOCKED_SCALE, probe, coc));
            blocked += pixel_blocked;
            area += weight;
            moment.0 += offset.0 * (weight - pixel_blocked);
            moment.1 += offset.1 * (weight - pixel_blocked);
        }
    }
    if area <= 0.0 || blocked <= 0.0 {
        return Occluder::Open;
    }
    let open = 1.0 - (blocked / area).clamp(0.0, 1.0);
    if open <= 0.0 {
        return Occluder::Hidden;
    }
    let edge = cap_height(open);
    let moment = (moment.0 / (area * probe), moment.1 / (area * probe));
    let cap_moment = 2.0 / (3.0 * std::f64::consts::PI) * (1.0 - edge * edge).powf(1.5);
    let length = moment.0.hypot(moment.1);
    let (one_sided, normal) = if length > 0.0 && cap_moment > 0.0 {
        (
            (length / cap_moment).clamp(0.0, 1.0),
            (moment.0 / length, moment.1 / length),
        )
    } else {
        (0.0, (0.0, 0.0))
    };
    Occluder::Partial {
        edge,
        normal,
        one_sided,
        open,
        softness: (open / SOFT_OPEN).min(1.0),
    }
}

/// Hidden fraction of a far source of `CoC` `coc` at `offset` from a destination with probe radius `probe`.
fn hidden_fraction(occluder: &Occluder, probe: f64, coc: f64, offset: (f64, f64)) -> f64 {
    let behind = smoothstep(probe, probe * BEHIND_SCALE, coc);
    match *occluder {
        Occluder::Open => 0.0,
        Occluder::Hidden => behind,
        Occluder::Partial {
            edge,
            normal,
            one_sided,
            open,
            softness,
        } => {
            let along = (normal.0 * offset.0 + normal.1 * offset.1) / coc;
            let edge_hidden = 1.0 - ((along - edge) * probe + softness).clamp(0.0, 1.0);
            behind * ((1.0 - open) + (edge_hidden - (1.0 - open)) * one_sided)
        }
    }
}

/// Far layer only, every pixel valid, plain disc: the normalised far gather with every source weighted by the part
/// the far occlusion leaves visible, handing over to the destination's own colour when almost all of it is hidden.
/// Destinations whose own far `CoC` is below the gather's blur keep their colour.
pub(super) fn far_occluded_reference(
    (w, h): (usize, usize),
    color: impl Fn(usize, usize) -> [f32; 4],
    radius: impl Fn(usize, usize) -> f32,
) -> Array3<f32> {
    let mut out = Array3::zeros((h, w, 4));
    for y in 0..h {
        for x in 0..w {
            let own = f64::from(radius(x, y));
            let probe = PROBE_SCALE * own.max(GATHER_MIN_COC);
            let occluder = occluder((w, h), &radius, (x, y), probe);
            let (mut premul, mut valid, mut hidden) = ([0.0f64; 4], 0.0f64, 0.0f64);
            for sy in 0..h {
                for sx in 0..w {
                    let coc = radius(sx, sy);
                    let offset = (sx as f64 - x as f64, sy as f64 - y as f64);
                    if f64::from(coc) <= GATHER_MIN_COC {
                        continue;
                    }
                    let weight = f64::from(disc_weight(coc, offset.0 as f32, offset.1 as f32));
                    let hidden_weight =
                        weight * hidden_fraction(&occluder, probe, f64::from(coc), offset);
                    for (channel, sum) in premul.iter_mut().enumerate() {
                        *sum += (weight - hidden_weight) * f64::from(color(sx, sy)[channel]);
                    }
                    valid += weight - hidden_weight;
                    hidden += hidden_weight;
                }
            }
            let surface = color(x, y);
            let visible = valid / (valid + hidden).max(1e-6);
            let trust = if hidden > 0.0 {
                smoothstep(HIDDEN_TRUST.0, HIDDEN_TRUST.1, visible)
            } else {
                1.0
            };
            let blur = smoothstep(GATHER_MIN_COC, CENTER_BLURRED_COC, own);
            for channel in 0..4 {
                let remainder = premul[channel] / valid.max(1e-6);
                let gathered =
                    f64::from(surface[channel]) + trust * (remainder - f64::from(surface[channel]));
                let value = f64::from(surface[channel]) * (1.0 - blur) + gathered * blur;
                out[[y, x, channel]] = value as f32;
            }
        }
    }
    out
}

#[test]
/// The occlusion-free limit of the reference is the plain normalised disc gather.
fn far_occluded_reference_without_occluders_is_the_plain_gather() {
    let color = |x: usize, y: usize| [x as f32 * 0.1, y as f32 * 0.2, 0.5, 1.0];
    let reference = far_occluded_reference((7, 5), color, |_, _| 3.0);
    let plain = super::references::far_reference((7, 5), color, |_, _| 3.0);
    for (a, b) in reference.iter().zip(plain.iter()) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }
    assert!(disc_coverage(3.0, 0.0, 0.0) > 0.0);
}

#[test]
/// In the `far-forced-descend` scene the 100 px neighbours lie behind the 1.1 px destinations, so the reference
/// hides part of them there and differs from the plain gather by more than the hardware comparison's tolerance (1e-5
/// relative), while the 100 px destinations hide nothing. The difference is small because the 100 px sources carry
/// little weight next to the 1.1 px disc of the destination itself.
fn far_occluded_reference_hides_the_background_of_the_forced_descend_scene() {
    use super::scenes::{LAYER_PATH_SIZE, forced_descend_radius, layer_path_color};
    let occluded = far_occluded_reference(LAYER_PATH_SIZE, layer_path_color, forced_descend_radius);
    let plain =
        super::references::far_reference(LAYER_PATH_SIZE, layer_path_color, forced_descend_radius);
    let mut largest = 0.0f32;
    for ((y, x, channel), value) in occluded.indexed_iter() {
        let difference = (value - plain[[y, x, channel]]).abs();
        if forced_descend_radius(x, y) < 100.0 {
            largest = largest.max(difference);
        } else {
            assert!(difference < 1e-6, "({x}, {y}) {channel}: {difference}");
        }
    }
    assert!(largest > 5e-5, "the occlusion hides nothing: {largest}");
}
