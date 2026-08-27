use egui::{Align, Align2, Layout, RichText, Vec2};
use crate::config::SaveMode;
use crate::theme::AppTheme;

pub struct SettingsModalOutput {
    pub is_open: bool,
    pub new_theme: Option<AppTheme>,
    pub new_save_mode: Option<SaveMode>,
    pub new_font_scale: Option<f32>,
}

/// 繪製偏好設定彈出對話框
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
        let mut save_mode = current_save_mode;
        let mut scale = current_font_scale;
        let accent_color = current_theme.accent_color();

        egui::Window::new("?? flash-md 偏好設定")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(340.0_f32);
                ui.add_space(4.0_f32);

                // 1. 主題設定
                ui.label(RichText::new("?? 外觀色彩主題").strong().color(accent_color));
                ui.horizontal(|ui| {
                    let light_selected = current_theme == AppTheme::Light;
                    let dark_selected = current_theme == AppTheme::Dark;
                    if ui.selectable_label(light_selected, "?? 亮色主題 (Light)").clicked() {
                        new_theme = Some(AppTheme::Light);
                    }
                    if ui.selectable_label(dark_selected, "?? 深色主題 (Dark)").clicked() {
                        new_theme = Some(AppTheme::Dark);
                    }
                });

                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // 2. 檔案保存模式
                ui.label(RichText::new("?? 編輯保存模式").strong().color(accent_color));
                ui.vertical(|ui| {
                    if ui.radio_value(&mut save_mode, SaveMode::Manual, "?? 按下 Ctrl + S 手動保存").clicked() {
                        new_save_mode = Some(SaveMode::Manual);
                    }
                    if ui.radio_value(&mut save_mode, SaveMode::AutoDebounce, "? 打字停止時自動防抖保存 (Auto-save 800ms)").clicked() {
                        new_save_mode = Some(SaveMode::AutoDebounce);
                    }
                });

                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // 3. 字型縮放
                ui.label(RichText::new("?? 字型顯示縮放").strong().color(accent_color));
                if ui.add(egui::Slider::new(&mut scale, 0.8_f32..=1.6_f32).text("比例")).changed() {
                    new_font_scale = Some(scale);
                }

                ui.add_space(10.0_f32);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("完成").clicked() {
                        close_settings = true;
                    }
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
