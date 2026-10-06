use ab_glyph::{Font, FontRef};
use egui::{FontData, FontDefinitions, FontFamily};
use log::{info, warn};

/// Keep the baseline's distance from the font center equal across all fonts.
/// egui centers fallback row heights, so aligning ascents alone would still
/// leave Chinese/Emoji fallback glyphs above or below Latin glyphs.
fn align_font_baselines(fonts: &mut FontDefinitions) {
    let Some(reference) = fonts
        .families
        .get(&FontFamily::Proportional)
        .and_then(|family| family.first())
        .and_then(|name| fonts.font_data.get(name))
        .and_then(font_metrics)
    else {
        return;
    };
    for data in fonts.font_data.values_mut() {
        if let Some(metrics) = font_metrics(data) {
            data.tweak.baseline_offset_factor +=
                (reference.centered_baseline - metrics.centered_baseline) / metrics.baseline_scale;
        }
    }
}

struct FontMetrics {
    centered_baseline: f32,
    baseline_scale: f32,
}

fn font_metrics(data: &FontData) -> Option<FontMetrics> {
    let face = FontRef::try_from_slice_and_index(data.font.as_ref(), data.index).ok()?;
    let units = face.units_per_em()?;
    let height = face.height_unscaled() / units;
    let row_height = (face.height_unscaled() + face.line_gap_unscaled()) / units;
    let baseline_scale = height * data.tweak.scale;
    if !baseline_scale.is_finite() || baseline_scale <= 0.0 {
        return None;
    }
    Some(FontMetrics {
        centered_baseline: face.ascent_unscaled() / units
            + baseline_scale * data.tweak.baseline_offset_factor
            - 0.5 * row_height,
        baseline_scale,
    })
}

pub fn setup_system_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = system_font_definitions();
    align_font_baselines(&mut fonts);
    ctx.set_fonts(fonts);
}

fn system_font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    // 1. 優先尋找並載入 Windows 原生 CJK 正文字型 (微軟正黑體 msjh.ttc)
    let cjk_font_paths = [
        r"C:\Windows\Fonts\msjh.ttc",    // 微軟正黑體 (Traditional Chinese)
        r"C:\Windows\Fonts\msjhbd.ttc",  // 微軟正黑體 Bold
        r"C:\Windows\Fonts\msjhl.ttc",   // 微軟正黑體 Light
        r"C:\Windows\Fonts\msyh.ttc",    // 微軟雅黑
        r"C:\Windows\Fonts\mingliu.ttc", // 細明體
    ];

    let mut loaded_cjk = false;
    for path in cjk_font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            info!("成功載入 Windows CJK 系統字型: {}", path);
            fonts
                .font_data
                .insert("windows_cjk".to_owned(), FontData::from_owned(bytes));
            // 正文中英數優先使用同一套字型；不同備援字型另依實際字型度量校正。
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
        fonts
            .font_data
            .insert("segoe_emoji".to_owned(), FontData::from_owned(emoji_bytes));
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            let prop_pos = if loaded_cjk { 1 } else { 0 };
            prop.insert(prop_pos, "segoe_emoji".to_owned());
        }
    }

    // 3. 載入 Monospace 等寬編程字型 (Consolas / Cascadia Mono)
    let mono_font_paths = [
        r"C:\Windows\Fonts\consola.ttf", // Consolas (Windows 標準極致清晰等寬編程字型)
        r"C:\Windows\Fonts\CascadiaMono.ttf", // Cascadia Mono
        r"C:\Windows\Fonts\CascadiaCode.ttf", // Cascadia Code
        r"C:\Windows\Fonts\cour.ttf",    // Courier New
    ];

    let mut loaded_mono = false;
    for path in mono_font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            info!("成功載入 Windows Monospace 系統字型: {}", path);
            fonts
                .font_data
                .insert("windows_mono".to_owned(), FontData::from_owned(bytes));
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

    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typography_fallback_baselines_follow_actual_system_font_metrics() {
        let mut definitions = system_font_definitions();
        #[cfg(windows)]
        for name in ["windows_cjk", "windows_mono", "segoe_emoji"] {
            assert!(definitions.font_data.contains_key(name), "missing {name}");
        }
        let sample = if cfg!(windows) {
            "Ag012()[]中文，🙂🚀"
        } else {
            "Ag012()[]🙂🚀"
        };
        let before = egui::epaint::text::Fonts::new(1.0, 8192, definitions.clone());
        let before = before.layout_no_wrap(
            sample.into(),
            egui::FontId::monospace(13.5),
            egui::Color32::WHITE,
        );
        let spread = |galley: &egui::Galley| {
            let row = &galley.rows[0];
            let low = row
                .glyphs
                .iter()
                .map(|glyph| glyph.pos.y)
                .fold(f32::INFINITY, f32::min);
            let high = row
                .glyphs
                .iter()
                .map(|glyph| glyph.pos.y)
                .fold(f32::NEG_INFINITY, f32::max);
            high - low
        };
        align_font_baselines(&mut definitions);
        let mut largest = 0.0_f32;
        for dpi in [1.0_f32, 1.25, 1.5, 2.0] {
            let fonts = egui::epaint::text::Fonts::new(dpi, 8192, definitions.clone());
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                for scale in [0.7_f32, 1.0, 1.35, 1.8] {
                    for size in [11.0_f32, 13.5, 14.5, 26.0] {
                        let font = egui::FontId::new(size * scale, family.clone());
                        assert!(fonts.has_glyphs(&font, sample));
                        let galley =
                            fonts.layout_no_wrap(sample.into(), font.clone(), egui::Color32::WHITE);
                        let pixels = spread(&galley) * dpi;
                        largest = largest.max(pixels);
                        // Subpixel metrics and egui's pixel-sized font cache can
                        // round a fallback to the adjacent physical pixel.
                        assert!(
                            pixels <= 1.01,
                            "{font:?} dpi={dpi} spread={pixels}: {:?}",
                            galley.rows[0]
                                .glyphs
                                .iter()
                                .map(|g| (g.chr, g.pos.y))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
        println!(
            "TYPOGRAPHY system-fonts monospace_before_px={} maximum_after_px={largest}",
            spread(&before)
        );
    }
}
