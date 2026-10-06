use super::*;

/// 依語言副檔名或標記尋找最佳 Syntect 語法定義 (包含多層備援機制)
pub fn find_syntax_by_lang<'a>(
    lang_lower: &str,
    syntax_set: &'a SyntaxSet,
) -> &'a syntect::parsing::SyntaxReference {
    syntax_set
        .find_syntax_by_token(lang_lower)
        .or_else(|| syntax_set.find_syntax_by_extension(lang_lower))
        .or_else(|| match lang_lower {
            "rs" | "rust" => syntax_set.find_syntax_by_name("Rust"),
            "py" | "python" => syntax_set.find_syntax_by_name("Python"),
            "js" | "mjs" | "cjs" | "javascript" => syntax_set.find_syntax_by_name("JavaScript"),
            "jsx" => syntax_set
                .find_syntax_by_name("JavaScript (JSX)")
                .or_else(|| syntax_set.find_syntax_by_name("JavaScript")),
            "ts" | "typescript" => syntax_set
                .find_syntax_by_name("TypeScript")
                .or_else(|| syntax_set.find_syntax_by_name("JavaScript")),
            "tsx" => syntax_set
                .find_syntax_by_name("TypeScript (TSX)")
                .or_else(|| syntax_set.find_syntax_by_name("JavaScript (JSX)"))
                .or_else(|| syntax_set.find_syntax_by_name("JavaScript")),
            "toml" => syntax_set
                .find_syntax_by_name("TOML")
                .or_else(|| syntax_set.find_syntax_by_name("YAML")),
            "ini" | "conf" | "cfg" | "env" => syntax_set
                .find_syntax_by_name("INI")
                .or_else(|| syntax_set.find_syntax_by_name("YAML")),
            "yaml" | "yml" => syntax_set.find_syntax_by_name("YAML"),
            "json" | "json5" | "jsonc" => syntax_set.find_syntax_by_name("JSON"),
            "c" | "h" => syntax_set.find_syntax_by_name("C"),
            "cpp" | "cc" | "cxx" | "hpp" => syntax_set.find_syntax_by_name("C++"),
            "cs" | "csharp" => syntax_set.find_syntax_by_name("C#"),
            "go" | "golang" => syntax_set.find_syntax_by_name("Go"),
            "java" => syntax_set.find_syntax_by_name("Java"),
            "kt" | "kts" | "kotlin" => syntax_set
                .find_syntax_by_name("Kotlin")
                .or_else(|| syntax_set.find_syntax_by_name("Java")),
            "html" | "htm" | "xhtml" => syntax_set.find_syntax_by_name("HTML"),
            "css" => syntax_set.find_syntax_by_name("CSS"),
            "scss" | "sass" | "less" => syntax_set
                .find_syntax_by_name("Sass")
                .or_else(|| syntax_set.find_syntax_by_name("CSS")),
            "sql" => syntax_set.find_syntax_by_name("SQL"),
            "sh" | "bash" | "zsh" | "fish" | "shell" => syntax_set
                .find_syntax_by_name("Bourne Again Shell (bash)")
                .or_else(|| syntax_set.find_syntax_by_name("Shell-Unix-Generic")),
            "ps1" | "psm1" | "psd1" | "powershell" | "pwsh" | "ps" => syntax_set
                .find_syntax_by_name("PowerShell")
                .or_else(|| syntax_set.find_syntax_by_name("Bourne Again Shell (bash)"))
                .or_else(|| syntax_set.find_syntax_by_name("Shell-Unix-Generic")),
            "bat" | "cmd" | "batch" => syntax_set
                .find_syntax_by_name("Batch File")
                .or_else(|| syntax_set.find_syntax_by_name("Batch File (DOS)"))
                .or_else(|| syntax_set.find_syntax_by_name("Bourne Again Shell (bash)")),
            "dockerfile" | "containerfile" => syntax_set
                .find_syntax_by_name("Dockerfile")
                .or_else(|| syntax_set.find_syntax_by_name("Bourne Again Shell (bash)")),
            "xml" | "svg" => syntax_set.find_syntax_by_name("XML"),
            "lua" => syntax_set.find_syntax_by_name("Lua"),
            "php" => syntax_set.find_syntax_by_name("PHP"),
            "rb" | "ruby" => syntax_set.find_syntax_by_name("Ruby"),
            "graphql" | "gql" => syntax_set.find_syntax_by_name("JSON"),
            "vue" | "svelte" => syntax_set.find_syntax_by_name("HTML"),
            _ => None,
        })
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

