use crate::theme::AppTheme;
use egui::{
    text::LayoutJob, Align, Color32, FontId, Frame, Layout, Margin,
    RichText, Rounding, Stroke, Ui, Vec2,
};
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
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

pub struct MarkdownRenderer<'a> {
    pub theme: AppTheme,
    pub font_scale: f32,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> MarkdownRenderer<'a> {
    pub fn new(theme: AppTheme, font_scale: f32) -> Self {
        Self {
            theme,
            font_scale,
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
        let mut context = RenderContext::new(self.theme, self.font_scale);

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

struct RenderContext {
    theme: AppTheme,
    font_scale: f32,
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

impl RenderContext {
    fn new(theme: AppTheme, font_scale: f32) -> Self {
        Self {
            theme,
            font_scale,
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
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
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
                        .size(16.0 * self.font_scale),
                );
            }
            _ => {}
        }
    }

    fn handle_start_tag(&mut self, ui: &mut Ui, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.flush_inline(ui);
            }
            Tag::Heading { level, .. } => {
                self.flush_inline(ui);
                self.in_heading = Some(level);
                ui.add_space(10.0);
            }
            Tag::BlockQuote(_) => {
                self.flush_inline(ui);
                self.in_blockquote = true;
            }
            Tag::CodeBlock(kind) => {
                self.flush_inline(ui);
                self.in_code_block = true;
                self.code_block_content.clear();
                self.code_block_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
            }
            Tag::List(start_num) => {
                self.flush_inline(ui);
                self.list_level += 1;
                self.ordered_list_index = start_num;
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
            Tag::TableCell => {
                // 清空收集 Cell inline
            }
            Tag::Emphasis => self.current_italic = true,
            Tag::Strong => self.current_bold = true,
            Tag::Strikethrough => self.current_strikethrough = true,
            Tag::Link { dest_url, .. } => {
                self.current_link = Some(dest_url.to_string());
            }
            _ => {}
        }
    }

    fn handle_end_tag(&mut self, ui: &mut Ui, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_inline(ui);
                ui.add_space(4.0);
            }
            TagEnd::Heading(level) => {
                self.render_heading(ui, level);
                self.in_heading = None;
                ui.add_space(8.0);
            }
            TagEnd::BlockQuote(_) => {
                self.flush_inline(ui);
                self.in_blockquote = false;
                ui.add_space(6.0);
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.render_code_block(ui);
                ui.add_space(8.0);
            }
            TagEnd::List(_) => {
                self.flush_inline(ui);
                if self.list_level > 0 {
                    self.list_level -= 1;
                }
                self.ordered_list_index = None;
                ui.add_space(4.0);
            }
            TagEnd::Item => {
                self.render_list_item(ui);
            }
            TagEnd::TableCell => {
                let cell_text: String = self.inlines.drain(..).map(|s| s.text).collect();
                self.current_row.push(cell_text);
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
            TagEnd::Table => {
                self.in_table = false;
                self.render_table(ui);
                ui.add_space(10.0);
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
                .inner_margin(Margin::symmetric(10.0, 6.0))
                .rounding(Rounding::same(4.0))
                .stroke(Stroke::new(3.0_f32, self.theme.quote_bar_color()))
                .show(ui, |ui| {
                    self.render_inline_spans(ui, inlines);
                });
        } else {
            self.render_inline_spans(ui, inlines);
        }
    }

    fn render_inline_spans(&self, ui: &mut Ui, spans: Vec<InlineSpan>) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;

            for span in spans {
                if span.code {
                    // Inline Code Pill
                    let bg = self.theme.code_bg_color();
                    let border = self.theme.border_color();
                    Frame::none()
                        .fill(bg)
                        .rounding(Rounding::same(4.0))
                        .stroke(Stroke::new(1.0_f32, border))
                        .inner_margin(Margin::symmetric(4.0, 1.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(&span.text)
                                    .font(FontId::monospace(13.0 * self.font_scale))
                                    .color(self.theme.accent_color()),
                            );
                        });
                } else if let Some(url) = span.link_url {
                    // Hyperlink
                    let link_text = RichText::new(&span.text)
                        .color(self.theme.accent_color())
                        .underline()
                        .size(14.0 * self.font_scale);
                    let resp = ui.add(egui::Hyperlink::from_label_and_url(link_text, &url));
                    if resp.hovered() {
                        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                    }
                } else {
                    let mut rich = RichText::new(&span.text)
                        .color(self.theme.text_primary())
                        .size(14.5 * self.font_scale);
                    if span.bold {
                        rich = rich.strong();
                    }
                    if span.italic {
                        rich = rich.italics();
                    }
                    if span.strikethrough {
                        rich = rich.strikethrough();
                    }
                    ui.label(rich);
                }
            }
        });
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

        ui.label(
            RichText::new(&heading_text)
                .size(size)
                .strong()
                .color(self.theme.text_primary()),
        );

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

                // 語法高亮呈現
                let syntax_set = get_syntax_set();
                let theme_set = get_theme_set();

                let syntect_theme = match self.theme {
                    AppTheme::Dark => &theme_set.themes["base16-eighties.dark"],
                    AppTheme::Light => &theme_set.themes["InspiredGitHub"],
                };

                let syntax = syntax_set
                    .find_syntax_by_token(lang)
                    .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

                let mut highlighter = HighlightLines::new(syntax, syntect_theme);

                let mut layout_job = LayoutJob::default();

                for line in code.lines() {
                    let ranges = highlighter
                        .highlight_line(line, syntax_set)
                        .unwrap_or_default();

                    for (style, text) in ranges {
                        let color = Color32::from_rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        );
                        layout_job.append(
                            text,
                            0.0,
                            egui::TextFormat {
                                font_id: FontId::monospace(13.0 * self.font_scale),
                                color,
                                ..Default::default()
                            },
                        );
                    }
                    layout_job.append(
                        "\n",
                        0.0,
                        egui::TextFormat {
                            font_id: FontId::monospace(13.0 * self.font_scale),
                            color: self.theme.text_primary(),
                            ..Default::default()
                        },
                    );
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
