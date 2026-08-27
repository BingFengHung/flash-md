use std::path::Path;
use egui::{Color32, Frame, Margin, RichText, Rounding, ScrollArea, Stroke};
use crate::theme::AppTheme;

pub struct PresentationOutput {
    pub toggle_fullscreen: bool,
    pub exit_slides: bool,
}

/// 繪製全螢幕簡報投影模式畫布 (支援 --- 分頁、左右鍵翻頁、大字級投影卡片)
pub fn render_slides_mode(
    ui: &mut egui::Ui,
    theme: AppTheme,
    font_scale: f32,
    content: &str,
    base_dir: Option<&Path>,
    current_slide_index: &mut usize,
    is_slides_fullscreen: bool,
) -> PresentationOutput {
    let slides = crate::markdown::extract_slides(content);
    let total = slides.len();
    if *current_slide_index >= total {
        *current_slide_index = total.saturating_sub(1);
    }

    let slide_text = if total > 0 {
        slides[*current_slide_index].clone()
    } else {
        String::new()
    };

    let available_rect = ui.available_rect_before_wrap();
    let center_pos = available_rect.center();

    // 幻燈片主卡片：若全螢幕則填滿整個螢幕，若視窗模式則自適應填滿視窗
    let margin_x = if is_slides_fullscreen { 28.0_f32 } else { 20.0_f32 };
    let margin_top = if is_slides_fullscreen { 24.0_f32 } else { 16.0_f32 };
    let margin_bottom = if is_slides_fullscreen { 76.0_f32 } else { 62.0_f32 };

    let card_w = (available_rect.width() - margin_x * 2.0_f32).max(200.0_f32);
    let card_h = (available_rect.height() - margin_top - margin_bottom).max(150.0_f32);
    let card_rect = egui::Rect::from_min_size(
        egui::pos2(available_rect.min.x + margin_x, available_rect.min.y + margin_top),
        egui::vec2(card_w, card_h),
    );

    let card_bg = match theme {
        AppTheme::Dark => Color32::from_rgb(20, 22, 28),
        AppTheme::Light => Color32::from_rgb(255, 255, 255),
    };
    let card_stroke = Stroke::new(1.0_f32, theme.border_color());

    ui.painter().rect(
        card_rect,
        Rounding::same(12.0_f32),
        card_bg,
        card_stroke,
    );

    // 卡片內部渲染 Markdown 投影片
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card_rect), |ui| {
        let pad_h = if is_slides_fullscreen { 44.0_f32 } else { 28.0_f32 };
        let pad_v = if is_slides_fullscreen { 32.0_f32 } else { 20.0_f32 };
        Frame::none()
            .inner_margin(Margin::symmetric(pad_h, pad_v))
            .show(ui, |ui| {
                // 幻燈片頂部微型資訊
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("📽️ 簡報投影模式")
                            .size(11.5_f32)
                            .color(theme.accent_color())
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("第 {} / {} 頁", *current_slide_index + 1, total))
                                .size(12.0_f32)
                                .color(theme.text_secondary()),
                        );
                    });
                });

                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(10.0_f32);

                // 簡報內容 Markdown 渲染 (全螢幕使用 1.5x 字級，視窗模式使用 1.35x 字級)
                let scale_mult = if is_slides_fullscreen { 1.5_f32 } else { 1.35_f32 };
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let renderer = crate::markdown::MarkdownRenderer::new(
                            theme,
                            font_scale * scale_mult,
                            "",
                            None,
                            None,
                            base_dir,
                        );
                        let _ = renderer.render(ui, &slide_text);
                    });
            });
    });

    // 底部懸浮控制條 (Floating Pill Controller)
    let pill_height = 42.0_f32;
    let pill_width = 350.0_f32;
    let pill_rect = egui::Rect::from_min_size(
        egui::pos2(center_pos.x - pill_width / 2.0_f32, available_rect.max.y - pill_height - 14.0_f32),
        egui::vec2(pill_width, pill_height),
    );

    let pill_bg = match theme {
        AppTheme::Dark => Color32::from_rgba_premultiplied(15, 17, 23, 240),
        AppTheme::Light => Color32::from_rgba_premultiplied(240, 244, 250, 240),
    };

    ui.painter().rect(
        pill_rect,
        Rounding::same(21.0_f32),
        pill_bg,
        Stroke::new(1.0_f32, theme.accent_color()),
    );

    let mut next_slide = false;
    let mut prev_slide = false;
    let mut toggle_fullscreen = false;
    let mut exit_slides = false;

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(pill_rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0_f32;
            Frame::none()
                .inner_margin(Margin::symmetric(14.0_f32, 4.0_f32))
                .show(ui, |ui| {
                    let can_prev = *current_slide_index > 0;
                    let prev_btn = ui.add_enabled(
                        can_prev,
                        egui::Button::new(RichText::new("◀").size(13.0_f32).color(if can_prev { theme.text_primary() } else { theme.text_secondary() }))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                    );
                    if prev_btn.on_hover_text("上一頁 (← / PageUp / Backspace)").clicked() {
                        prev_slide = true;
                    }

                    ui.label(
                        RichText::new(format!("{}/{}", *current_slide_index + 1, total))
                            .size(13.0_f32)
                            .strong()
                            .color(theme.accent_color()),
                    );

                    let can_next = *current_slide_index + 1 < total;
                    let next_btn = ui.add_enabled(
                        can_next,
                        egui::Button::new(RichText::new("▶").size(13.0_f32).color(if can_next { theme.text_primary() } else { theme.text_secondary() }))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                    );
                    if next_btn.on_hover_text("下一頁 (→ / Space / PageDown)").clicked() {
                        next_slide = true;
                    }

                    ui.separator();

                    let fs_icon = if is_slides_fullscreen { "🗗 視窗" } else { "⛶ 全螢幕" };
                    let fs_btn = ui.add(
                        egui::Button::new(RichText::new(fs_icon).size(12.0_f32).color(theme.text_primary()))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                    );
                    if fs_btn.on_hover_text("切換全螢幕 (F / F11)").clicked() {
                        toggle_fullscreen = true;
                    }

                    let exit_btn = ui.add(
                        egui::Button::new(RichText::new("✕ 退出").size(12.0_f32).color(theme.text_primary()))
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                    );
                    if exit_btn.on_hover_text("退出簡報模式 (Esc / F5)").clicked() {
                        exit_slides = true;
                    }
                });
        });
    });

    if prev_slide && *current_slide_index > 0 {
        *current_slide_index -= 1;
    }
    if next_slide && *current_slide_index + 1 < total {
        *current_slide_index += 1;
    }

    // 螢幕最底部簡報進度條
    if total > 0 {
        let progress = (*current_slide_index + 1) as f32 / total as f32;
        let bar_width = available_rect.width() * progress;
        ui.painter().hline(
            available_rect.min.x..=available_rect.min.x + bar_width,
            available_rect.max.y - 2.0_f32,
            Stroke::new(3.0_f32, theme.accent_color()),
        );
    }

    PresentationOutput {
        toggle_fullscreen,
        exit_slides,
    }
}