mod highlighting;

use std::ops::Range;
use std::sync::Arc;

#[cfg(test)]
pub(crate) fn code_highlighting_ready(
    ctx: &egui::Context,
    code: &str,
    language: &str,
    theme: AppTheme,
) -> bool {
    let rows = highlighting::snapshot(
        ctx,
        crate::parsers::content_hash(code),
        code,
        language,
        theme,
    );
    let limit = if code.len() > 300 * 1024 { 200 } else { 2000 };
    rows.len() == code.split_inclusive('\n').take(limit).count()
}

struct CodeLayout {
    lines: Vec<Range<usize>>,
    total_lines: usize,
    row_height: f32,
    code_width: f32,
    gutter_width: f32,
    truncated: bool,
    expanded: bool,
}

fn code_layout(ui: &Ui, hash: u64, code: &str, scale: f32, expanded: bool) -> Arc<CodeLayout> {
    let id = egui::Id::new("flash-md-code-layout");
    let key = (
        hash,
        scale.to_bits(),
        ui.ctx().pixels_per_point().to_bits(),
        expanded,
    );
    if let Some((old, value)) = ui
        .ctx()
        .data(|store| store.get_temp::<((u64, u32, u32, bool), Arc<CodeLayout>)>(id))
    {
        if old == key {
            return value;
        }
    }
    let font = FontId::monospace(13.5 * scale);
    let mut metrics = crate::text_metrics::TextMetrics::new(ui, font.clone());
    let limit = if expanded {
        usize::MAX
    } else if code.len() > 300 * 1024 {
        1000
    } else {
        3000
    };
    let mut start = 0;
    let mut lines = Vec::new();
    let mut total_lines = 0;
    let mut code_width = 0.0_f32;
    let mut truncated = false;
    for line in code.split('\n') {
        total_lines += 1;
        if lines.len() < limit {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let boundary = if expanded {
                line.len()
            } else {
                line.char_indices()
                    .nth(1000)
                    .map_or(line.len(), |(index, _)| index)
            };
            let text = &line[..boundary];
            let mut width = metrics.width(text, true);
            if boundary < line.len() {
                truncated = true;
                width += metrics.width(" … [單行過長已截斷]", true) + 2.0;
            }
            code_width = code_width.max(width);
            lines.push(start..start + line.len());
        } else {
            truncated = true;
        }
        start += line.len() + 1;
    }
    let gutter_width = metrics
        .width(&total_lines.to_string(), true)
        .max(metrics.width("00", true))
        .ceil();
    let mut sample = LayoutJob::default();
    sample.append(
        "X",
        0.0,
        egui::TextFormat {
            font_id: font,
            line_height: Some(21.0 * scale),
            ..Default::default()
        },
    );
    let row_height = ui.fonts(|fonts| fonts.layout_job(sample)).size().y;
    let value = Arc::new(CodeLayout {
        lines,
        total_lines,
        row_height,
        code_width: code_width.ceil() + 2.0,
        gutter_width,
        truncated,
        expanded,
    });
    ui.ctx()
        .data_mut(|store| store.insert_temp(id, (key, value.clone())));
    value
}

struct CodePainter<'a> {
    code: &'a str,
    layout: &'a CodeLayout,
    colors: &'a [Vec<highlighting::ColorSpan>],
    matches: &'a [Range<usize>],
    active: Option<usize>,
    theme: AppTheme,
    scale: f32,
}

