mod fonts;
pub use fonts::setup_system_cjk_fonts;

use egui::{Color32, Rounding, Stroke, Visuals};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AppTheme {
    Dark,
    Light,
}

impl AppTheme {
    pub fn toggle(&mut self) {
        *self = match self {
            AppTheme::Dark => AppTheme::Light,
            AppTheme::Light => AppTheme::Dark,
        };
    }

    pub fn bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(18, 18, 22), // 深邃高級黑炭色 (Zinc-950)
            AppTheme::Light => Color32::from_rgb(248, 249, 251), // 純淨柔和淺灰白
        }
    }

    pub fn card_bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(28, 28, 34), // 卡片/導航列背景 (Zinc-900)
            AppTheme::Light => Color32::from_rgb(255, 255, 255), // 純白
        }
    }

    pub fn code_bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(13, 13, 16), // 程式碼區塊暗黑底色
            AppTheme::Light => Color32::from_rgb(241, 243, 246), // 淺色程式碼底色
        }
    }

    pub fn text_primary(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(244, 244, 248), // 明亮白
            AppTheme::Light => Color32::from_rgb(20, 24, 33),   // 深邃深藍黑
        }
    }

    pub fn text_secondary(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(156, 163, 175), // 柔和灰 (Zinc-400)
            AppTheme::Light => Color32::from_rgb(107, 114, 128), // 次要文字灰
        }
    }

    pub fn border_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(46, 46, 56), // 細緻暗邊框
            AppTheme::Light => Color32::from_rgb(226, 232, 240), // 淺色邊框
        }
    }

    pub fn accent_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(56, 189, 248), // 閃電青藍色 (Sky-400)
            AppTheme::Light => Color32::from_rgb(2, 132, 199), // 鮮明天藍色 (Sky-600)
        }
    }

    pub fn accent_bg(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgba_unmultiplied(56, 189, 248, 30), // 淺透青光
            AppTheme::Light => Color32::from_rgba_unmultiplied(2, 132, 199, 25),
        }
    }

    pub fn quote_bar_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(56, 189, 248),
            AppTheme::Light => Color32::from_rgb(2, 132, 199),
        }
    }

    pub fn apply_to_ctx(&self, ctx: &egui::Context) {
        let mut visuals = match self {
            AppTheme::Dark => Visuals::dark(),
            AppTheme::Light => Visuals::light(),
        };

        visuals.override_text_color = Some(self.text_primary());
        visuals.panel_fill = self.bg_color();
        visuals.window_fill = self.bg_color();
        visuals.window_stroke = Stroke::new(1.0_f32, self.border_color());
        visuals.window_rounding = Rounding::same(10.0);

        // 按鈕與互動元件樣式 (無突兀厚重外框，柔和現代圓角)
        visuals.widgets.noninteractive.bg_fill = self.card_bg_color();
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border_color());
        visuals.widgets.noninteractive.rounding = Rounding::same(6.0);

        visuals.widgets.inactive.bg_fill = match self {
            AppTheme::Dark => Color32::from_rgb(34, 34, 42),
            AppTheme::Light => Color32::from_rgb(241, 243, 247),
        };
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border_color());
        visuals.widgets.inactive.rounding = Rounding::same(6.0);

        visuals.widgets.hovered.bg_fill = match self {
            AppTheme::Dark => Color32::from_rgb(48, 48, 60),
            AppTheme::Light => Color32::from_rgb(230, 235, 245),
        };
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent_color());
        visuals.widgets.hovered.rounding = Rounding::same(6.0);

        visuals.widgets.active.bg_fill = self.accent_color();
        visuals.widgets.active.rounding = Rounding::same(6.0);

        visuals.selection.bg_fill = self.accent_bg();
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent_color());

        ctx.set_visuals(visuals);
    }
}
