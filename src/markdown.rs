use crate::theme::AppTheme;
use egui::{
    text::LayoutJob, Align, Color32, FontId, Frame, Layout, Margin,
    RichText, Rounding, Stroke, Ui, Vec2,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn get_theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// 依搜尋關鍵字即時進行高亮分段附加
pub fn append_highlighted_text(
    job: &mut LayoutJob,
    text: &str,
    search_query: &str,
    base_format: egui::TextFormat,
    highlight_bg: Color32,
    highlight_fg: Color32,
) {
    let clean_query = search_query.trim();
    if clean_query.is_empty() {
        job.append(text, 0.0, base_format);
        return;
    }

    let text_lower = text.to_lowercase();
    let query_lower = clean_query.to_lowercase();

    // 以 char_indices 支援 Unicode 中文字元正確索引切片
    let mut last_char_idx = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let lower_chars: Vec<char> = text_lower.chars().collect();
    let query_chars: Vec<char> = query_lower.chars().collect();

    if query_chars.is_empty() || lower_chars.len() < query_chars.len() {
        job.append(text, 0.0, base_format);
        return;
    }

    let mut i = 0;
    while i + query_chars.len() <= lower_chars.len() {
        if lower_chars[i..i + query_chars.len()] == query_chars[..] {
            let start_byte = chars[i].0;
            let end_byte = if i + query_chars.len() < chars.len() {
                chars[i + query_chars.len()].0
            } else {
                text.len()
            };

            let last_byte = if last_char_idx < chars.len() {
                chars[last_char_idx].0
            } else {
                text.len()
            };

            if start_byte > last_byte {
                job.append(&text[last_byte..start_byte], 0.0, base_format.clone());
            }

            let mut hl_format = base_format.clone();
            hl_format.background = highlight_bg;
            hl_format.color = highlight_fg;
            job.append(&text[start_byte..end_byte], 0.0, hl_format);

            i += query_chars.len();
            last_char_idx = i;
        } else {
            i += 1;
        }
    }

    let last_byte = if last_char_idx < chars.len() {
        chars[last_char_idx].0
    } else {
        text.len()
    };

    if last_byte < text.len() {
        job.append(&text[last_byte..], 0.0, base_format);
    }
}

pub struct MarkdownRenderer<'a> {
    pub theme: AppTheme,
    pub font_scale: f32,
    pub search_query: &'a str,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> MarkdownRenderer<'a> {
    pub fn new(theme: AppTheme, font_scale: f32, search_query: &'a str) -> Self {
        Self {
            theme,
            font_scale,
            search_query,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn render(&self, ui: &mut Ui, markdown_text: &str) {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = Parser::new_ext(markdown_text, options);
        let mut context = RenderContext::new(self.theme, self.font_scale, self.search_query);

        for event in parser {
            context.process_event(ui, event);
        }

        // 刷新剩餘段落
        context.flush_inline(ui);
    }
}

struct InlineSpan {
    text: String,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
    link_url: Option<String>,
}

struct RenderContext<'a> {
    theme: AppTheme,
    font_scale: f32,
    search_query: &'a str,
    inlines: Vec<InlineSpan>,
    current_bold: bool,
    current_italic: bool,
    current_strikethrough: bool,
    current_link: Option<String>,
    in_code_block: bool,
    code_block_lang: String,
    code_block_content: String,
    in_heading: Option<HeadingLevel>,
    in_blockquote: bool,
    in_table: bool,
    table_headers: Vec<String>,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    table_alignments: Vec<Alignment>,
    in_table_head: bool,
    list_level: usize,
    ordered_list_index: Option<u64>,
}

impl<'a> RenderContext<'a> {
    fn new(theme: AppTheme, font_scale: f32, search_query: &'a str) -> Self {
        Self {
            theme,
            font_scale,
            search_query,
            inlines: Vec::new(),
            current_bold: false,
            current_italic: false,
            current_strikethrough: false,
            current_link: None,
            in_code_block: false,
            code_block_lang: String::new(),
            code_block_content: String::new(),
            in_heading: None,
            in_blockquote: false,
            in_table: false,
            table_headers: Vec::new(),
            table_rows: Vec::new(),
            current_row: Vec::new(),
            table_alignments: Vec::new(),
            in_table_head: false,
            list_level: 0,
            ordered_list_index: None,
        }
    }

    fn push_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_block_content.push_str(text);
        } else {
            self.inlines.push(InlineSpan {
                text: text.to_string(),
                bold: self.current_bold,
                italic: self.current_italic,
                strikethrough: self.current_strikethrough,
                code: false,
                link_url: self.current_link.clone(),
            });
        }
    }

    fn process_event(&mut self, ui: &mut Ui, event: Event) {
        match event {
            Event::Start(tag) => self.handle_start_tag(ui, tag),
            Event::End(tag) => self.handle_end_tag(ui, tag),
            Event::Text(text) => self.push_text(&text),
            Event::Code(code) => {
                if self.in_code_block {
                    self.code_block_content.push_str(&code);
                } else {
                    self.inlines.push(InlineSpan {
                        text: code.to_string(),
                        bold: self.current_bold,
                        italic: self.current_italic,
                        strikethrough: self.current_strikethrough,
                        code: true,
                        link_url: self.current_link.clone(),
                    });
                }
            }
            Event::Rule => {
                self.flush_inline(ui);
                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);
            }
            Event::SoftBreak => {
                self.push_text(" ");
            }
            Event::HardBreak => {
                self.push_text("\n");
            }
            Event::TaskListMarker(checked) => {
                self.flush_inline(ui);
                let check_str = if checked { "☑ " } else { "☐ " };
                ui.label(
                    RichText::new(check_str)
                        .color(if checked {
                            self.theme.accent_color()
                        } else {
                            self.theme.text_secondary()
                        })
                        .size(16.0_f32 * self.font_scale),
                );
            }
            _ => {}
        }
    }

    fn handle_start_tag(&mut self, ui: &mut Ui, tag: Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_inline(ui);
                self.in_heading = Some(level);
            }
            Tag::BlockQuote(..) => {
                self.flush_inline(ui);
                self.in_blockquote = true;
            }
            Tag::CodeBlock(kind) => {
                self.flush_inline(ui);
                self.in_code_block = true;
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_block_content.clear();
            }
            Tag::List(first_item) => {
                self.flush_inline(ui);
                self.list_level += 1;
                self.ordered_list_index = first_item;
            }
            Tag::Item => {
                self.flush_inline(ui);
            }
            Tag::Table(alignments) => {
                self.flush_inline(ui);
                self.in_table = true;
                self.table_alignments = alignments;
                self.table_headers.clear();
                self.table_rows.clear();
            }
            Tag::TableHead => {
                self.in_table_head = true;
                self.current_row.clear();
            }
            Tag::TableRow => {
                self.current_row.clear();
            }
            Tag::TableCell => {}
            Tag::Emphasis => self.current_italic = true,
            Tag::Strong => self.current_bold = true,
            Tag::Strikethrough => self.current_strikethrough = true,
            Tag::Link { dest_url, .. } => {
                self.current_link = Some(dest_url.to_string());
            }
            Tag::Image { dest_url, .. } => {
                self.flush_inline(ui);
                ui.add_space(4.0_f32);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("🖼️ [圖片連結]")
                            .color(self.theme.accent_color())
                            .italics(),
                    );
                    let resp = ui.add(egui::Hyperlink::from_label_and_url(
                        RichText::new(&dest_url.to_string()).underline(),
                        &dest_url.to_string(),
                    ));
                    if resp.hovered() {
                        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                    }
                });
                ui.add_space(4.0_f32);
            }
            _ => {}
        }
    }

    fn handle_end_tag(&mut self, ui: &mut Ui, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_inline(ui);
                ui.add_space(6.0_f32);
            }
            TagEnd::Heading(level) => {
                self.render_heading(ui, level);
                self.in_heading = None;
                ui.add_space(8.0_f32);
            }
            TagEnd::BlockQuote(..) => {
                self.flush_inline(ui);
                self.in_blockquote = false;
                ui.add_space(6.0_f32);
            }
            TagEnd::CodeBlock => {
                self.render_code_block(ui);
                self.in_code_block = false;
                self.code_block_lang.clear();
                self.code_block_content.clear();
                ui.add_space(8.0_f32);
            }
            TagEnd::List(_) => {
                self.flush_inline(ui);
                self.list_level = self.list_level.saturating_sub(1);
                self.ordered_list_index = None;
                ui.add_space(4.0_f32);
            }
            TagEnd::Item => {
                self.render_list_item(ui);
            }
            TagEnd::Table => {
                self.render_table(ui);
                self.in_table = false;
                ui.add_space(8.0_f32);
            }
            TagEnd::TableHead => {
                self.in_table_head = false;
                self.table_headers = std::mem::take(&mut self.current_row);
            }
            TagEnd::TableRow => {
                if !self.in_table_head {
                    self.table_rows.push(std::mem::take(&mut self.current_row));
                }
            }
            TagEnd::TableCell => {
                let cell_text: String = self.inlines.drain(..).map(|s| s.text).collect();
                self.current_row.push(cell_text);
            }
            TagEnd::Emphasis => self.current_italic = false,
            TagEnd::Strong => self.current_bold = false,
            TagEnd::Strikethrough => self.current_strikethrough = false,
            TagEnd::Link => self.current_link = None,
            _ => {}
        }
    }

    fn flush_inline(&mut self, ui: &mut Ui) {
        if self.inlines.is_empty() {
            return;
        }

        let inlines = std::mem::take(&mut self.inlines);

        if self.in_blockquote {
            // Blockquote 渲染
            Frame::none()
                .fill(self.theme.card_bg_color())
                .inner_margin(Margin::symmetric(10.0_f32, 6.0_f32))
                .rounding(Rounding::same(4.0_f32))
                .stroke(Stroke::new(3.0_f32, self.theme.quote_bar_color()))
                .show(ui, |ui| {
                    self.render_inline_spans(ui, inlines);
                });
        } else {
            self.render_inline_spans(ui, inlines);
        }
    }

    fn render_inline_spans(&self, ui: &mut Ui, spans: Vec<InlineSpan>) {
        let hl_bg = match self.theme {
            AppTheme::Dark => Color32::from_rgba_unmultiplied(234, 179, 8, 180),
            AppTheme::Light => Color32::from_rgb(254, 240, 138),
        };
        let hl_fg = match self.theme {
            AppTheme::Dark => Color32::BLACK,
            AppTheme::Light => Color32::from_rgb(113, 63, 18),
        };

        let mut job = LayoutJob::default();
        let mut has_hyperlinks = false;

        for span in &spans {
            if span.link_url.is_some() {
                has_hyperlinks = true;
                break;
            }
        }

        if !has_hyperlinks {
            for span in spans {
                let font_id = if span.code {
                    FontId::monospace(13.0_f32 * self.font_scale)
                } else {
                    FontId::proportional(14.5_f32 * self.font_scale)
                };

                let color = if span.code {
                    self.theme.accent_color()
                } else {
                    self.theme.text_primary()
                };

                let base_fmt = egui::TextFormat {
                    font_id,
                    color,
                    italics: span.italic,
                    strikethrough: Stroke::new(if span.strikethrough { 1.5_f32 } else { 0.0_f32 }, color),
                    line_height: Some(22.0_f32 * self.font_scale),
                    background: if span.code {
                        self.theme.code_bg_color()
                    } else {
                        Color32::TRANSPARENT
                    },
                    ..Default::default()
                };

                append_highlighted_text(&mut job, &span.text, self.search_query, base_fmt, hl_bg, hl_fg);
            }
            ui.label(job);
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0_f32;

                for span in spans {
                    if span.code {
                        // Inline Code Pill
                        let bg = self.theme.code_bg_color();
                        let border = self.theme.border_color();
                        Frame::none()
                            .fill(bg)
                            .rounding(Rounding::same(4.0_f32))
                            .stroke(Stroke::new(1.0_f32, border))
                            .inner_margin(Margin::symmetric(4.0_f32, 1.0_f32))
                            .show(ui, |ui| {
                                let mut code_job = LayoutJob::default();
                                let base_fmt = egui::TextFormat {
                                    font_id: FontId::monospace(13.0_f32 * self.font_scale),
                                    color: self.theme.accent_color(),
                                    ..Default::default()
                                };
                                append_highlighted_text(&mut code_job, &span.text, self.search_query, base_fmt, hl_bg, hl_fg);
                                ui.label(code_job);
                            });
                    } else if let Some(url) = span.link_url {
                        // Hyperlink
                        let link_text = RichText::new(&span.text)
                            .color(self.theme.accent_color())
                            .underline()
                            .size(14.0_f32 * self.font_scale);
                        let resp = ui.add(egui::Hyperlink::from_label_and_url(link_text, &url));
                        if resp.hovered() {
                            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                        }
                    } else {
                        let mut span_job = LayoutJob::default();
                        let base_fmt = egui::TextFormat {
                            font_id: FontId::proportional(14.5_f32 * self.font_scale),
                            color: self.theme.text_primary(),
                            italics: span.italic,
                            strikethrough: Stroke::new(if span.strikethrough { 1.5_f32 } else { 0.0_f32 }, self.theme.text_primary()),
                            line_height: Some(22.0_f32 * self.font_scale),
                            ..Default::default()
                        };
                        append_highlighted_text(&mut span_job, &span.text, self.search_query, base_fmt, hl_bg, hl_fg);
                        ui.label(span_job);
                    }
                }
            });
        }
    }

    fn render_heading(&mut self, ui: &mut Ui, level: HeadingLevel) {
        if self.inlines.is_empty() {
            return;
        }

        let heading_text: String = self.inlines.drain(..).map(|s| s.text).collect();
        let (size, is_h1_or_h2) = match level {
            HeadingLevel::H1 => (26.0 * self.font_scale, true),
            HeadingLevel::H2 => (21.0 * self.font_scale, true),
            HeadingLevel::H3 => (18.0 * self.font_scale, false),
            HeadingLevel::H4 => (16.0 * self.font_scale, false),
            HeadingLevel::H5 => (14.5 * self.font_scale, false),
            HeadingLevel::H6 => (13.0 * self.font_scale, false),
        };

        let hl_bg = match self.theme {
            AppTheme::Dark => Color32::from_rgba_unmultiplied(234, 179, 8, 180),
            AppTheme::Light => Color32::from_rgb(254, 240, 138),
        };
        let hl_fg = match self.theme {
            AppTheme::Dark => Color32::BLACK,
            AppTheme::Light => Color32::from_rgb(113, 63, 18),
        };

        let mut job = LayoutJob::default();
        let base_fmt = egui::TextFormat {
            font_id: FontId::proportional(size),
            color: self.theme.text_primary(),
            ..Default::default()
        };
        append_highlighted_text(&mut job, &heading_text, self.search_query, base_fmt, hl_bg, hl_fg);
        ui.label(job);

        if is_h1_or_h2 {
            ui.add_space(2.0);
            ui.separator();
        }
    }

    fn render_list_item(&mut self, ui: &mut Ui) {
        if self.inlines.is_empty() {
            return;
        }
        let inlines = std::mem::take(&mut self.inlines);
        let indent = (self.list_level.saturating_sub(1) as f32) * 16.0;

        ui.horizontal_wrapped(|ui| {
            ui.add_space(indent);
            let bullet = if let Some(idx) = self.ordered_list_index {
                format!("{}. ", idx)
            } else {
                "• ".to_string()
            };
            ui.label(
                RichText::new(bullet)
                    .color(self.theme.accent_color())
                    .strong()
                    .size(14.0 * self.font_scale),
            );

            self.render_inline_spans(ui, inlines);
        });

        if let Some(ref mut idx) = self.ordered_list_index {
            *idx += 1;
        }
    }

    fn render_code_block(&self, ui: &mut Ui) {
        let lang = self.code_block_lang.trim();
        let code = self.code_block_content.trim_end();

        Frame::none()
            .fill(self.theme.code_bg_color())
            .rounding(Rounding::same(6.0))
            .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                // 程式碼標頭工具列 (語言名稱 + 複製按鈕)
                ui.horizontal(|ui| {
                    let display_lang = if lang.is_empty() { "text" } else { lang };
                    ui.label(
                        RichText::new(display_lang.to_uppercase())
                            .font(FontId::monospace(11.0 * self.font_scale))
                            .color(self.theme.text_secondary())
                            .strong(),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(RichText::new("📋 複製").size(11.5 * self.font_scale))
                            .clicked()
                        {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(code.to_string());
                            }
                        }
                    });
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 語法高亮呈現 (使用 LinesWithEndings 精確解析完整換行語意)
                let syntax_set = get_syntax_set();
                let theme_set = get_theme_set();

                let syntect_theme = match self.theme {
                    AppTheme::Dark => &theme_set.themes["base16-eighties.dark"],
                    AppTheme::Light => &theme_set.themes["InspiredGitHub"],
                };

                let lang_lower = lang.to_lowercase();
                let syntax = syntax_set
                    .find_syntax_by_token(&lang_lower)
                    .or_else(|| syntax_set.find_syntax_by_extension(&lang_lower))
                    .or_else(|| {
                        match lang_lower.as_str() {
                            "rs" => syntax_set.find_syntax_by_name("Rust"),
                            "py" => syntax_set.find_syntax_by_name("Python"),
                            "js" | "mjs" | "cjs" => syntax_set.find_syntax_by_name("JavaScript"),
                            "jsx" => syntax_set.find_syntax_by_name("JavaScript (JSX)"),
                            "ts" => syntax_set.find_syntax_by_name("TypeScript"),
                            "tsx" => syntax_set.find_syntax_by_name("TypeScript (TSX)"),
                            "toml" => syntax_set.find_syntax_by_name("TOML"),
                            "yaml" | "yml" => syntax_set.find_syntax_by_name("YAML"),
                            "json" => syntax_set.find_syntax_by_name("JSON"),
                            "c" | "h" => syntax_set.find_syntax_by_name("C"),
                            "cpp" | "cc" | "cxx" | "hpp" => syntax_set.find_syntax_by_name("C++"),
                            "cs" => syntax_set.find_syntax_by_name("C#"),
                            "go" => syntax_set.find_syntax_by_name("Go"),
                            "java" => syntax_set.find_syntax_by_name("Java"),
                            "kt" | "kts" => syntax_set.find_syntax_by_name("Kotlin"),
                            "html" | "htm" => syntax_set.find_syntax_by_name("HTML"),
                            "css" => syntax_set.find_syntax_by_name("CSS"),
                            "scss" | "sass" => syntax_set.find_syntax_by_name("Sass"),
                            "sql" => syntax_set.find_syntax_by_name("SQL"),
                            "sh" | "bash" | "zsh" => syntax_set.find_syntax_by_name("Bourne Again Shell (bash)"),
                            "ps1" | "psm1" => syntax_set.find_syntax_by_name("PowerShell"),
                            "bat" | "cmd" => syntax_set.find_syntax_by_name("Batch File"),
                            "xml" | "svg" => syntax_set.find_syntax_by_name("XML"),
                            "lua" => syntax_set.find_syntax_by_name("Lua"),
                            "php" => syntax_set.find_syntax_by_name("PHP"),
                            "rb" => syntax_set.find_syntax_by_name("Ruby"),
                            _ => None,
                        }
                    })
                    .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

                let mut highlighter = HighlightLines::new(syntax, syntect_theme);
                let font_id = FontId::monospace(13.0 * self.font_scale);
                let mut layout_job = LayoutJob::default();

                let hl_bg = match self.theme {
                    AppTheme::Dark => Color32::from_rgba_unmultiplied(234, 179, 8, 180),
                    AppTheme::Light => Color32::from_rgb(254, 240, 138),
                };
                let hl_fg = match self.theme {
                    AppTheme::Dark => Color32::BLACK,
                    AppTheme::Light => Color32::from_rgb(113, 63, 18),
                };

                for line in syntect::util::LinesWithEndings::from(code) {
                    let ranges = highlighter
                        .highlight_line(line, syntax_set)
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
                            ..Default::default()
                        };
                        append_highlighted_text(&mut layout_job, text, self.search_query, base_fmt, hl_bg, hl_fg);
                    }
                }

                ui.label(layout_job);
            });
    }

    fn render_table(&self, ui: &mut Ui) {
        if self.table_headers.is_empty() && self.table_rows.is_empty() {
            return;
        }

        let num_cols = self.table_headers.len().max(
            self.table_rows
                .iter()
                .map(|r| r.len())
                .max()
                .unwrap_or(0),
        );

        if num_cols == 0 {
            return;
        }

        Frame::none()
            .fill(self.theme.card_bg_color())
            .rounding(Rounding::same(6.0))
            .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
            .inner_margin(Margin::same(8.0))
            .show(ui, |ui| {
                egui::Grid::new("markdown_table_grid")
                    .striped(true)
                    .spacing(Vec2::new(14.0, 8.0))
                    .show(ui, |ui| {
                        // Header
                        if !self.table_headers.is_empty() {
                            for (_i, header) in self.table_headers.iter().enumerate() {
                                let text = RichText::new(header)
                                    .strong()
                                    .size(13.5 * self.font_scale)
                                    .color(self.theme.text_primary());
                                ui.label(text);
                            }
                            ui.end_row();
                        }

                        // Rows
                        for row in &self.table_rows {
                            for (_i, cell) in row.iter().enumerate() {
                                let text = RichText::new(cell)
                                    .size(13.0 * self.font_scale)
                                    .color(self.theme.text_secondary());
                                ui.label(text);
                            }
                            ui.end_row();
                        }
                    });
            });
    }
}

