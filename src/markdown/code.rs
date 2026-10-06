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

/// 支援全語法高亮 + 行號 + 搜尋高亮的獨立程式碼檢視器 (全量 LayoutJob 快取，秒開 100K 行超大檔案)
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

    let font_id = FontId::monospace(13.5 * font_scale);
    let gutter_color = theme.text_secondary().gamma_multiply(0.6);
    let border_color = theme.border_color();

    let (hl_bg, hl_fg, act_bg, act_fg) = match theme {
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

    // 快取整個檔案的高亮 LayoutJob，避免每幀在 60 FPS 下反覆進行 syntect 正則運算 (零堆疊分配雜湊)
    // Large files start in a safe, fast preview. Keep the expansion state in
    // egui's temporary data so the viewer can offer a real way to render the
    // remaining content without adding UI state to every caller.
    let expand_id = egui::Id::new(("code_viewer_expand", crate::parsers::content_hash(code)));
    let is_expanded = !search_query.trim().is_empty()
        || ui
            .ctx()
            .data(|d| d.get_temp::<bool>(expand_id).unwrap_or(false));

    let cache_id = egui::Id::new((
        "code_viewer_fast_v3",
        crate::parsers::content_hash(code),
        (font_scale * 100.0_f32) as u32,
        theme as u8,
        &lang_lower,
        is_expanded,
    ));

    let (gutter_job, mut code_job, total_line_count, displayed_line_count, is_truncated) =
        ui.ctx().data_mut(|d| {
            if let Some(cached) = d.get_temp::<(LayoutJob, LayoutJob, usize, usize, bool)>(cache_id)
            {
                cached.clone()
            } else {
                let mut gutter_job = LayoutJob::default();
                let mut code_job = LayoutJob::default();

                // 1. 極速位元組行數統計 (7MB 僅需 0.3ms，完全不卡主執行緒)
                let total_lines = code.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;

                // 2. 依照檔案大小動態決定安全預覽策略
                let is_huge_file = code.len() > 300 * 1024; // > 300 KB
                let max_render_lines = if is_expanded {
                    total_lines
                } else if is_huge_file {
                    1000
                } else {
                    3000
                };
                let max_highlight_lines = if is_huge_file { 200 } else { 2000 };
                const MAX_LINE_CHAR_LIMIT: usize = 1000;

                let default_text_color = match theme {
                    AppTheme::Dark => Color32::from_rgb(226, 232, 240),
                    AppTheme::Light => Color32::from_rgb(30, 41, 59),
                };

                let syntax_set = get_syntax_set();
                let theme_set = get_theme_set();
                let syntect_theme = match theme {
                    AppTheme::Dark => &theme_set.themes["base16-eighties.dark"],
                    AppTheme::Light => &theme_set.themes["InspiredGitHub"],
                };
                let syntax = find_syntax_by_lang(&lang_lower, syntax_set);
                let mut highlighter = HighlightLines::new(syntax, syntect_theme);

                let mut displayed_lines = 0;
                let mut has_line_truncation = false;

                // 3. 僅迭代需要預覽的行數，絕不浪費 CPU 遍歷整個 7MB 字串
                for line in code.lines().take(max_render_lines) {
                    displayed_lines += 1;

                    // 超長單行截斷防護 (例如 minified bundle)
                    let (chunk, is_line_truncated) =
                        if !is_expanded && line.len() > MAX_LINE_CHAR_LIMIT {
                            let boundary = line
                                .char_indices()
                                .nth(MAX_LINE_CHAR_LIMIT)
                                .map(|(idx, _)| idx)
                                .unwrap_or(line.len());
                            (&line[..boundary], true)
                        } else {
                            (line, false)
                        };
                    has_line_truncation |= is_line_truncated;

                    let line_with_nl = format!("{}\n", chunk);

                    if displayed_lines <= max_highlight_lines {
                        let ranges = highlighter
                            .highlight_line(&line_with_nl, syntax_set)
                            .unwrap_or_default();

                        for (style, text) in ranges {
                            let color = Color32::from_rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            );

                            let base_fmt = egui::TextFormat {
                                font_id: font_id.clone(),
                                color,
                                line_height: Some(21.0 * font_scale),
                                ..Default::default()
                            };

                            code_job.append(text, 0.0, base_fmt);
                        }
                    } else {
                        let base_fmt = egui::TextFormat {
                            font_id: font_id.clone(),
                            color: default_text_color,
                            line_height: Some(21.0 * font_scale),
                            ..Default::default()
                        };
                        code_job.append(&line_with_nl, 0.0, base_fmt);
                    }

                    if is_line_truncated {
                        let base_fmt = egui::TextFormat {
                            font_id: font_id.clone(),
                            color: theme.text_secondary(),
                            line_height: Some(21.0 * font_scale),
                            ..Default::default()
                        };
                        code_job.append(" ... [單行過長已截斷]\n", 0.0, base_fmt);
                    }
                }

                let gutter_digits = format!("{}", displayed_lines.max(1)).len().max(2);
                for i in 0..displayed_lines {
                    let line_num_str = format!("{:>width$}\n", i + 1, width = gutter_digits);
                    gutter_job.append(
                        &line_num_str,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: gutter_color,
                            line_height: Some(21.0 * font_scale),
                            ..Default::default()
                        },
                    );
                }

                let is_truncated =
                    !is_expanded && (total_lines > displayed_lines || has_line_truncation);
                let result = (
                    gutter_job,
                    code_job,
                    total_lines,
                    displayed_lines,
                    is_truncated,
                );
                d.insert_temp(cache_id, result.clone());
                result
            }
        });

    let mut match_count = 0;
    crate::search::highlight_job(
        &mut code_job,
        search_query,
        active_match_index,
        &mut match_count,
        hl_bg,
        hl_fg,
        act_bg,
        act_fg,
    );

    // 容器卡片外框
    Frame::none()
        .fill(theme.card_bg_color())
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0_f32, border_color))
        .inner_margin(Margin::symmetric(16.0, 14.0))
        .show(ui, |ui| {
            // 程式碼檢視器頂部工具列 (語言識別 + 行數 + 複製按鈕)
            ui.horizontal(|ui| {
                let (name, emoji) = get_language_badge(&lang_lower);
                ui.label(
                    RichText::new(format!("{} {}", emoji, name))
                        .font(FontId::monospace(11.5 * font_scale))
                        .color(theme.accent_color())
                        .strong(),
                );
                let line_desc = if is_truncated {
                    format!("•  {} 行 (已預覽前 {} 行)", total_line_count, displayed_line_count)
                } else {
                    format!("•  {} 行", total_line_count)
                };
                ui.label(
                    RichText::new(line_desc)
                        .size(11.0 * font_scale)
                        .color(theme.text_secondary()),
                );
                if is_truncated {
                    Frame::none()
                        .fill(theme.code_bg_color())
                        .rounding(Rounding::same(3.0))
                        .inner_margin(Margin::symmetric(5.0, 1.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("⚡ 大檔極速防護模式")
                                    .size(10.0 * font_scale)
                                    .color(theme.accent_color()),
                            );
                        });
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let copy_id = ui.make_persistent_id(format!("viewer_cb_copy_{:p}_{}", code.as_ptr(), code.len()));
                    let is_copied = ui.ctx().data(|d| {
                        d.get_temp::<std::time::Instant>(copy_id)
                            .map(|t| t.elapsed().as_secs_f32() < 2.0_f32)
                            .unwrap_or(false)
                    });

                    let btn_text = if is_copied {
                        RichText::new("✓ 已複製完整代碼")
                            .color(Color32::from_rgb(34, 197, 94))
                            .size(11.5 * font_scale)
                            .strong()
                    } else {
                        RichText::new("📋 複製完整代碼")
                            .color(theme.text_secondary())
                            .size(11.5 * font_scale)
                    };

                    if ui.button(btn_text).clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(code.to_string());
                        }
                        ui.ctx().data_mut(|d| d.insert_temp(copy_id, std::time::Instant::now()));
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal_top(|ui| {
                // 1. 行號欄 (Line Numbers Gutter)
                ui.vertical(|ui| {
                    ui.label(gutter_job);
                });

                // 分隔垂直線
                ui.add_space(8.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, (displayed_line_count as f32) * 21.0 * font_scale), egui::Sense::hover());
                ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0_f32, border_color));
                ui.add_space(8.0);

                // 2. 程式碼語法高亮區域 (使用快取的 LayoutJob，瞬時渲染)
                ui.vertical(|ui| {
                    crate::search::searchable_label(ui, code_job, search_query, active_match_index, search_jump, Sense::hover(), false);
                });
            });

            if is_truncated {
                ui.add_space(10.0);
                Frame::none()
                    .fill(theme.code_bg_color())
                    .rounding(Rounding::same(6.0))
                    .stroke(Stroke::new(1.0_f32, theme.accent_color().gamma_multiply(0.4)))
                    .inner_margin(Margin::symmetric(14.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "⚡ 檔案較大（共 {} 行），已為您極速安全預覽前 {} 行以維持 60 FPS 順暢體驗。點擊右上角「複製完整代碼」可提取完整內容。",
                                    total_line_count, displayed_line_count
                                ))
                                .color(theme.accent_color())
                                .size(11.5 * font_scale),
                            );
                            if ui.button("載入完整內容").clicked() {
                                ui.ctx().data_mut(|d| d.insert_temp(expand_id, true));
                                ui.ctx().request_repaint();
                            }
                        });
                    });
            }
        });
    match_count
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