impl CodePainter<'_> {
    fn job(&self, line: usize) -> LayoutJob {
        let source = &self.layout.lines[line];
        let full_text = &self.code[source.clone()];
        let end = if self.layout.expanded {
            full_text.len()
        } else {
            full_text
                .char_indices()
                .nth(1000)
                .map_or(full_text.len(), |(index, _)| index)
        };
        let text = &full_text[..end];
        let format = egui::TextFormat {
            font_id: FontId::monospace(13.5 * self.scale),
            color: self.theme.text_primary(),
            line_height: Some(21.0 * self.scale),
            ..Default::default()
        };
        let mut job = LayoutJob::default();
        let mut position = 0;
        if let Some(spans) = self.colors.get(line) {
            for span in spans {
                let start = span.range.start.min(end);
                let stop = span.range.end.min(end);
                if start >= stop {
                    continue;
                }
                if position < start {
                    job.append(&text[position..start], 0.0, format.clone());
                }
                let mut colored = format.clone();
                colored.color = span.color;
                job.append(&text[start..stop], 0.0, colored);
                position = stop;
            }
        }
        if position < end || job.sections.is_empty() {
            job.append(&text[position..], 0.0, format.clone());
        }
        if end < full_text.len() {
            let mut note = format;
            note.color = self.theme.text_secondary();
            job.append(" … [單行過長已截斷]", 0.0, note);
        }
        let first = self
            .matches
            .partition_point(|range| range.end <= source.start);
        let local: Vec<_> = self.matches[first..]
            .iter()
            .enumerate()
            .take_while(|(_, range)| range.start < source.start + end)
            .filter_map(|(index, range)| {
                let start = range.start.max(source.start) - source.start;
                let stop = range.end.min(source.start + end) - source.start;
                (start < stop).then_some((first + index, start..stop))
            })
            .collect();
        let colors = match self.theme {
            AppTheme::Dark => (
                Color32::from_rgba_unmultiplied(234, 179, 8, 110),
                Color32::from_rgb(254, 240, 138),
                Color32::from_rgb(249, 115, 22),
                Color32::BLACK,
            ),
            AppTheme::Light => (
                Color32::from_rgb(254, 240, 138),
                Color32::from_rgb(113, 63, 18),
                Color32::from_rgb(234, 88, 12),
                Color32::WHITE,
            ),
        };
        crate::search::highlight_ranges(&mut job, &local, self.active, colors);
        job
    }

    fn galley(&self, ui: &Ui, line: usize) -> Arc<egui::Galley> {
        ui.fonts(|fonts| fonts.layout_job(self.job(line)))
    }
}

