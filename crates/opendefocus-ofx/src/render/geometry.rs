//! Pixel geometry for the render path: integer rectangles, the padded source request, the mapping
//! between canonical and pixel coordinates, and the renderer's frame coordinates.

use super::{
    HostSuites, IVector4, OFX_IMAGE_EFFECT_PROP_RENDER_SCALE, OFX_IMAGE_PROP_PIXEL_ASPECT_RATIO,
    OfxPropertySetHandle, Props, Settings, UVector2, Vector2,
};

/// Inclusive-exclusive pixel rectangle in OFX coordinates (x1, y1, x2, y2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rect {
    /// Left edge, inclusive.
    pub(super) x1: i32,
    /// Bottom edge, inclusive; OFX rows run bottom-up.
    pub(super) y1: i32,
    /// Right edge, exclusive.
    pub(super) x2: i32,
    /// Top edge, exclusive.
    pub(super) y2: i32,
}

impl Rect {
    /// Builds a rectangle from `[x1, y1, x2, y2]`, the order the OFX properties use.
    pub(super) fn from_ints(values: [i32; 4]) -> Self {
        Self {
            x1: values[0],
            y1: values[1],
            x2: values[2],
            y2: values[3],
        }
    }

    /// Width in pixels; `0` for an empty or inverted rectangle.
    pub(super) fn width(self) -> usize {
        (self.x2 - self.x1).max(0) as usize
    }

    /// Height in pixels; `0` for an empty or inverted rectangle.
    pub(super) fn height(self) -> usize {
        (self.y2 - self.y1).max(0) as usize
    }

    /// True when `other` lies entirely inside `self`, edges included.
    pub(super) fn contains(self, other: Rect) -> bool {
        self.x1 <= other.x1 && self.y1 <= other.y1 && self.x2 >= other.x2 && self.y2 >= other.y2
    }
}

/// True when the settings can read Source pixels beyond the padded window. Astigmatism folds
/// remote aperture coordinates back into the support, and a radius halo cannot bound that warp
/// (`adaptive_source_padding` in crates/opendefocus/src/lib.rs; ARCHITECTURE.md,
/// "Astigmatic support can reach remote source chunks"). Such requests cover the whole region.
pub(super) fn reads_remote_source(settings: &Settings) -> bool {
    settings.non_uniform.astigmatism.enable
}

/// The Source rectangle a window needs: the window grown by the renderer's `padding` on every
/// side, clamped to the region of definition `rod`. With `remote` (see [`reads_remote_source`])
/// the whole `rod` is needed, so the support is never cropped.
pub(super) fn source_request(window: Rect, rod: Rect, padding: u32, remote: bool) -> Rect {
    if remote {
        return rod;
    }
    let padding = i32::try_from(padding).unwrap_or(i32::MAX);
    Rect {
        x1: window.x1.saturating_sub(padding).max(rod.x1),
        y1: window.y1.saturating_sub(padding).max(rod.y1),
        x2: window.x2.saturating_add(padding).min(rod.x2),
        y2: window.y2.saturating_add(padding).min(rod.y2),
    }
}

/// An OFX rectangle in the renderer's frame coordinates: the origin is the top-left corner of
/// `rod` and y grows downwards (OFX row `rod.y2 - 1` is the top row).
pub(super) fn top_down(rect: Rect, rod: Rect) -> IVector4 {
    IVector4 {
        x: rect.x1 - rod.x1,
        y: rod.y2 - rect.y2,
        z: rect.x2 - rod.x1,
        w: rod.y2 - rect.y1,
    }
}
/// Maps canonical coordinates to pixels for one action (ofxImageEffect.h, "`CanonicalCoordinates`"
/// and kOfxImageEffectPropRenderScale). The render scale excludes the pixel aspect ratio, so
/// `pixel_x = canonical_x * scale_x / par` and `pixel_y = canonical_y * scale_y`. Image bounds,
/// regions of definition of images, and render windows are already in pixels; only the canonical
/// values from `GetRegionsOfInterest`, clipGetRegionOfDefinition and the `RoI` out-args go through here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PixelMap {
    /// Horizontal render scale from the host.
    pub(super) scale_x: f64,
    /// Vertical render scale from the host.
    pub(super) scale_y: f64,
    /// Pixel aspect ratio of the clip; it divides the horizontal scale only.
    pub(super) par: f64,
}

impl PixelMap {
    /// Rounds canonical (x1, y1, x2, y2) outwards to whole pixels.
    pub(super) fn to_pixels(self, values: [f64; 4]) -> Rect {
        let (x, y) = (self.scale_x / self.par, self.scale_y);
        Rect {
            x1: (values[0] * x).floor() as i32,
            y1: (values[1] * y).floor() as i32,
            x2: (values[2] * x).ceil() as i32,
            y2: (values[3] * y).ceil() as i32,
        }
    }

    /// The canonical (x1, y1, x2, y2) of a pixel rectangle.
    pub(super) fn to_canonical(self, rect: Rect) -> [f64; 4] {
        let (x, y) = (self.par / self.scale_x, 1.0 / self.scale_y);
        [
            f64::from(rect.x1) * x,
            f64::from(rect.y1) * y,
            f64::from(rect.x2) * x,
            f64::from(rect.y2) * y,
        ]
    }
}

/// The host's render scale (kOfxImageEffectPropRenderScale), `(1, 1)` when the host omits it.
pub(super) fn render_scale(in_props: &Props<'_>) -> (f64, f64) {
    // Hosts that do not implement the property render at full scale.
    let scale = |index| {
        in_props
            .get_double(OFX_IMAGE_EFFECT_PROP_RENDER_SCALE, index)
            .unwrap_or(1.0)
    };
    (scale(0), scale(1))
}

/// The pixel aspect ratio of a clip, `1` when the host omits it.
pub(super) fn clip_pixel_aspect(clip_props: OfxPropertySetHandle, suites: &HostSuites) -> f64 {
    Props::new(clip_props, suites.property)
        .get_double(OFX_IMAGE_PROP_PIXEL_ASPECT_RATIO, 0)
        .unwrap_or(1.0)
}
/// Sets the renderer's `resolution` and lens `center` for a frame with this region of
/// definition. The centre is the middle of the full frame, in the frame's own pixels, so catseye,
/// astigmatism, barndoors and the other lens effects stay centred whatever tile is rendered.
pub(super) fn set_frame(settings: &mut Settings, rod: Rect) {
    settings.render.resolution = UVector2 {
        x: rod.width() as u32,
        y: rod.height() as u32,
    };
    settings.render.center = Vector2 {
        x: rod.width() as f32 * 0.5,
        y: rod.height() as f32 * 0.5,
    };
}
/// Formats a rectangle for diagnostics, as `[x1 y1 x2 y2]`.
pub(super) fn rect_text(rect: Rect) -> String {
    format!("[{} {} {} {}]", rect.x1, rect.y1, rect.x2, rect.y2)
}

/// Intersection of two rectangles (possibly empty).
pub(super) fn intersect(a: Rect, b: Rect) -> Rect {
    Rect {
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
        x2: a.x2.min(b.x2),
        y2: a.y2.min(b.y2),
    }
}
