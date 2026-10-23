//! Zoom, pan and pixel mapping for the image shown in the central panel.

use eframe::egui::{self, TextureHandle};
use opendefocus::datamodel::IVector4;

/// Zoom and pan state of the image view.
#[derive(Default)]
pub(crate) struct ImageView {
    /// Zoom in screen points per image pixel, or `None` to fit the image to the viewport.
    pub(crate) zoom: Option<f32>,
    /// Offset of the image center from the viewport center, in screen points.
    pub(crate) pan: egui::Vec2,
    /// True while a middle-button drag pans the image.
    pub(crate) panning: bool,
}

impl ImageView {
    /// Returns the screen rectangle for an image of `size` inside the available area.
    ///
    /// Scrolling zooms around the pointer and middle-button dragging pans. The zoom and pan
    /// update in place, so the view keeps its state between frames.
    pub(crate) fn image_rect(&mut self, ui: &mut egui::Ui, size: egui::Vec2) -> egui::Rect {
        let (viewport, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let mut zoom = self
            .zoom
            .unwrap_or_else(|| (viewport.size() / size).min_elem());
        let (pointer, middle_pressed, middle_down, delta) = ui.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Middle),
                i.pointer.delta(),
            )
        });
        if response.contains_pointer() {
            let factor = ui.input_mut(|i| {
                let scroll = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta = egui::Vec2::ZERO;
                (scroll * 0.005).exp() * i.zoom_delta()
            });
            if (factor - 1.0).abs() > f32::EPSILON
                && let Some(pointer) = pointer
            {
                let new_zoom = (zoom * factor).clamp(0.01, 64.0);
                let offset = pointer - viewport.center();
                self.pan = offset - (offset - self.pan) * (new_zoom / zoom);
                zoom = new_zoom;
                self.zoom = Some(zoom);
            }
            if middle_pressed {
                self.panning = true;
            }
        }
        if !middle_down {
            self.panning = false;
        }
        if self.panning {
            if !middle_pressed {
                self.pan += delta;
            }
            self.zoom = Some(zoom);
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        egui::Rect::from_center_size(viewport.center() + self.pan, size * zoom)
    }
}

/// Maps a screen position inside `rect` to the image pixel it covers.
///
/// `size` is the image width and height in pixels. The result is clamped to the image bounds.
/// Returns `None` if the position is outside `rect`, `rect` is empty, or the image is empty.
pub(crate) fn image_pixel(
    rect: egui::Rect,
    position: egui::Pos2,
    size: [usize; 2],
) -> Option<[usize; 2]> {
    if !rect.contains(position) || rect.width() <= 0.0 || rect.height() <= 0.0 || size.contains(&0)
    {
        return None;
    }
    let uv = (position - rect.min) / rect.size();
    Some([
        ((uv.x * size[0] as f32) as usize).min(size[0] - 1),
        ((uv.y * size[1] as f32) as usize).min(size[1] - 1),
    ])
}

/// Returns the image pixel rectangle that is visible through `clip`.
///
/// The rectangle is rounded outward to whole pixels and limited to the image. Returns `None`
/// when nothing is visible or the image is empty.
pub(crate) fn image_region(
    rect: egui::Rect,
    clip: egui::Rect,
    size: [usize; 2],
) -> Option<IVector4> {
    let visible = rect.intersect(clip);
    if !visible.is_positive() || !rect.is_positive() || size.contains(&0) {
        return None;
    }
    let scale = egui::vec2(size[0] as f32, size[1] as f32) / rect.size();
    let min = (visible.min - rect.min) * scale;
    let max = (visible.max - rect.min) * scale;
    Some(IVector4 {
        x: min.x.floor().max(0.0) as i32,
        y: min.y.floor().max(0.0) as i32,
        z: max.x.ceil().min(size[0] as f32) as i32,
        w: max.y.ceil().min(size[1] as f32) as i32,
    })
}

/// Lays out `texture` in the available space and handles picking and drawing, as
/// [`viewer_image_at`] does. Test helper.
#[cfg(test)]
pub(crate) fn viewer_image(
    ui: &mut egui::Ui,
    texture: &TextureHandle,
    size: egui::Vec2,
    picking: bool,
    stripe: Option<IVector4>,
    focus_position: Option<[usize; 2]>,
) -> (Option<[usize; 2]>, Option<IVector4>) {
    let response = ui.allocate_response(size, egui::Sense::hover());
    let rect = ui.layout().align_size_within_rect(size, response.rect);
    viewer_image_at(ui, texture, rect, picking, stripe, focus_position)
}