/// 支援全語法高亮 + 行號 + 搜尋高亮的獨立程式碼檢視器
pub fn render_code_viewer(
    ui: &mut Ui,
    theme: AppTheme,
    font_scale: f32,
    code: &str,
    extension_or_lang: &str,
    search_query: &str,
) {
    let syntax_set = get_syntax_set();
    let theme_set = get_theme_set();

    let syntect_theme = match theme {
        AppTheme::Dark => &theme_set.themes["base16-eighties.dark"],
        AppTheme::Light => &theme_set.themes["InspiredGitHub"],
    };

    let lang_lower = extension_or_lang.to_lowercase();
    let syntax = syntax_set
        .find_syntax_by_extension(&lang_lower)
        .or_else(|| syntax_set.find_syntax_by_token(&lang_lower))
        .or_else(|| {
            match lang_lower.as_str() {
                "rs" => syntax_set.find_syntax_by_name("Rust"),
                "py" => syntax_set.find_syntax_by_name("Python"),
                "js" | "mjs" | "cjs" => syntax_set.find_syntax_by_name("JavaScript"),
                "jsx" => syntax_set.find_syntax_by_name("JavaScript (JSX)"),
                "ts" => syntax_set.find_syntax_by_name("TypeScript"),
                "tsx" => syntax_set.find_syntax_by_name("TypeScript (TSX)"),
                "toml" => syntax_set.find_syntax_by_name("TOML"),
                "yaml" | "yml" => syntax_set.find_syntax_by_name("YAML"),
                "json" => syntax_set.find_syntax_by_name("JSON"),
                "c" | "h" => syntax_set.find_syntax_by_name("C"),
                "cpp" | "cc" | "cxx" | "hpp" => syntax_set.find_syntax_by_name("C++"),
                "cs" => syntax_set.find_syntax_by_name("C#"),
                "go" => syntax_set.find_syntax_by_name("Go"),
                "java" => syntax_set.find_syntax_by_name("Java"),
                "kt" | "kts" => syntax_set.find_syntax_by_name("Kotlin"),
                "html" | "htm" => syntax_set.find_syntax_by_name("HTML"),
                "css" => syntax_set.find_syntax_by_name("CSS"),
                "scss" | "sass" => syntax_set.find_syntax_by_name("Sass"),
                "sql" => syntax_set.find_syntax_by_name("SQL"),
                "sh" | "bash" | "zsh" => syntax_set.find_syntax_by_name("Bourne Again Shell (bash)"),
                "ps1" | "psm1" => syntax_set.find_syntax_by_name("PowerShell"),
                "bat" | "cmd" => syntax_set.find_syntax_by_name("Batch File"),
                "xml" | "svg" => syntax_set.find_syntax_by_name("XML"),
                "lua" => syntax_set.find_syntax_by_name("Lua"),
                "php" => syntax_set.find_syntax_by_name("PHP"),
                "rb" => syntax_set.find_syntax_by_name("Ruby"),
                _ => None,
            }
        })
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, syntect_theme);

    let line_count = code.lines().count().max(1);
    let gutter_digits = format!("{}", line_count).len().max(2);

    let font_id = FontId::monospace(13.5 * font_scale);
    let gutter_color = theme.text_secondary().gamma_multiply(0.6);
    let border_color = theme.border_color();

    let hl_bg = match theme {
        AppTheme::Dark => Color32::from_rgba_unmultiplied(234, 179, 8, 180),
        AppTheme::Light => Color32::from_rgb(254, 240, 138),
    };
    let hl_fg = match theme {
        AppTheme::Dark => Color32::BLACK,
        AppTheme::Light => Color32::from_rgb(113, 63, 18),
    };

    // 容器卡片外框
    Frame::none()
        .fill(theme.card_bg_color())
        .rounding(Rounding::same(8.0))
        .stroke(Stroke::new(1.0_f32, border_color))
        .inner_margin(Margin::symmetric(16.0, 14.0))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // 1. 行號欄 (Line Numbers Gutter)
                ui.vertical(|ui| {
                    let mut gutter_job = LayoutJob::default();
                    for (i, _) in code.lines().enumerate() {
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
                    ui.label(gutter_job);
                });

                // 分隔垂直線
                ui.add_space(8.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, ui.available_height()), egui::Sense::hover());
                ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0_f32, border_color));
                ui.add_space(8.0);

                // 2. 程式碼語法高亮區域 (使用 LinesWithEndings 保持 \n 供 syntect 精確解析)
                ui.vertical(|ui| {
                    let mut code_job = LayoutJob::default();
                    for line in syntect::util::LinesWithEndings::from(code) {
                        let ranges = highlighter
                            .highlight_line(line, syntax_set)
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

                            append_highlighted_text(&mut code_job, text, search_query, base_fmt, hl_bg, hl_fg);
                        }
                    }

                    ui.label(code_job);
                });
            });
        });
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
        "png" => ("PNG 圖片".to_string(), "🖼️"),
        "jpg" | "jpeg" => ("JPEG 圖片".to_string(), "📷"),
        "svg" => ("SVG 向量圖".to_string(), "🎨"),
        "gif" => ("GIF 動態圖".to_string(), "🎬"),
        "webp" => ("WEBP 圖片".to_string(), "🌐"),
        "ico" => ("ICO 圖示".to_string(), "💠"),
        "bmp" => ("BMP 點陣圖".to_string(), "🖼️"),
        "tiff" | "tif" => ("TIFF 圖片".to_string(), "📸"),
        "avif" => ("AVIF 圖片".to_string(), "🌟"),
        _ => (format!("{} 圖片", ext.to_uppercase()), "🖼️"),
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
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "json" | "toml" | "yaml" | "yml"
            | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "cs" | "go" | "java" | "kt"
            | "html" | "css" | "scss" | "sass" | "sql" | "sh" | "bash" | "ps1" | "bat"
            | "cmd" | "xml" | "lua" | "php" | "rb" | "swift" | "dart" | "vue" | "svelte"
    )
}

