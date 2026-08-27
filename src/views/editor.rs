use egui::{FontId, ScrollArea, Vec2};
use crate::theme::AppTheme;

pub struct EditorOutput {
    pub changed: bool,
    pub new_line_count: usize,
}

/// 繪製全螢幕就地編輯模式畫布
pub fn render_editor(
    ui: &mut egui::Ui,
    theme: AppTheme,
    font_scale: f32,
    content: &mut String,
) -> EditorOutput {
    let text_color = theme.text_primary();
    let scroll = ScrollArea::vertical().auto_shrink([false, false]);
    let mut changed = false;

    scroll.show(ui, |ui| {
        ui.add_space(4.0_f32);
        let available_w = ui.available_width();
        let available_h = (ui.available_height() - 10.0_f32).max(200.0_f32);

        let font_id = FontId::monospace(14.0_f32 * font_scale);
        let edit_resp = ui.add_sized(
            Vec2::new(available_w, available_h),
            egui::TextEdit::multiline(content)
                .font(font_id)
                .text_color(text_color)
                .frame(false)
                .desired_width(f32::INFINITY)
                .lock_focus(true),
        );

        if edit_resp.changed() {
            changed = true;
        }
    });

    let new_line_count = if changed {
        content.lines().count()
    } else {
        0
    };

    EditorOutput {
        changed,
        new_line_count,
    }
}
