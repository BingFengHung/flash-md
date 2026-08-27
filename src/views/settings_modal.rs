use egui::{Align, Align2, Color32, Frame, Layout, Margin, RichText, Rounding, Stroke, Vec2};
use crate::config::SaveMode;
use crate::theme::AppTheme;

pub struct SettingsModalOutput {
    pub is_open: bool,
    pub new_theme: Option<AppTheme>,
    pub new_save_mode: Option<SaveMode>,
    pub new_font_scale: Option<f32>,
}

/// 繪製現代質感偏好設定彈出對話框 (Modern Fluent / macOS Card Style)
pub fn render_settings_modal(
    ctx: &egui::Context,
    is_open: bool,
    current_theme: AppTheme,
    current_save_mode: SaveMode,
    current_font_scale: f32,
) -> SettingsModalOutput {
    let mut open = is_open;
    let mut new_theme = None;
    let mut new_save_mode = None;
    let mut new_font_scale = None;
    let mut close_settings = false;

    if open {
        let mut scale = current_font_scale;
        let accent = current_theme.accent_color();
        let border_color = current_theme.border_color();
        let card_bg = current_theme.card_bg_color();
        let bg_color = current_theme.bg_color();
        let text_primary = current_theme.text_primary();
        let text_secondary = current_theme.text_secondary();

        egui::Window::new("⚙️  偏好設定")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(
                Frame::none()
                    .fill(bg_color)
                    .rounding(Rounding::same(14.0_f32))
                    .stroke(Stroke::new(1.2_f32, border_color))
                    .inner_margin(Margin::symmetric(22.0_f32, 18.0_f32)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(380.0_f32);
                ui.set_max_width(420.0_f32);

                // 頂部標題與副標
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("⚙️ flash-md 偏好設定")
                            .size(16.0_f32)
                            .strong()
                            .color(text_primary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(
                            egui::Button::new(RichText::new("✕").size(13.0_f32).color(text_secondary))
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                        ).on_hover_text("關閉 (Esc)").clicked() {
                            close_settings = true;
                        }
                    });
                });
                ui.label(
                    RichText::new("自訂介面色彩、保存行為與閱讀顯示體驗")
                        .size(11.5_f32)
                        .color(text_secondary),
                );

                ui.add_space(14.0_f32);

                // 區塊 1: 外觀主題 (Segmented Control 分段膠囊切換器)
                ui.label(RichText::new("🎨 外觀色彩主題").size(13.0_f32).strong().color(accent));
                ui.add_space(4.0_f32);

                Frame::none()
                    .fill(card_bg)
                    .rounding(Rounding::same(8.0_f32))
                    .stroke(Stroke::new(1.0_f32, border_color))
                    .inner_margin(Margin::same(4.0_f32))
                    .show(ui, |ui| {
                        ui.columns(2, |cols| {
                            // 亮色主題按鈕
                            let is_light = current_theme == AppTheme::Light;
                            let (bg_light, fg_light, stroke_light) = if is_light {
                                (accent, Color32::WHITE, Stroke::NONE)
                            } else {
                                (Color32::TRANSPARENT, text_primary, Stroke::NONE)
                            };

                            let btn_light = cols[0].add_sized(
                                [cols[0].available_width(), 32.0_f32],
                                egui::Button::new(
                                    RichText::new("☀️  亮色主題 (Light)")
                                        .size(12.5_f32)
                                        .strong()
                                        .color(fg_light),
                                )
                                .fill(bg_light)
                                .rounding(Rounding::same(6.0_f32))
                                .stroke(stroke_light),
                            );
                            if btn_light.clicked() && !is_light {
                                new_theme = Some(AppTheme::Light);
                            }

                            // 深色主題按鈕
                            let is_dark = current_theme == AppTheme::Dark;
                            let (bg_dark, fg_dark, stroke_dark) = if is_dark {
                                (accent, Color32::WHITE, Stroke::NONE)
                            } else {
                                (Color32::TRANSPARENT, text_primary, Stroke::NONE)
                            };

                            let btn_dark = cols[1].add_sized(
                                [cols[1].available_width(), 32.0_f32],
                                egui::Button::new(
                                    RichText::new("🌙  深色主題 (Dark)")
                                        .size(12.5_f32)
                                        .strong()
                                        .color(fg_dark),
                                )
                                .fill(bg_dark)
                                .rounding(Rounding::same(6.0_f32))
                                .stroke(stroke_dark),
                            );
                            if btn_dark.clicked() && !is_dark {
                                new_theme = Some(AppTheme::Dark);
                            }
                        });
                    });

                ui.add_space(14.0_f32);

                // 區塊 2: 檔案保存模式 (互動式卡片選擇器)
                ui.label(RichText::new("💾 編輯保存模式").size(13.0_f32).strong().color(accent));
                ui.add_space(4.0_f32);

                // 卡片 A: 手動保存
                let is_manual = current_save_mode == SaveMode::Manual;
                let card_a_stroke = if is_manual {
                    Stroke::new(1.5_f32, accent)
                } else {
                    Stroke::new(1.0_f32, border_color)
                };
                let card_a_bg = if is_manual {
                    match current_theme {
                        AppTheme::Dark => Color32::from_rgba_unmultiplied(56, 189, 248, 20),
                        AppTheme::Light => Color32::from_rgba_unmultiplied(2, 132, 199, 15),
                    }
                } else {
                    card_bg
                };

                let resp_a = Frame::none()
                    .fill(card_a_bg)
                    .rounding(Rounding::same(8.0_f32))
                    .stroke(card_a_stroke)
                    .inner_margin(Margin::symmetric(12.0_f32, 9.0_f32))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let icon = if is_manual { "🔘" } else { "⚪" };
                            ui.label(RichText::new(icon).size(13.0_f32));
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("按下 Ctrl + S 手動保存")
                                        .size(12.5_f32)
                                        .strong()
                                        .color(if is_manual { accent } else { text_primary }),
                                );
                                ui.label(
                                    RichText::new("適合精確控制，僅在主動確認時寫入磁碟")
                                        .size(11.0_f32)
                                        .color(text_secondary),
                                );
                            });
                        });
                    }).response;

                if resp_a.interact(egui::Sense::click()).clicked() && !is_manual {
                    new_save_mode = Some(SaveMode::Manual);
                }

                ui.add_space(6.0_f32);

                // 卡片 B: 自動防抖保存
                let is_auto = current_save_mode == SaveMode::AutoDebounce;
                let card_b_stroke = if is_auto {
                    Stroke::new(1.5_f32, accent)
                } else {
                    Stroke::new(1.0_f32, border_color)
                };
                let card_b_bg = if is_auto {
                    match current_theme {
                        AppTheme::Dark => Color32::from_rgba_unmultiplied(56, 189, 248, 20),
                        AppTheme::Light => Color32::from_rgba_unmultiplied(2, 132, 199, 15),
                    }
                } else {
                    card_bg
                };

                let resp_b = Frame::none()
                    .fill(card_b_bg)
                    .rounding(Rounding::same(8.0_f32))
                    .stroke(card_b_stroke)
                    .inner_margin(Margin::symmetric(12.0_f32, 9.0_f32))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let icon = if is_auto { "🔘" } else { "⚪" };
                            ui.label(RichText::new(icon).size(13.0_f32));
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("打字停止時自動防抖保存 (Auto-save 800ms)")
                                        .size(12.5_f32)
                                        .strong()
                                        .color(if is_auto { accent } else { text_primary }),
                                );
                                ui.label(
                                    RichText::new("無感即時同步，停止輸入 800ms 後自動存檔")
                                        .size(11.0_f32)
                                        .color(text_secondary),
                                );
                            });
                        });
                    }).response;

                if resp_b.interact(egui::Sense::click()).clicked() && !is_auto {
                    new_save_mode = Some(SaveMode::AutoDebounce);
                }

                ui.add_space(14.0_f32);

                // 區塊 3: 字型縮放 (帶百分比標籤與快速預設)
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍 字型顯示縮放").size(13.0_f32).strong().color(accent));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let pct = (scale * 100.0_f32).round() as u32;
                        ui.label(
                            RichText::new(format!("{}%", pct))
                                .size(12.5_f32)
                                .strong()
                                .color(accent),
                        );
                    });
                });
                ui.add_space(4.0_f32);

                Frame::none()
                    .fill(card_bg)
                    .rounding(Rounding::same(8.0_f32))
                    .stroke(Stroke::new(1.0_f32, border_color))
                    .inner_margin(Margin::symmetric(12.0_f32, 8.0_f32))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new("➖").small()).on_hover_text("縮小 (每次 -10%)").clicked() {
                                let next_s = (scale - 0.10_f32).clamp(0.7_f32, 1.8_f32);
                                new_font_scale = Some(next_s);
                            }

                            let slider = egui::Slider::new(&mut scale, 0.7_f32..=1.8_f32)
                                .show_value(false)
                                .step_by(0.05_f64);
                            if ui.add_sized([ui.available_width() - 80.0_f32, 20.0_f32], slider).changed() {
                                new_font_scale = Some(scale);
                            }

                            if ui.add(egui::Button::new("➕").small()).on_hover_text("放大 (每次 +10%)").clicked() {
                                let next_s = (scale + 0.10_f32).clamp(0.7_f32, 1.8_f32);
                                new_font_scale = Some(next_s);
                            }

                            if ui.add(egui::Button::new("100%").small()).on_hover_text("重置為預設 100%").clicked() {
                                new_font_scale = Some(1.0_f32);
                            }
                        });
                    });

                ui.add_space(18.0_f32);

                // 底部動作按鈕
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("💡 設定變更即刻生效並自動持久化儲存")
                            .size(10.5_f32)
                            .color(text_secondary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ok_btn = ui.add_sized(
                            [90.0_f32, 32.0_f32],
                            egui::Button::new(
                                RichText::new("✓ 完成")
                                    .size(13.0_f32)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(accent)
                            .rounding(Rounding::same(7.0_f32)),
                        );
                        if ok_btn.clicked() {
                            close_settings = true;
                        }
                    });
                });
            });
    }

    SettingsModalOutput {
        is_open: open && !close_settings,
        new_theme,
        new_save_mode,
        new_font_scale,
    }
}
