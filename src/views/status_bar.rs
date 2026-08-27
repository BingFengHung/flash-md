use egui::{RichText, Rounding, Stroke};
use crate::theme::AppTheme;
use crate::views::empty_state::CURRENT_VERSION;

/// 繪製導覽列現代按鈕元件
pub fn render_nav_button(
    ui: &mut egui::Ui,
    theme: AppTheme,
    label: &str,
    is_active: bool,
    tooltip: &str,
) -> egui::Response {
    let bg = if is_active {
        theme.accent_bg()
    } else {
        theme.code_bg_color()
    };
    let border = if is_active {
        theme.accent_color()
    } else {
        theme.border_color()
    };
    let text_color = if is_active {
        theme.accent_color()
    } else {
        theme.text_secondary()
    };

    let btn = egui::Button::new(
        RichText::new(label)
            .size(11.5_f32)
            .color(text_color),
    )
    .fill(bg)
    .stroke(Stroke::new(1.0_f32, border))
    .rounding(Rounding::same(5.0_f32));

    ui.add(btn).on_hover_text(tooltip)
}

/// 繪製底部快捷鍵操作提示
pub fn render_bottom_tips(ui: &mut egui::Ui, theme: AppTheme, is_editing: bool) {
    let tips = if is_editing {
        format!(
            "flash-md v{}  •  [就地編輯中]  •  Ctrl+S (保存)  •  E / Esc (退出編輯)  •  / (搜尋)",
            CURRENT_VERSION
        )
    } else {
        format!(
            "flash-md v{}  •  Alt+Space (預覽)  •  E (就地編輯)  •  ←/→/h/l (切換)  •  ↑/↓/j/k (捲動)  •  / (搜尋)  •  Ctrl+T (大綱)  •  Ctrl+Shift+O (定位)  •  Ctrl+M (模式)  •  Esc (隱藏)",
            CURRENT_VERSION
        )
    };
    ui.label(
        RichText::new(tips)
            .color(theme.text_secondary())
            .size(11.5_f32),
    );
}
