//! Bottom status panel of the UI, showing the current status line and a detail line.

use std::sync::Arc;

use eframe::egui;

/// Draws the bottom status panel.
///
/// The first line shows `status` with its full text on hover. The second line shows the
/// error in red if there is one, otherwise a busy or dirty-state message, otherwise blank.
pub(crate) fn status_bar(
    ui: &mut egui::Ui,
    status: &str,
    dirty: bool,
    busy: bool,
    error: Option<&str>,
) {
    let ctx = ui.ctx().clone();
    let style = Arc::clone(ui.style());
    let frame = egui::Frame::side_top_panel(&style);
    let row_height =
        ctx.fonts_mut(|fonts| fonts.row_height(&egui::TextStyle::Body.resolve(&style)));
    let height = row_height * 2.0 + style.spacing.item_spacing.y + frame.total_margin().sum().y;
    egui::Panel::bottom("status")
        .exact_size(height)
        .frame(frame)
        .show(ui, |ui| {
            ui.add(egui::Label::new(status).truncate())
                .on_hover_text(status);
            let detail = error.unwrap_or(if busy {
                "Processing…"
            } else if dirty {
                "Settings changed — result needs rendering"
            } else {
                " "
            });
            let color = if error.is_some() {
                egui::Color32::LIGHT_RED
            } else {
                ui.visuals().text_color()
            };
            ui.add(egui::Label::new(egui::RichText::new(detail).color(color)).truncate())
                .on_hover_text(detail);
        });
}