/// Paints `texture` into `rect` and returns the pixel picked this frame and the visible region.
///
/// A stripe boundary is drawn as a white line, and the focus marker is drawn at
/// `focus_position`. The pixel is only returned when `picking` is set and the user clicks or
/// drags the marker.
pub(crate) fn viewer_image_at(
    ui: &egui::Ui,
    texture: &TextureHandle,
    rect: egui::Rect,
    picking: bool,
    stripe: Option<IVector4>,
    focus_position: Option<[usize; 2]>,
) -> (Option<[usize; 2]>, Option<IVector4>) {
    egui::Image::new(texture).paint_at(ui, rect);
    let response = ui.interact(
        rect,
        ui.scope_id().with("viewer-image"),
        if picking {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if let Some(stripe) = stripe {
        let scale = rect.size() / texture.size_vec2();
        let y = rect.top() + stripe.w as f32 * scale.y;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + stripe.x as f32 * scale.x, y),
                egui::pos2(rect.left() + stripe.z as f32 * scale.x, y),
            ],
            egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
        );
    }
    let visible = image_region(rect, ui.clip_rect(), texture.size());
    if let Some([x, y]) = focus_position {
        let scale = rect.size() / texture.size_vec2();
        let center = rect.min + egui::vec2(x as f32 + 0.5, y as f32 + 0.5) * scale;
        let dot = ui.interact(
            egui::Rect::from_center_size(center, egui::vec2(20.0, 20.0)).intersect(rect),
            ui.scope_id().with("focus-dot"),
            if picking {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::hover()
            },
        );
        ui.painter().circle_filled(
            center,
            4.0,
            if picking {
                egui::Color32::WHITE
            } else {
                egui::Color32::GRAY
            },
        );
        ui.painter().circle_stroke(
            center,
            4.0,
            egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
        );
        if picking {
            let dot = dot.on_hover_cursor(egui::CursorIcon::Grab);
            if (dot.dragged_by(egui::PointerButton::Primary)
                || dot.drag_stopped_by(egui::PointerButton::Primary))
                && let Some(position) = dot.interact_pointer_pos()
            {
                return (
                    image_pixel(rect, rect.clamp(position), texture.size()),
                    visible,
                );
            }
        }
    }
    if picking {
        let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
        if response.clicked_by(egui::PointerButton::Primary) {
            let picked = response
                .interact_pointer_pos()
                .and_then(|position| image_pixel(rect, position, texture.size()));
            return (picked, visible);
        }
    }
    (None, visible)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 50.0))
    }

    /// Checks that a pointer position maps to the image pixel under it, scaled by the view size.
    #[test]
    fn pixel_under_pointer_is_scaled_to_image_size() {
        assert_eq!(
            image_pixel(rect(), egui::pos2(50.0, 25.0), [10, 5]),
            Some([5, 2])
        );
        assert_eq!(
            image_pixel(rect(), egui::pos2(99.9, 49.9), [10, 5]),
            Some([9, 4])
        );
    }

    /// Checks that positions outside the view, empty views and empty images map to no pixel.
    #[test]
    fn pixel_is_none_outside_view_or_for_empty_inputs() {
        assert_eq!(image_pixel(rect(), egui::pos2(-1.0, 25.0), [10, 5]), None);
        assert_eq!(image_pixel(rect(), egui::pos2(50.0, 25.0), [0, 5]), None);
        let empty = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 50.0));
        assert_eq!(image_pixel(empty, egui::pos2(0.0, 25.0), [10, 5]), None);
    }

    /// Checks that a clipped view yields the outward-rounded pixel range that is visible.
    #[test]
    fn visible_region_rounds_outward_to_whole_pixels() {
        let clip = egui::Rect::from_min_max(egui::pos2(25.0, 0.0), egui::pos2(75.0, 50.0));
        assert_eq!(
            image_region(rect(), clip, [10, 5]),
            Some(IVector4 {
                x: 2,
                y: 0,
                z: 8,
                w: 5,
            })
        );
    }

    /// Checks that a clip wholly outside the view yields no visible region.
    #[test]
    fn visible_region_is_none_when_clip_misses_view() {
        let clip = egui::Rect::from_min_max(egui::pos2(200.0, 0.0), egui::pos2(300.0, 50.0));
        assert_eq!(image_region(rect(), clip, [10, 5]), None);
        assert_eq!(image_region(rect(), rect(), [0, 5]), None);
    }
}