/// Reserve the complete scroll extent, but lay out only visible source lines.
/// Syntax parsing runs on a coalescing worker; uncolored text is readable while
/// colors arrive, and search always indexes the complete original source.
#[allow(clippy::too_many_arguments)]
pub fn render_code_viewer(
    ui: &mut Ui,
    theme: AppTheme,
    font_scale: f32,
    code: &str,
    extension_or_lang: &str,
    search_query: &str,
    active_match_index: Option<usize>,
    search_jump: bool,
) -> usize {
    let lang_lower = extension_or_lang.to_lowercase();
    let hash = crate::parsers::content_hash(code);
    let expand_id = egui::Id::new(("code_viewer_expand", hash));
    let expanded = !search_query.trim().is_empty()
        || ui
            .ctx()
            .data(|store| store.get_temp::<bool>(expand_id).unwrap_or(false));
    let layout = code_layout(ui, hash, code, font_scale, expanded);
    let colors = highlighting::snapshot(ui.ctx(), hash, code, &lang_lower, theme);
    let matches_id = egui::Id::new("flash-md-code-matches");
    let search_key = (hash, search_query.to_string());
    let cached = ui
        .ctx()
        .data(|store| store.get_temp::<((u64, String), Arc<Vec<Range<usize>>>)>(matches_id));
    let matches = if let Some((_, matches)) = cached.filter(|(old, _)| *old == search_key) {
        matches
    } else {
        let matches = Arc::new(crate::search::find_matches(code, search_query));
        ui.ctx()
            .data_mut(|store| store.insert_temp(matches_id, (search_key, matches.clone())));
        matches
    };
    let painter = CodePainter {
        code,
        layout: &layout,
        colors: &colors,
        matches: &matches,
        active: active_match_index,
        theme,
        scale: font_scale,
    };
    let font = FontId::monospace(13.5 * font_scale);
    Frame::none()
        .fill(theme.card_bg_color())
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0, theme.border_color()))
        .inner_margin(Margin::symmetric(16.0, 14.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (name, emoji) = get_language_badge(&lang_lower);
                ui.label(
                    RichText::new(format!("{emoji} {name}"))
                        .font(FontId::monospace(11.5 * font_scale))
                        .color(theme.accent_color())
                        .strong(),
                );
                let description = if layout.truncated {
                    format!(
                        "• {} 行（預覽前 {} 行）",
                        layout.total_lines,
                        layout.lines.len()
                    )
                } else {
                    format!("• {} 行", layout.total_lines)
                };
                ui.label(
                    RichText::new(description)
                        .size(11.0 * font_scale)
                        .color(theme.text_secondary()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let copy_id = ui.make_persistent_id(("viewer-copy", hash));
                    let copied = ui.ctx().data(|store| {
                        store
                            .get_temp::<std::time::Instant>(copy_id)
                            .is_some_and(|time| time.elapsed().as_secs_f32() < 2.0)
                    });
                    if ui
                        .button(if copied {
                            "✓ 已複製完整代碼"
                        } else {
                            "📋 複製完整代碼"
                        })
                        .clicked()
                    {
                        ui.ctx().copy_text(code.to_string());
                        ui.ctx().data_mut(|store| {
                            store.insert_temp(copy_id, std::time::Instant::now())
                        });
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs(2));
                    }
                });
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let width = (layout.gutter_width + 18.0 + layout.code_width).max(ui.available_width());
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(width, layout.lines.len() as f32 * layout.row_height),
                Sense::hover(),
            );
            let clip = ui.clip_rect();
            let first = (((clip.top() - rect.top()).max(0.0) / layout.row_height).floor() as usize)
                .min(layout.lines.len());
            let end = (((clip.bottom() - rect.top()).max(0.0) / layout.row_height).ceil() as usize)
                .min(layout.lines.len());
            let code_x = rect.left() + layout.gutter_width + 18.0;
            for line in first..end {
                let y = rect.top() + line as f32 * layout.row_height;
                let number = ui.fonts(|fonts| {
                    fonts.layout_no_wrap(
                        (line + 1).to_string(),
                        font.clone(),
                        theme.text_secondary().gamma_multiply(0.6),
                    )
                });
                ui.painter().galley(
                    egui::pos2(rect.left() + layout.gutter_width - number.size().x, y),
                    number,
                    theme.text_secondary(),
                );
                ui.painter().galley(
                    egui::pos2(code_x, y),
                    painter.galley(ui, line),
                    theme.text_primary(),
                );
            }
            let separator = rect.left() + layout.gutter_width + 8.0;
            ui.painter().vline(
                separator,
                rect.y_range(),
                Stroke::new(1.0, theme.border_color()),
            );
            if layout.truncated {
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("目前顯示部分內容；搜尋會涵蓋完整檔案，也可手動展開。")
                            .color(theme.accent_color())
                            .size(11.5 * font_scale),
                    );
                    if ui.button("載入完整內容").clicked() {
                        ui.ctx()
                            .data_mut(|store| store.insert_temp(expand_id, true));
                        ui.ctx().request_repaint();
                    }
                });
            }
            // Submit after allocating the whole document so distant matches
            // can scroll both axes even though their line was not painted.
            if search_jump {
                if let Some(range) = active_match_index.and_then(|index| matches.get(index)) {
                    let line = layout
                        .lines
                        .partition_point(|line| line.start <= range.start)
                        .saturating_sub(1);
                    if let Some(source) = layout.lines.get(line) {
                        let galley = painter.galley(ui, line);
                        let column = code[source.start..range.start.min(source.end)]
                            .chars()
                            .count();
                        let cursor = galley.from_ccursor(egui::text::CCursor::new(column));
                        let target = galley.pos_from_cursor(&cursor).translate(egui::vec2(
                            code_x,
                            rect.top() + line as f32 * layout.row_height,
                        ));
                        ui.scroll_to_rect(target, Some(Align::Center));
                    }
                }
            }
        });
    matches.len()
}

/// 判斷特定副檔名是否為圖片或向量圖類型
pub fn is_image_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" | "tiff" | "tif" | "avif"
    )
}

/// 取得圖片類型的美觀顯示名稱與 Emoji 徽章
pub fn get_image_badge(ext: &str) -> (String, &'static str) {
    match ext.to_lowercase().as_str() {
        "png" => ("PNG 圖片".to_string(), "🖼"),
        "jpg" | "jpeg" => ("JPEG 圖片".to_string(), "📷"),
        "svg" => ("SVG 向量圖".to_string(), "🎨"),
        "gif" => ("GIF 動態圖".to_string(), "🎬"),
        "webp" => ("WEBP 圖片".to_string(), "🌐"),
        "ico" => ("ICO 圖示".to_string(), "💠"),
        "bmp" => ("BMP 點陣圖".to_string(), "🖼"),
        "tiff" | "tif" => ("TIFF 圖片".to_string(), "📸"),
        "avif" => ("AVIF 圖片".to_string(), "🌟"),
        _ => (format!("{} 圖片", ext.to_uppercase()), "🖼"),
    }
}

