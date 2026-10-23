//! Measures the shape of bright points in a rendered image: luma centroid and covariance,
//! per-channel energy and support, and the JSON form written to the report.
use serde_json::{Map, Value, json};

use crate::fixtures::quality::image::{Plane, Rgba};

/// Channel names in RGB order, as written to the report.
const CHANNELS: [&str; 3] = ["r", "g", "b"];
/// Half-width in pixels of the window measured around each point.
const WINDOW_RADIUS: f64 = 28.0;

/// A bright point to measure: its position in pixels and its label.
#[derive(Clone, Debug)]
pub struct SpotPoint {
    /// Position of the point in pixels.
    pub xy: (f64, f64),
    /// Label of the point: `near`, `focus` or `far`.
    pub sign: String,
}

/// A measured spot: its source point and its shape. The shape is `None` when the window holds
/// no signal.
#[derive(Clone, Debug)]
pub struct Spot {
    /// Position of the source point in pixels.
    pub xy: (f64, f64),
    /// Label of the source point.
    pub sign: String,
    /// `None` when the luma-above-background mass is exactly zero.
    pub shape: Option<SpotShape>,
}

/// Shape of a spot measured on luma, with the details of each colour channel.
#[derive(Clone, Debug)]
pub struct SpotShape {
    /// Luma-weighted centroid `(x, y)` in pixels.
    pub centroid: (f64, f64),
    /// Distance from the centroid to the source point, in pixels.
    pub centroid_error_px: f64,
    /// Background-subtracted luma mass of the window.
    pub energy: f64,
    /// Eigenvalues of the luma covariance matrix in pixels squared, ascending.
    pub eigenvalues: [f64; 2],
    /// Relative difference of the two eigenvalues: zero for a round spot.
    pub anisotropy: f64,
    /// Angle of the major axis of the luma covariance, in degrees.
    pub principal_axis_degrees: f64,
    /// Largest distance between two channel centroids, in pixels.
    pub channel_spread_px: f64,
    /// Shapes of the red, green and blue channels.
    pub channels: [ChannelShape; 3],
}

/// Energy, centroid and support of one colour channel of a spot.
#[derive(Clone, Debug)]
pub struct ChannelShape {
    /// Background-subtracted mass of the channel.
    pub energy: f64,
    /// Centroid of the channel, or `None` when the channel has no mass.
    pub centroid: Option<(f64, f64)>,
    /// Largest distance from the source point of a pixel above 0.1% of the channel peak.
    pub support_radius_px: f64,
    /// Eigenvalues of the channel covariance, or `None` when the channel has no mass.
    pub eigenvalues: Option<[f64; 2]>,
}

impl Spot {
    /// Whether the spot has no shape, which means it holds no measurable signal.
    pub fn is_empty(&self) -> bool {
        self.shape.is_none()
    }

    /// Serialises the spot to JSON. An empty spot is written as its position and label with
    /// `empty: true`.
    pub fn to_value(&self) -> Value {
        let Some(shape) = &self.shape else {
            return json!({"xy": [self.xy.0, self.xy.1], "sign": self.sign, "empty": true});
        };
        let mut centroids = Map::new();
        for (name, channel) in CHANNELS.iter().zip(&shape.channels) {
            centroids.insert((*name).to_owned(), pair_value(channel.centroid));
        }
        centroids.insert(
            "rgb_centroid_spread_px".to_owned(),
            json!(shape.channel_spread_px),
        );
        let mut channels = Map::new();
        for (name, channel) in CHANNELS.iter().zip(&shape.channels) {
            let mut entry = json!({
                "energy": channel.energy,
                "centroid_xy": pair_value(channel.centroid),
                "support_radius_px": channel.support_radius_px,
            });
            if let (Some(eigen), Some(object)) = (channel.eigenvalues, entry.as_object_mut()) {
                object.insert("covariance_eigenvalues_px2".to_owned(), json!(eigen));
            }
            channels.insert((*name).to_owned(), entry);
        }
        json!({
            "xy": [self.xy.0, self.xy.1],
            "sign": self.sign,
            "centroid_xy": [shape.centroid.0, shape.centroid.1],
            "centroid_error_px": shape.centroid_error_px,
            "energy": shape.energy,
            "covariance_eigenvalues_px2": shape.eigenvalues,
            "anisotropy": shape.anisotropy,
            "principal_axis_degrees": shape.principal_axis_degrees,
            "channel_centroids_xy": centroids,
            "channels": channels,
        })
    }
}

/// Converts an optional pair to a JSON array, or to `null` when it is absent.
fn pair_value(pair: Option<(f64, f64)>) -> Value {
    pair.map_or(Value::Null, |(x, y)| json!([x, y]))
}

