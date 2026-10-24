//! Subpixel focus resolution: removes thin crossing artifacts from prepared layers without softening focused surfaces.
use super::{AdaptiveLayerPixel, PreparedLayers};
use ndarray::Array3;

/// `CoC` magnitude in pixels from which a source pixel is visibly blurred when
/// measuring how far a focus band extends from its crossing.
const BLURRED_COC: f32 = 0.75;
/// Distance in pixels from a pixel centre to the edge of its footprint. A
/// crossing whose focused band reaches this far on either side is resolvable.
const HALF_PIXEL: f32 = 0.5;

impl PreparedLayers {
    /// Resolve focus bands narrower than one pixel after either CPU or GPU
    /// gathering. Focused surfaces with a focused neighbor remain sharp.
    pub(crate) fn resolve_subpixel_focus(&self, output: &mut Array3<f32>) {
        resolve_subpixel_focus_with(
            self.width,
            self.height,
            [0, 0],
            |x, y| self.pixels[y * self.width + x],
            output,
        );
    }

    /// Return a source copy with mixed RGB removed from prepared foreground
    /// silhouette pixels. `near_rgba` is coverage-premultiplied, so dividing by
    /// `near_valid` recovers the prepared foreground color used by both gathers.
    pub(crate) fn decontaminated_source(&self, source: &Array3<f32>) -> Option<Array3<f32>> {
        let mut corrected = None;
        for (index, pixel) in self.pixels.iter().enumerate() {
            let coverage = pixel.near_valid;
            if coverage <= 0.0 {
                continue;
            }
            let y = index / self.width;
            let x = index % self.width;
            #[allow(
                clippy::float_cmp,
                reason = "exact equality decides whether a pixel needs correction; a tolerance would change which pixels are rewritten"
            )]
            let needs_correction = coverage < 1.0
                || (0..4)
                    .any(|channel| source[[y, x, channel]] != pixel.near_rgba[channel] / coverage);
            if !needs_correction {
                continue;
            }
            let corrected = corrected.get_or_insert_with(|| source.clone());
            for channel in 0..4 {
                corrected[[y, x, channel]] = pixel.near_rgba[channel] / coverage;
            }
        }
        corrected
    }
}

/// Resolve narrow focus bands using a global prepared-pixel accessor while
/// writing only the supplied frame-space output region. Streamed callers can
/// read neighboring prepared cores without materializing a full-frame copy.
pub(crate) fn resolve_subpixel_focus_with<F>(
    frame_width: usize,
    frame_height: usize,
    output_origin: [usize; 2],
    pixel_at: F,
    output: &mut Array3<f32>,
) where
    F: Fn(usize, usize) -> AdaptiveLayerPixel,
{
    let output_height = output.dim().0;
    let output_width = output.dim().1;
    if output.dim().2 < 4
        || frame_width == 0
        || frame_height == 0
        || output_origin[0].saturating_add(output_width) > frame_width
        || output_origin[1].saturating_add(output_height) > frame_height
    {
        return;
    }
    let coc_at = |x: usize, y: usize| {
        let pixel = pixel_at(x, y);
        if pixel.near_valid > 0.0 {
            pixel.near_coc
        } else {
            pixel.far_coc
        }
    };
    let mut corrections = Vec::new();
    for local_y in 0..output_height {
        let y = output_origin[1] + local_y;
        for local_x in 0..output_width {
            let x = output_origin[0] + local_x;
            let pixel = pixel_at(x, y);
            if pixel.near_valid < 0.999 || pixel.near_coc.abs() > 0.25 {
                continue;
            }
            let mut crossing = None;
            let mut narrowest_reach = HALF_PIXEL;
            let center_coc = coc_at(x, y);
            for axis in 0..2 {
                let (dx, dy) = if axis == 0 { (1, 0) } else { (0, 1) };
                if x < 4 * dx
                    || y < 4 * dy
                    || x + 4 * dx >= frame_width
                    || y + 4 * dy >= frame_height
                    || local_x < 4 * dx
                    || local_y < 4 * dy
                    || local_x + 4 * dx >= output_width
                    || local_y + 4 * dy >= output_height
                {
                    continue;
                }
                let before = coc_at(x - dx, y - dy);
                let after = coc_at(x + dx, y + dy);
                let sign_crossing = before * after < 0.0;
                let local_focus_minimum = center_coc.abs() <= 0.25
                    && center_coc.abs() < before.abs()
                    && center_coc.abs() < after.abs();
                if !sign_crossing || !local_focus_minimum {
                    continue;
                }
                // Opposite signs distinguish a focus ramp from an object
                // contour. Only reconstruct a band too narrow to resolve: the
                // pixel footprint must reach visible blur on both sides. A
                // gentle side is a real focused band; reconstructing it
                // interpolates across sharp detail and leaves streaks.
                // shortcut: linear focus bands; use subpixel CoC samples if curved ramps still alias.
                let focused_reach = BLURRED_COC / before.abs().min(after.abs());
                if focused_reach >= narrowest_reach {
                    continue;
                }
                // Do not reach across a nearby focused surface or another
                // depth boundary while reconstructing the sampling footprint.
                if !(1..=4).all(|step| {
                    coc_at(x - step * dx, y - step * dy) * before > 0.0
                        && coc_at(x + step * dx, y + step * dy) * after > 0.0
                }) {
                    continue;
                }
                narrowest_reach = focused_reach;
                crossing = Some((dx, dy));
            }
            if let Some((dx, dy)) = crossing {
                // Layer preparation and bilinear filtering spread the isolated
                // column into its neighbors. Reconstruct that footprint too;
                // retaining sharp source color leaves a visible residual ridge.
                for step in 1..8 {
                    let px = x - 4 * dx + step * dx;
                    let py = y - 4 * dy + step * dy;
                    let Some(local_px) = px.checked_sub(output_origin[0]) else {
                        continue;
                    };
                    let Some(local_py) = py.checked_sub(output_origin[1]) else {
                        continue;
                    };
                    if local_px >= output_width || local_py >= output_height {
                        continue;
                    }
                    let t = step as f32 / 8.0;
                    let weight = (step.min(8 - step) as f32 * 0.5).min(1.0);
                    let rgba = std::array::from_fn::<_, 4, _>(|channel| {
                        let reconstructed = output[[local_y - 4 * dy, local_x - 4 * dx, channel]]
                            * (1.0 - t)
                            + output[[local_y + 4 * dy, local_x + 4 * dx, channel]] * t;
                        output[[local_py, local_px, channel]] * (1.0 - weight)
                            + reconstructed * weight
                    });
                    corrections.push((local_px, local_py, rgba));
                }
            }
        }
    }
    // Read every neighbor before writing so vertical/horizontal scans agree.
    for (x, y, rgba) in corrections {
        for channel in 0..4 {
            output[[y, x, channel]] = rgba[channel];
        }
    }
}
