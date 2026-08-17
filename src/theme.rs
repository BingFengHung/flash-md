use egui::{Color32, FontId, Stroke, Visuals};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            AppTheme::Dark => Color32::from_rgb(24, 24, 27),   // zinc-900
            AppTheme::Light => Color32::from_rgb(250, 250, 250), // zinc-50
        }
    }

    pub fn card_bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(39, 39, 42),   // zinc-800
            AppTheme::Light => Color32::from_rgb(255, 255, 255), // white
        }
    }

    pub fn code_bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(20, 20, 22),   // zinc-950
            AppTheme::Light => Color32::from_rgb(244, 244, 245), // zinc-100
        }
    }

    pub fn text_primary(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(244, 244, 245), // zinc-100
            AppTheme::Light => Color32::from_rgb(24, 24, 27),   // zinc-900
        }
    }

    pub fn text_secondary(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(161, 161, 170), // zinc-400
            AppTheme::Light => Color32::from_rgb(113, 113, 122), // zinc-500
        }
    }

    pub fn border_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(63, 63, 70),   // zinc-700
            AppTheme::Light => Color32::from_rgb(228, 228, 231), // zinc-200
        }
    }

    pub fn accent_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(96, 165, 250),  // blue-400
            AppTheme::Light => Color32::from_rgb(37, 99, 235),  // blue-600
        }
    }

    pub fn quote_bar_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(82, 82, 91),   // zinc-600
            AppTheme::Light => Color32::from_rgb(212, 212, 216), // zinc-300
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
        visuals.window_stroke = Stroke::new(1.0, self.border_color());
        visuals.widgets.noninteractive.bg_fill = self.card_bg_color();
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border_color());
        visuals.widgets.inactive.bg_fill = self.card_bg_color();
        visuals.widgets.hovered.bg_fill = match self {
            AppTheme::Dark => Color32::from_rgb(50, 50, 56),
            AppTheme::Light => Color32::from_rgb(240, 240, 243),
        };
        visuals.widgets.active.bg_fill = self.accent_color();

        ctx.set_visuals(visuals);
    }
}
