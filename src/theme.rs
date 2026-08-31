use egui::{
    Color32, FontData, FontDefinitions, FontFamily, Rounding, Stroke, Visuals,
};
use log::{info, warn};

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
            AppTheme::Dark => Color32::from_rgb(18, 18, 22),     // 深邃高級黑炭色 (Zinc-950)
            AppTheme::Light => Color32::from_rgb(248, 249, 251), // 純淨柔和淺灰白
        }
    }

    pub fn card_bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(28, 28, 34),     // 卡片/導航列背景 (Zinc-900)
            AppTheme::Light => Color32::from_rgb(255, 255, 255), // 純白
        }
    }

    pub fn code_bg_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(13, 13, 16),     // 程式碼區塊暗黑底色
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
            AppTheme::Dark => Color32::from_rgb(46, 46, 56),     // 細緻暗邊框
            AppTheme::Light => Color32::from_rgb(226, 232, 240), // 淺色邊框
        }
    }

    pub fn accent_color(&self) -> Color32 {
        match self {
            AppTheme::Dark => Color32::from_rgb(56, 189, 248),  // 閃電青藍色 (Sky-400)
            AppTheme::Light => Color32::from_rgb(2, 132, 199),  // 鮮明天藍色 (Sky-600)
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

/// 載入 Windows 繁體中文、等寬編程字型與系統 Emoji 字型，徹底解決中文方塊 (Tofu)、Emoji 與等寬代碼排版問題
pub fn setup_system_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // 1. 優先尋找並載入 Windows 原生 CJK 正文字型 (微軟正黑體 msjh.ttc)
    // 微軟正黑體由微軟官方精心調校，中文、英文、數字、括號與標點共用完全相同的水平基準線與字距比例！
    let cjk_font_paths = [
        r"C:\Windows\Fonts\msjh.ttc",   // 微軟正黑體 (Traditional Chinese)
        r"C:\Windows\Fonts\msjhbd.ttc", // 微軟正黑體 Bold
        r"C:\Windows\Fonts\msjhl.ttc",  // 微軟正黑體 Light
        r"C:\Windows\Fonts\msyh.ttc",   // 微軟雅黑
        r"C:\Windows\Fonts\mingliu.ttc",// 細明體
    ];

    let mut loaded_cjk = false;
    for path in cjk_font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            info!("成功載入 Windows CJK 系統字型: {}", path);
            fonts.font_data.insert(
                "windows_cjk".to_owned(),
                FontData::from_owned(bytes),
            );
            // 必須置於 Proportional 第一位，讓中英數括號全來自同一個字型庫，徹底統一水平基準線！
            if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
                prop.insert(0, "windows_cjk".to_owned());
            }
            loaded_cjk = true;
            break;
        }
    }

    if !loaded_cjk {
        warn!("未能在系統目錄中找到 Windows CJK 字型檔案！");
    }

    // 2. 載入 Windows 系統 Segoe UI Emoji 字型 (置於 CJK 之後作為備援字型)
    // 如此一來，一般中英數文字絕不被 Emoji 字型的 ASCII 覆蓋，僅在遇到 Unicode Emoji 時才從 segoe_emoji 解析
    if let Ok(emoji_bytes) = std::fs::read(r"C:\Windows\Fonts\seguiemj.ttf") {
        info!("成功載入 Windows Segoe UI Emoji 字型");
        fonts.font_data.insert(
            "segoe_emoji".to_owned(),
            FontData::from_owned(emoji_bytes),
        );
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            let prop_pos = if loaded_cjk { 1 } else { 0 };
            prop.insert(prop_pos, "segoe_emoji".to_owned());
        }
    }

    // 3. 載入 Monospace 等寬編程字型 (Consolas / Cascadia Mono)
    let mono_font_paths = [
        r"C:\Windows\Fonts\consola.ttf",      // Consolas (Windows 標準極致清晰等寬編程字型)
        r"C:\Windows\Fonts\CascadiaMono.ttf", // Cascadia Mono
        r"C:\Windows\Fonts\CascadiaCode.ttf", // Cascadia Code
        r"C:\Windows\Fonts\cour.ttf",         // Courier New
    ];

    let mut loaded_mono = false;
    for path in mono_font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            info!("成功載入 Windows Monospace 系統字型: {}", path);
            fonts.font_data.insert(
                "windows_mono".to_owned(),
                FontData::from_owned(bytes),
            );
            if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
                mono.insert(0, "windows_mono".to_owned());
            }
            loaded_mono = true;
            break;
        }
    }

    // Monospace 備援加入 CJK 與 Emoji
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        if loaded_cjk {
            let pos = if loaded_mono { 1 } else { 0 };
            mono.insert(pos, "windows_cjk".to_owned());
        }
        if fonts.font_data.contains_key("segoe_emoji") {
            let pos = (if loaded_mono { 1 } else { 0 }) + (if loaded_cjk { 1 } else { 0 });
            mono.insert(pos, "segoe_emoji".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}