/// 判斷特定副檔名是否為程式碼/設定檔類型
pub fn is_code_extension(ext: &str) -> bool {
    let syntax_set = get_syntax_set();
    if syntax_set.find_syntax_by_extension(ext).is_some() {
        return true;
    }
    matches!(
        ext.to_lowercase().as_str(),
        "rs" | "py"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "json"
            | "json5"
            | "jsonc"
            | "toml"
            | "yaml"
            | "yml"
            | "c"
            | "cpp"
            | "cc"
            | "cxx"
            | "h"
            | "hpp"
            | "cs"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "html"
            | "htm"
            | "xhtml"
            | "css"
            | "scss"
            | "sass"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "psm1"
            | "psd1"
            | "powershell"
            | "pwsh"
            | "ps"
            | "bat"
            | "cmd"
            | "xml"
            | "lua"
            | "php"
            | "rb"
            | "swift"
            | "dart"
            | "vue"
            | "svelte"
            | "csv"
            | "tsv"
            | "ini"
            | "conf"
            | "env"
            | "dockerfile"
            | "graphql"
            | "gql"
            | "diff"
            | "patch"
            | "log"
            | "r"
            | "scala"
            | "zig"
            | "proto"
    )
}

/// 取得語言的美觀顯示名稱與 Emoji 徽章
pub fn get_language_badge(ext: &str) -> (String, &'static str) {
    match ext.to_lowercase().as_str() {
        "rs" => ("Rust".to_string(), "🦀"),
        "py" => ("Python".to_string(), "🐍"),
        "js" | "mjs" | "cjs" => ("JavaScript".to_string(), "⚡"),
        "jsx" => ("React JSX".to_string(), "⚛"),
        "ts" => ("TypeScript".to_string(), "🔷"),
        "tsx" => ("React TSX".to_string(), "⚛"),
        "json" | "json5" | "jsonc" => ("JSON".to_string(), "📦"),
        "toml" => ("TOML".to_string(), "⚙"),
        "yaml" | "yml" => ("YAML".to_string(), "📄"),
        "csv" => ("CSV 表格".to_string(), "📊"),
        "tsv" => ("TSV 表格".to_string(), "📊"),
        "c" => ("C".to_string(), "📘"),
        "cpp" | "cc" | "cxx" | "hpp" => ("C++".to_string(), "💠"),
        "cs" => ("C#".to_string(), "🟣"),
        "go" => ("Go".to_string(), "🐹"),
        "java" => ("Java".to_string(), "☕"),
        "kt" | "kts" => ("Kotlin".to_string(), "🎯"),
        "html" | "htm" | "xhtml" => ("HTML".to_string(), "🌐"),
        "css" => ("CSS".to_string(), "🎨"),
        "scss" | "sass" => ("SCSS".to_string(), "🎨"),
        "sql" => ("SQL".to_string(), "🗄"),
        "sh" | "bash" | "zsh" | "fish" => ("Shell".to_string(), "🐚"),
        "ps1" | "psm1" | "psd1" | "powershell" | "pwsh" | "ps" => ("PowerShell".to_string(), "💻"),
        "bat" | "cmd" => ("Batch".to_string(), "📜"),
        "xml" => ("XML".to_string(), "📑"),
        "lua" => ("Lua".to_string(), "🌙"),
        "php" => ("PHP".to_string(), "🐘"),
        "rb" => ("Ruby".to_string(), "💎"),
        "swift" => ("Swift".to_string(), "🐦"),
        "dart" => ("Dart".to_string(), "🎯"),
        "vue" => ("Vue".to_string(), "💚"),
        "svelte" => ("Svelte".to_string(), "🧡"),
        "dockerfile" => ("Dockerfile".to_string(), "🐳"),
        "graphql" | "gql" => ("GraphQL".to_string(), "🔺"),
        "ini" | "conf" | "env" => ("Config".to_string(), "⚙"),
        "diff" | "patch" => ("Diff".to_string(), "🔄"),
        "log" => ("Log 記錄".to_string(), "📋"),
        "zig" => ("Zig".to_string(), "⚡"),
        "r" => ("R 語言".to_string(), "📈"),
        "scala" => ("Scala".to_string(), "🔴"),
        "proto" => ("Protobuf".to_string(), "📦"),
        _ => (ext.to_uppercase(), "💻"),
    }
}
