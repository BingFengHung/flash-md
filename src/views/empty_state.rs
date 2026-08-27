use egui::{Color32, FontId, Frame, Margin, RichText, Rounding, Stroke, Vec2};
use crate::theme::AppTheme;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 繪製擬真鍵盤按鍵 (Keycap) 元件
pub fn render_keycap(ui: &mut egui::Ui, theme: AppTheme, key_text: &str) {
    Frame::none()
        .fill(theme.code_bg_color())
        .rounding(Rounding::same(6.0_f32))
        .stroke(Stroke::new(1.0_f32, theme.border_color()))
        .inner_margin(Margin::symmetric(12.0_f32, 6.0_f32))
        .show(ui, |ui| {
            ui.label(
                RichText::new(key_text)
                    .font(FontId::monospace(13.0_f32))
                    .strong()
                    .color(theme.accent_color()),
            );
        });
}

/// 繪製極具現代質感的空狀態卡片介面 (Raycast / Linear Style)
pub fn render_empty_state(ui: &mut egui::Ui, theme: AppTheme, on_browse_click: impl FnOnce()) {
    ui.centered_and_justified(|ui| {
        Frame::none()
            .fill(theme.card_bg_color())
            .rounding(Rounding::same(12.0_f32))
            .stroke(Stroke::new(1.0_f32, theme.border_color()))
            .inner_margin(Margin::symmetric(36.0_f32, 32.0_f32))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    // 現代極光藍發光品牌圖示
                    Frame::none()
                        .fill(theme.accent_bg())
                        .rounding(Rounding::same(20.0_f32))
                        .stroke(Stroke::new(1.5_f32, theme.accent_color()))
                        .inner_margin(Margin::symmetric(14.0_f32, 10.0_f32))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("⚡")
                                    .size(26.0_f32)
                                    .strong()
                                    .color(theme.accent_color()),
                            );
                        });

                    ui.add_space(14.0_f32);

                    ui.label(
                        RichText::new(format!("flash-md v{}", CURRENT_VERSION))
                            .size(19.0_f32)
                            .strong()
                            .color(theme.text_primary()),
                    );

                    ui.add_space(6.0_f32);
                    ui.label(
                        RichText::new("Windows 快捷鍵極速檔案預覽 • 毫秒級渲染")
                            .size(13.0_f32)
                            .color(theme.text_secondary()),
                    );

                    ui.add_space(20.0_f32);

                    // 擬真實體鍵盤按鍵 UI
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0_f32;
                        render_keycap(ui, theme, "Alt");
                        ui.label(RichText::new("+").size(15.0_f32).color(theme.text_secondary()));
                        render_keycap(ui, theme, "Space");
                    });

                    ui.add_space(22.0_f32);

                    // 選擇檔案按鈕
                    let browse_btn = ui.add_sized(
                        Vec2::new(180.0_f32, 34.0_f32),
                        egui::Button::new(
                            RichText::new("📂 瀏覽開啟檔案")
                                .size(13.0_f32)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent_color())
                        .rounding(Rounding::same(7.0_f32)),
                    );

                    if browse_btn.clicked() {
                        on_browse_click();
                    }

                    ui.add_space(16.0_f32);
                    ui.separator();
                    ui.add_space(10.0_f32);

                    // 特色小標
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("⚡ 毫秒級預覽  •  📄 Markdown  •  💻 全語言程式碼高亮  •  🔄 即時同步")
                                .size(11.0_f32)
                                .color(theme.text_secondary()),
                        );
                    });
                });
            });
    });
}