/// 取得語言的美觀顯示名稱與 Emoji 徽章
pub fn get_language_badge(ext: &str) -> (String, &'static str) {
    match ext.to_lowercase().as_str() {
        "rs" => ("Rust".to_string(), "🦀"),
        "py" => ("Python".to_string(), "🐍"),
        "js" => ("JavaScript".to_string(), "⚡"),
        "jsx" => ("React JSX".to_string(), "⚛️"),
        "ts" => ("TypeScript".to_string(), "🔷"),
        "tsx" => ("React TSX".to_string(), "⚛️"),
        "json" => ("JSON".to_string(), "📦"),
        "toml" => ("TOML".to_string(), "⚙️"),
        "yaml" | "yml" => ("YAML".to_string(), "📄"),
        "c" => ("C".to_string(), "📘"),
        "cpp" | "cc" | "cxx" | "hpp" => ("C++".to_string(), "💠"),
        "cs" => ("C#".to_string(), "🟣"),
        "go" => ("Go".to_string(), "🐹"),
        "java" => ("Java".to_string(), "☕"),
        "kt" => ("Kotlin".to_string(), "🎯"),
        "html" => ("HTML".to_string(), "🌐"),
        "css" => ("CSS".to_string(), "🎨"),
        "scss" | "sass" => ("SCSS".to_string(), "🎨"),
        "sql" => ("SQL".to_string(), "🗄️"),
        "sh" | "bash" => ("Shell".to_string(), "🐚"),
        "ps1" => ("PowerShell".to_string(), "💻"),
        "bat" | "cmd" => ("Batch".to_string(), "📜"),
        "xml" => ("XML".to_string(), "📑"),
        "lua" => ("Lua".to_string(), "🌙"),
        "php" => ("PHP".to_string(), "🐘"),
        "rb" => ("Ruby".to_string(), "💎"),
        "swift" => ("Swift".to_string(), "🐦"),
        "dart" => ("Dart".to_string(), "🎯"),
        _ => (ext.to_uppercase(), "💻"),
    }
}


