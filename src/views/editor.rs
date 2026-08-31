use egui::{text::LayoutJob, FontId, Frame, Margin, Rounding, ScrollArea, Stroke, Vec2};
use crate::theme::AppTheme;

pub struct EditorOutput {
    pub changed: bool,
    pub new_line_count: usize,
}

/// 針對編輯器進行 Markdown 輕量語法著色與寬敞行距優化 (徹底解決字句黏在一起的問題)
pub fn highlight_markdown_for_editor(
    text: &str,
    theme: AppTheme,
    font_scale: f32,
    wrap_width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let normal_font = FontId::monospace(14.0_f32 * font_scale);
    let heading_font = FontId::monospace(15.0_f32 * font_scale);
    let line_height = Some(24.0_f32 * font_scale);

    let text_primary = theme.text_primary();
    let text_secondary = theme.text_secondary();
    let accent_color = theme.accent_color();
    let code_bg = theme.code_bg_color();

    let mut line_count = 0;
    const MAX_EDITOR_HIGHLIGHT_LINES: usize = 3500;

    for line in text.split_inclusive('\n') {
        line_count += 1;
        if line_count > MAX_EDITOR_HIGHLIGHT_LINES {
            job.append(
                line,
                0.0_f32,
                egui::TextFormat {
                    font_id: normal_font.clone(),
                    color: text_primary,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            // 標題行 (# 標題)
            job.append(
                line,
                0.0_f32,
                egui::TextFormat {
                    font_id: heading_font.clone(),
                    color: accent_color,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
        } else if trimmed.starts_with('>') {
            // 引用行 (> 區塊引用)
            job.append(
                line,
                0.0_f32,
                egui::TextFormat {
                    font_id: normal_font.clone(),
                    color: text_secondary,
                    italics: true,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
        } else if trimmed.starts_with("```") || trimmed.starts_with("---") {
            // 程式碼區塊標記或分隔線
            job.append(
                line,
                0.0_f32,
                egui::TextFormat {
                    font_id: normal_font.clone(),
                    color: accent_color,
                    background: code_bg,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            // 清單項目符號
            let bullet_len = line.len() - trimmed.len() + 2;
            let (bullet_part, rest) = line.split_at(bullet_len.min(line.len()));
            job.append(
                bullet_part,
                0.0_f32,
                egui::TextFormat {
                    font_id: normal_font.clone(),
                    color: accent_color,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
            job.append(
                rest,
                0.0_f32,
                egui::TextFormat {
                    font_id: normal_font.clone(),
                    color: text_primary,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
        } else {
            // 一般內文行 (享有舒適的 24px 行高與純淨等寬字型排版)
            job.append(
                line,
                0.0_f32,
                egui::TextFormat {
                    font_id: normal_font.clone(),
                    color: text_primary,
                    line_height,
                    valign: egui::Align::BOTTOM,
                    ..Default::default()
                },
            );
        }
    }

    job
}

/// 繪製全螢幕就地編輯模式畫布
pub fn render_editor(
    ui: &mut egui::Ui,
    theme: AppTheme,
    font_scale: f32,
    content: &mut String,
) -> EditorOutput {
    let scroll = ScrollArea::vertical().auto_shrink([false, false]);
    let mut changed = false;

    Frame::none()
        .fill(theme.card_bg_color())
        .inner_margin(Margin::symmetric(16.0_f32, 12.0_f32))
        .rounding(Rounding::same(8.0_f32))
        .stroke(Stroke::new(1.0_f32, theme.border_color()))
        .show(ui, |ui| {
            scroll.show(ui, |ui| {
                let available_w = ui.available_width();
                let available_h = (ui.available_height() - 8.0_f32).max(200.0_f32);

                let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
                    let job = highlight_markdown_for_editor(string, theme, font_scale, wrap_width);
                    ui.fonts(|f| f.layout_job(job))
                };

                let edit_resp = ui.add_sized(
                    Vec2::new(available_w, available_h),
                    egui::TextEdit::multiline(content)
                        .layouter(&mut layouter)
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .lock_focus(true),
                );

                if edit_resp.changed() {
                    changed = true;
                }
            });
        });

    let new_line_count = if changed {
        content.lines().count()
    } else {
        0
    };

    EditorOutput {
        changed,
        new_line_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_markdown_for_editor() {
        let text = "# Title\n> Quote\n```rust\nlet x = 1;\n```\n- item 1\nNormal text";
        let job = highlight_markdown_for_editor(text, AppTheme::Dark, 1.0, 500.0);
        assert_eq!(job.text, text);
        assert!(!job.sections.is_empty());
    }

    #[test]
    fn test_editor_empty_text() {
        let text = "";
        let job = highlight_markdown_for_editor(text, AppTheme::Light, 1.0, 500.0);
        assert_eq!(job.text, "");
    }
}