/// Eigenvalues of a symmetric 2x2 matrix `[[a, b], [b, c]]`, ascending like `eigvalsh`.
pub fn eigenvalues_2x2(a: f64, b: f64, c: f64) -> [f64; 2] {
    let mean = f64::midpoint(a, c);
    let spread = (((a - c) / 2.0).powi(2) + b * b).sqrt();
    [mean - spread, mean + spread]
}

/// Measure the HDR point shape, energy and per-channel support around each source.
pub fn bokeh_spot_metrics(image: &Rgba, points: &[SpotPoint]) -> Vec<Spot> {
    let luma = image.luma();
    let luma_background = f64::from(luma.corner_median());
    let planes: [Plane; 3] = std::array::from_fn(|channel| image.channel(channel));
    let backgrounds: [f64; 3] =
        std::array::from_fn(|channel| f64::from(planes[channel].corner_median()));
    points
        .iter()
        .map(|point| {
            let window = Window::around(image, point.xy);
            let channels: [ChannelShape; 3] = std::array::from_fn(|channel| {
                channel_shape(&planes[channel], &window, backgrounds[channel], point.xy)
            });
            let shape = luma_moments(&luma, &window, luma_background, point.xy).map(|moments| {
                let centroids: Vec<(f64, f64)> = channels
                    .iter()
                    .filter_map(|channel| channel.centroid)
                    .collect();
                SpotShape {
                    centroid: moments.centroid,
                    centroid_error_px: moments.centroid_error_px,
                    energy: moments.energy,
                    eigenvalues: moments.eigenvalues,
                    anisotropy: moments.anisotropy,
                    principal_axis_degrees: moments.principal_axis_degrees,
                    channel_spread_px: max_pairwise_distance(&centroids),
                    channels: channels.clone(),
                }
            });
            Spot {
                xy: point.xy,
                sign: point.sign.clone(),
                shape,
            }
        })
        .collect()
}

/// Pixel window around a point, with half-open bounds clipped to the image.
struct Window {
    /// First column of the window.
    left: usize,
    /// First row of the window.
    top: usize,
    /// Column after the last column of the window.
    right: usize,
    /// Row after the last row of the window.
    bottom: usize,
}

impl Window {
    /// Returns the window of radius `WINDOW_RADIUS` around `(x, y)`, clipped to the image.
    fn around(image: &Rgba, (x, y): (f64, f64)) -> Self {
        let clamp = |value: f64, limit: usize| (value.max(0.0) as usize).min(limit);
        Self {
            left: clamp(x.floor() - WINDOW_RADIUS, image.width),
            right: clamp(x.ceil() + WINDOW_RADIUS + 1.0, image.width),
            top: clamp(y.floor() - WINDOW_RADIUS, image.height),
            bottom: clamp(y.ceil() + WINDOW_RADIUS + 1.0, image.height),
        }
    }

    /// Background-subtracted weights with pixel coordinates, in row-major order.
    fn weights(&self, plane: &Plane, background: f64) -> Vec<(f64, f64, f64)> {
        (self.top..self.bottom)
            .flat_map(|y| (self.left..self.right).map(move |x| (x, y)))
            .map(|(x, y)| {
                let weight = (f64::from(plane.at(x, y)) - background).max(0.0);
                (weight, x as f64, y as f64)
            })
            .collect()
    }
}

/// Luma-weighted moments of the window around one point.
struct LumaMoments {
    /// Luma-weighted centroid in pixels.
    centroid: (f64, f64),
    /// Distance from the centroid to the source point, in pixels.
    centroid_error_px: f64,
    /// Background-subtracted luma mass.
    energy: f64,
    /// Ascending eigenvalues of the luma covariance.
    eigenvalues: [f64; 2],
    /// Relative difference of the eigenvalues.
    anisotropy: f64,
    /// Angle of the major axis of the luma covariance, in degrees.
    principal_axis_degrees: f64,
}

