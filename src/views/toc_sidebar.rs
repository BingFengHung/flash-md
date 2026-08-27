use egui::{Align, Frame, Layout, Margin, RichText, Rounding, ScrollArea, Stroke};
use crate::theme::AppTheme;

/// 渲染 Markdown TOC 目錄大綱側邊欄，回傳 (是否收起大綱, 選取的目標標題錨點)
pub fn render_toc_sidebar(
    ui: &mut egui::Ui,
    theme: AppTheme,
    font_scale: f32,
    content: &str,
) -> (bool, Option<String>) {
    let mut should_close = false;
    let mut target_anchor = None;

    let toc = crate::markdown::extract_markdown_toc(content);

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("📑 目錄大綱")
                .strong()
                .size(13.0_f32 * font_scale)
                .color(theme.accent_color()),
        );
        if !toc.is_empty() {
            Frame::none()
                .fill(theme.code_bg_color())
                .rounding(Rounding::same(4.0_f32))
                .inner_margin(Margin::symmetric(5.0_f32, 1.0_f32))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{} 節", toc.len()))
                            .size(10.5_f32 * font_scale)
                            .color(theme.text_secondary()),
                    );
                });
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("✕").on_hover_text("收起大綱 (Ctrl+T)").clicked() {
                should_close = true;
            }
        });
    });
    ui.add_space(4.0_f32);
    ui.separator();
    ui.add_space(4.0_f32);

    if toc.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0_f32);
            ui.label(
                RichText::new("此文件無章節標題")
                    .italics()
                    .color(theme.text_secondary())
                    .size(12.0_f32 * font_scale),
            );
        });
        return (should_close, None);
    }

    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0_f32;
        for item in toc {
            let indent = ((item.level.saturating_sub(1)) as f32) * 10.0_f32 * font_scale;
            let (font_size, is_h1) = match item.level {
                1 => (12.5_f32 * font_scale, true),
                2 => (12.0_f32 * font_scale, false),
                3 => (11.5_f32 * font_scale, false),
                _ => (11.0_f32 * font_scale, false),
            };

            let item_resp = ui.horizontal(|ui| {
                if indent > 0.0_f32 {
                    ui.add_space(indent);
                }
                Frame::none()
                    .fill(theme.code_bg_color())
                    .rounding(Rounding::same(3.0_f32))
                    .stroke(Stroke::new(0.5_f32, if is_h1 { theme.accent_color() } else { theme.border_color() }))
                    .inner_margin(Margin::symmetric(4.0_f32, 1.0_f32))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("H{}", item.level))
                                .size(9.0_f32 * font_scale)
                                .color(if is_h1 { theme.accent_color() } else { theme.text_secondary() })
                                .strong(),
                        );
                    });

                let text_color = if is_h1 {
                    theme.text_primary()
                } else {
                    theme.text_secondary()
                };

                let label_text = if is_h1 {
                    RichText::new(&item.title).size(font_size).strong().color(text_color)
                } else {
                    RichText::new(&item.title).size(font_size).color(text_color)
                };

                ui.add(egui::Label::new(label_text).sense(egui::Sense::click()).truncate())
            }).inner;

            if item_resp.hovered() {
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
            }
            if item_resp.clicked() {
                target_anchor = Some(item.title.clone());
            }
        }
    });

    (should_close, target_anchor)
}