/// Luma-weighted moments over a window; `None` when there is no mass.
fn luma_moments(
    luma: &Plane,
    window: &Window,
    background: f64,
    point: (f64, f64),
) -> Option<LumaMoments> {
    let weights = window.weights(luma, background);
    let mass: f64 = weights.iter().map(|(weight, _, _)| weight).sum();
    if mass == 0.0 {
        return None;
    }
    let centroid_x = weights.iter().map(|(w, x, _)| w * x).sum::<f64>() / mass;
    let centroid_y = weights.iter().map(|(w, _, y)| w * y).sum::<f64>() / mass;
    let (var_xx, var_yy, cov_xy) = second_moments(&weights, mass, (centroid_x, centroid_y));
    let eigenvalues = eigenvalues_2x2(var_xx, cov_xy, var_yy);
    Some(LumaMoments {
        centroid: (centroid_x, centroid_y),
        centroid_error_px: (centroid_x - point.0).hypot(centroid_y - point.1),
        energy: mass,
        eigenvalues,
        anisotropy: (eigenvalues[1] - eigenvalues[0])
            / (eigenvalues[0] + eigenvalues[1]).max(1e-12),
        principal_axis_degrees: 0.5 * (2.0 * cov_xy).atan2(var_xx - var_yy).to_degrees(),
    })
}

/// Measures one colour channel of the window: its background-subtracted mass, centroid,
/// support radius and covariance. A channel without mass has no centroid.
fn channel_shape(
    plane: &Plane,
    window: &Window,
    background: f64,
    point: (f64, f64),
) -> ChannelShape {
    let weights = window.weights(plane, background);
    let mass: f64 = weights.iter().map(|(weight, _, _)| weight).sum();
    if mass <= 1e-12 {
        return ChannelShape {
            energy: mass,
            centroid: None,
            support_radius_px: 0.0,
            eigenvalues: None,
        };
    }
    let centroid_x = weights.iter().map(|(w, x, _)| w * x).sum::<f64>() / mass;
    let centroid_y = weights.iter().map(|(w, _, y)| w * y).sum::<f64>() / mass;
    // A relative threshold suppresses numerical tails while keeping low-amplitude blue stable.
    let peak = weights
        .iter()
        .map(|(weight, _, _)| *weight)
        .fold(f64::NEG_INFINITY, f64::max);
    let threshold = (peak * 1e-3).max(1e-8);
    let support_radius_px = weights
        .iter()
        .filter(|(weight, _, _)| *weight > threshold)
        .map(|(_, x, y)| (x - point.0).hypot(y - point.1))
        .fold(f64::NEG_INFINITY, f64::max);
    let (var_xx, var_yy, cov_xy) = second_moments(&weights, mass, (centroid_x, centroid_y));
    ChannelShape {
        energy: mass,
        centroid: Some((centroid_x, centroid_y)),
        support_radius_px,
        eigenvalues: Some(eigenvalues_2x2(var_xx, cov_xy, var_yy)),
    }
}

/// Returns the weighted variances `(xx, yy)` and covariance `xy` of `weights` about `centre`,
/// normalised by `mass`.
fn second_moments(weights: &[(f64, f64, f64)], mass: f64, (cx, cy): (f64, f64)) -> (f64, f64, f64) {
    let (mut var_xx, mut var_yy, mut cov_xy) = (0.0, 0.0, 0.0);
    for (weight, x, y) in weights {
        let (dx, dy) = (x - cx, y - cy);
        var_xx += weight * dx * dx;
        var_yy += weight * dy * dy;
        cov_xy += weight * dx * dy;
    }
    (var_xx / mass, var_yy / mass, cov_xy / mass)
}

/// Maximum pairwise distance between present centroids; zero with fewer than two.
fn max_pairwise_distance(centroids: &[(f64, f64)]) -> f64 {
    if centroids.len() < 2 {
        return 0.0;
    }
    let mut spread = f64::NEG_INFINITY;
    for (index, first) in centroids.iter().enumerate() {
        for second in &centroids[index + 1..] {
            spread = spread.max((first.0 - second.0).hypot(first.1 - second.1));
        }
    }
    spread
}

/// Tests for the spot measurements.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a centred impulse gives the expected centroid and energy.
    #[test]
    fn test_centered_spot_centroid_and_energy() {
        let image = Rgba::from_fn(9, 9, |x, y| {
            let value = if (x, y) == (4, 4) { 2.0 } else { 0.0 };
            [value, value, value, 1.0]
        });
        let spot = bokeh_spot_metrics(
            &image,
            &[SpotPoint {
                xy: (4.0, 4.0),
                sign: "near".to_owned(),
            }],
        )
        .remove(0);
        let shape = spot.shape.expect("spot has mass");
        assert!((shape.centroid.0 - 4.0).abs() < 1e-9);
        assert!((shape.centroid.1 - 4.0).abs() < 1e-9);
        assert!((shape.energy - 2.0).abs() < 1e-6);
    }

    /// Verifies that the eigenvalues are returned in ascending order.
    #[test]
    fn test_eigenvalues_are_ascending() {
        let values = eigenvalues_2x2(3.0, 0.0, 1.0);
        assert_eq!(values, [1.0, 3.0]);
    }
}
