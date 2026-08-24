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

/// 依搜尋關鍵字即時進行高亮分段附加 (全 Unicode 安全切片，支援中英文與特殊字元，區分當前聚焦與一般相符)
pub fn append_highlighted_text(
    job: &mut LayoutJob,
    text: &str,
    search_query: &str,
    base_format: egui::TextFormat,
    normal_hl_bg: Color32,
    normal_hl_fg: Color32,
    active_hl_bg: Color32,
    active_hl_fg: Color32,
    active_match_idx: Option<usize>,
    match_counter: &mut usize,
) {
    let clean_query = search_query.trim();
    if clean_query.is_empty() || text.is_empty() {
        job.append(text, 0.0, base_format);
        return;
    }

    // ASCII 快速路徑：無任何堆疊分配，零拷貝極速掃描
    if text.is_ascii() && clean_query.is_ascii() {
        let query_lower = clean_query.to_ascii_lowercase();
        let text_lower = text.to_ascii_lowercase();
        let mut last_end = 0;
        let mut search_idx = 0;

        while let Some(pos) = text_lower[search_idx..].find(&query_lower) {
            let start = search_idx + pos;
            let end = start + query_lower.len();

            if start > last_end {
                job.append(&text[last_end..start], 0.0, base_format.clone());
            }

            let is_active = active_match_idx == Some(*match_counter);
            *match_counter += 1;

            let mut hl_fmt = base_format.clone();
            if is_active {
                hl_fmt.background = active_hl_bg;
                hl_fmt.color = active_hl_fg;
            } else {
                hl_fmt.background = normal_hl_bg;
                hl_fmt.color = normal_hl_fg;
            }
            job.append(&text[start..end], 0.0, hl_fmt);

            last_end = end;
            search_idx = end;
        }

        if last_end < text.len() {
            job.append(&text[last_end..], 0.0, base_format);
        }
        return;
    }

    // Unicode 多語系路徑：安全字元索引比對
    let query_lower: Vec<char> = clean_query.to_lowercase().chars().collect();
    let text_chars: Vec<(usize, char)> = text.char_indices().collect();

    let mut i = 0;
    let mut last_byte_idx = 0;

    while i + query_lower.len() <= text_chars.len() {
        let is_match = (0..query_lower.len()).all(|k| {
            text_chars[i + k].1.to_lowercase().eq(query_lower[k].to_lowercase())
        });

        if is_match {
            let start_byte = text_chars[i].0;
            let end_byte = if i + query_lower.len() < text_chars.len() {
                text_chars[i + query_lower.len()].0
            } else {
                text.len()
            };

            if start_byte > last_byte_idx {
                job.append(&text[last_byte_idx..start_byte], 0.0, base_format.clone());
            }

            let is_active = active_match_idx == Some(*match_counter);
            *match_counter += 1;

            let mut hl_fmt = base_format.clone();
            if is_active {
                hl_fmt.background = active_hl_bg;
                hl_fmt.color = active_hl_fg;
            } else {
                hl_fmt.background = normal_hl_bg;
                hl_fmt.color = normal_hl_fg;
            }
            job.append(&text[start_byte..end_byte], 0.0, hl_fmt);

            i += query_lower.len();
            last_byte_idx = end_byte;
        } else {
            i += 1;
        }
    }

    if last_byte_idx < text.len() {
        job.append(&text[last_byte_idx..], 0.0, base_format);
    }
}

/// 將標題或錨點字串正規化（去除符號、空格與 URL 編碼，保留中英文字母與數字）
pub fn normalize_anchor_slug(input: &str) -> String {
    let decoded = crate::explorer::url_decode(input);
    let trimmed = decoded.trim().trim_start_matches('#');
    trimmed
        .chars()
        .filter(|c| c.is_alphanumeric() || *c >= '\u{4E00}')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 智慧比對標題與目標錨點（支援精確比對、GitHub Slug 比對與中文字元子字串模糊匹配）
pub fn is_anchor_match(heading: &str, anchor: &str) -> bool {
    let clean_heading = heading.trim();
    let clean_anchor = anchor.trim().trim_start_matches('#');

    if clean_heading.eq_ignore_ascii_case(clean_anchor) {
        return true;
    }

    let slug_h = normalize_anchor_slug(clean_heading);
    let slug_a = normalize_anchor_slug(clean_anchor);

    if !slug_h.is_empty() && !slug_a.is_empty() {
        if slug_h == slug_a || slug_h.contains(&slug_a) || slug_a.contains(&slug_h) {
            return true;
        }
    }

    false
}

pub struct MarkdownRenderer<'a> {
    pub theme: AppTheme,
    pub font_scale: f32,
    pub search_query: &'a str,
    pub active_match_index: Option<usize>,
    pub target_anchor: Option<&'a str>,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> MarkdownRenderer<'a> {
    pub fn new(
        theme: AppTheme,
        font_scale: f32,
        search_query: &'a str,
        active_match_index: Option<usize>,
        target_anchor: Option<&'a str>,
    ) -> Self {
        Self {
            theme,
            font_scale,
            search_query,
            active_match_index,
            target_anchor,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn render(&self, ui: &mut Ui, markdown_text: &str) -> Option<String> {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = Parser::new_ext(markdown_text, options);
        let mut context = RenderContext::new(
            self.theme,
            self.font_scale,
            self.search_query,
            self.active_match_index,
            self.target_anchor,
        );

        for event in parser {
            context.process_event(ui, event);
        }

        // 刷新剩餘段落
        context.flush_inline(ui);

        context.clicked_anchor
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
    active_match_index: Option<usize>,
    target_anchor: Option<&'a str>,
    clicked_anchor: Option<String>,
    match_counter: usize,
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
    fn new(
        theme: AppTheme,
        font_scale: f32,
        search_query: &'a str,
        active_match_index: Option<usize>,
        target_anchor: Option<&'a str>,
    ) -> Self {
        Self {
            theme,
            font_scale,
            search_query,
            active_match_index,
            target_anchor,
            clicked_anchor: None,
            match_counter: 0,
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

    fn hl_colors(&self) -> (Color32, Color32, Color32, Color32) {
        match self.theme {
            AppTheme::Dark => (
                Color32::from_rgba_unmultiplied(234, 179, 8, 110), // 普通相符：柔和暗金黃底
                Color32::from_rgb(254, 240, 138),                  // 普通相符：淺金黃字
                Color32::from_rgb(249, 115, 22),                   // 當前 Focus 相符：耀眼亮橘橙底
                Color32::BLACK,                                    // 當前 Focus 相符：純黑字
            ),
            AppTheme::Light => (
                Color32::from_rgb(254, 240, 138),                  // 普通相符：柔和檸檬黃底
                Color32::from_rgb(113, 63, 18),                    // 普通相符：深褐色字
                Color32::from_rgb(234, 88, 12),                    // 當前 Focus 相符：深橘紅底
                Color32::WHITE,                                    // 當前 Focus 相符：純白字
            ),
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

    fn render_inline_spans(&mut self, ui: &mut Ui, spans: Vec<InlineSpan>) {
        let (hl_bg, hl_fg, act_bg, act_fg) = self.hl_colors();

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
                    FontId::monospace(13.5_f32 * self.font_scale)
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
                    valign: egui::Align::Center,
                    background: if span.code {
                        self.theme.code_bg_color()
                    } else {
                        Color32::TRANSPARENT
                    },
                    ..Default::default()
                };

                append_highlighted_text(
                    &mut job,
                    &span.text,
                    self.search_query,
                    base_fmt,
                    hl_bg,
                    hl_fg,
                    act_bg,
                    act_fg,
                    self.active_match_index,
                    &mut self.match_counter,
                );
            }
            ui.label(job);
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0_f32;

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
                                    font_id: FontId::monospace(13.5_f32 * self.font_scale),
                                    color: self.theme.accent_color(),
                                    valign: egui::Align::Center,
                                    ..Default::default()
                                };
                                append_highlighted_text(
                                    &mut code_job,
                                    &span.text,
                                    self.search_query,
                                    base_fmt,
                                    hl_bg,
                                    hl_fg,
                                    act_bg,
                                    act_fg,
                                    self.active_match_index,
                                    &mut self.match_counter,
                                );
                                ui.label(code_job);
                            });
                    } else if let Some(url) = span.link_url {
                        // Hyperlink or Internal Document Anchor Link (使用 LayoutJob + Label 確保與普通文字完全等高並水平對齊)
                        let mut link_job = LayoutJob::default();
                        let base_fmt = egui::TextFormat {
                            font_id: FontId::proportional(14.5_f32 * self.font_scale),
                            color: self.theme.accent_color(),
                            underline: Stroke::new(1.0_f32, self.theme.accent_color()),
                            italics: span.italic,
                            strikethrough: Stroke::new(if span.strikethrough { 1.5_f32 } else { 0.0_f32 }, self.theme.accent_color()),
                            line_height: Some(22.0_f32 * self.font_scale),
                            valign: egui::Align::Center,
                            ..Default::default()
                        };
                        append_highlighted_text(
                            &mut link_job,
                            &span.text,
                            self.search_query,
                            base_fmt,
                            hl_bg,
                            hl_fg,
                            act_bg,
                            act_fg,
                            self.active_match_index,
                            &mut self.match_counter,
                        );

                        let resp = ui.add(egui::Label::new(link_job).sense(egui::Sense::click()));
                        if resp.hovered() {
                            ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            if url.starts_with('#') {
                                self.clicked_anchor = Some(url.trim_start_matches('#').to_string());
                            } else {
                                let _ = open::that(&url);
                            }
                        }
                        resp.on_hover_text(&url);
                    } else {
                        let mut span_job = LayoutJob::default();
                        let base_fmt = egui::TextFormat {
                            font_id: FontId::proportional(14.5_f32 * self.font_scale),
                            color: self.theme.text_primary(),
                            italics: span.italic,
                            strikethrough: Stroke::new(if span.strikethrough { 1.5_f32 } else { 0.0_f32 }, self.theme.text_primary()),
                            line_height: Some(22.0_f32 * self.font_scale),
                            valign: egui::Align::Center,
                            ..Default::default()
                        };
                        append_highlighted_text(
                            &mut span_job,
                            &span.text,
                            self.search_query,
                            base_fmt,
                            hl_bg,
                            hl_fg,
                            act_bg,
                            act_fg,
                            self.active_match_index,
                            &mut self.match_counter,
                        );
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

        let (hl_bg, hl_fg, act_bg, act_fg) = self.hl_colors();

        let mut job = LayoutJob::default();
        let base_fmt = egui::TextFormat {
            font_id: FontId::proportional(size),
            color: self.theme.text_primary(),
            ..Default::default()
        };
        append_highlighted_text(
            &mut job,
            &heading_text,
            self.search_query,
            base_fmt,
            hl_bg,
            hl_fg,
            act_bg,
            act_fg,
            self.active_match_index,
            &mut self.match_counter,
        );
        let heading_resp = ui.label(job);

        if let Some(target) = self.target_anchor {
            if is_anchor_match(&heading_text, target) {
                heading_resp.scroll_to_me(Some(egui::Align::TOP));
            }
        }

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

/// 快取 Mermaid 圖表渲染結果（避免每次捲動 frame 重複編譯 SVG，大幅提升效能）
pub fn get_or_render_mermaid(code: &str) -> Option<String> {
    use std::sync::Mutex;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    static CACHE: Mutex<Option<HashMap<u64, Option<String>>>> = Mutex::new(None);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    let key = hasher.finish();

    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);

    if let Some(cached) = map.get(&key) {
        return cached.clone();
    }

    let rendered = mermaid_rs_renderer::render(code).ok();
    map.insert(key, rendered.clone());
    rendered
}

    fn render_code_block(&mut self, ui: &mut Ui) {
        let lang = self.code_block_lang.trim();
        let code = self.code_block_content.trim_end();

        // 1. Mermaid 向量流程圖即時渲染 (具備記憶體快取與 60fps 滑順捲動)
        if lang.eq_ignore_ascii_case("mermaid") && !code.trim().is_empty() {
            if let Some(svg_str) = get_or_render_mermaid(code) {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                code.hash(&mut hasher);
                let code_hash = hasher.finish();

                ui.add_space(4.0_f32);
                Frame::none()
                    .fill(self.theme.card_bg_color())
                    .rounding(Rounding::same(8.0_f32))
                    .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                    .inner_margin(Margin::same(12.0_f32))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("📊 Mermaid 流程圖")
                                    .font(FontId::proportional(12.0_f32 * self.font_scale))
                                    .color(self.theme.accent_color())
                                    .strong(),
                            );

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let copy_id = ui.make_persistent_id(format!("md_mermaid_copy_{:x}", code_hash));
                                let is_copied = ui.ctx().data(|d| {
                                    d.get_temp::<std::time::Instant>(copy_id)
                                        .map(|t| t.elapsed().as_secs_f32() < 2.0_f32)
                                        .unwrap_or(false)
                                });

                                let btn_text = if is_copied {
                                    RichText::new("✓ 已複製代碼")
                                        .color(Color32::from_rgb(34, 197, 94))
                                        .size(11.5_f32 * self.font_scale)
                                        .strong()
                                } else {
                                    RichText::new("📋 複製代碼")
                                        .color(self.theme.text_secondary())
                                        .size(11.5_f32 * self.font_scale)
                                };

                                if ui.button(btn_text).clicked() {
                                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                        let _ = clipboard.set_text(code.to_string());
                                    }
                                    ui.ctx().data_mut(|d| d.insert_temp(copy_id, std::time::Instant::now()));
                                }
                            });
                        });

                        ui.add_space(6.0_f32);
                        ui.separator();
                        ui.add_space(8.0_f32);

                        let uri = format!("bytes://mermaid_{:x}.svg", code_hash);
                        let img = egui::Image::from_bytes(uri, svg_str.into_bytes())
                            .rounding(Rounding::same(4.0_f32))
                            .fit_to_original_size(self.font_scale);

                        ui.centered_and_justified(|ui| {
                            ui.add(img);
                        });
                    });
                ui.add_space(4.0_f32);
                return;
            }
        }

        let (hl_bg, hl_fg, act_bg, act_fg) = self.hl_colors();

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
                        let copy_id = ui.make_persistent_id(format!("md_cb_copy_{:p}_{}", code.as_ptr(), code.len()));
                        let is_copied = ui.ctx().data(|d| {
                            d.get_temp::<std::time::Instant>(copy_id)
                                .map(|t| t.elapsed().as_secs_f32() < 2.0_f32)
                                .unwrap_or(false)
                        });

                        let btn_text = if is_copied {
                            RichText::new("✓ 已複製")
                                .color(Color32::from_rgb(34, 197, 94))
                                .size(11.5 * self.font_scale)
                                .strong()
                        } else {
                            RichText::new("📋 複製")
                                .color(self.theme.text_secondary())
                                .size(11.5 * self.font_scale)
                        };

                        if ui.button(btn_text).clicked() {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(code.to_string());
                            }
                            ui.ctx().data_mut(|d| d.insert_temp(copy_id, std::time::Instant::now()));
                        }
                    });
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 語法高亮 (快取 LayoutJob 避免每幀重複執行 syntect 正則高亮)
                let cache_id = ui.make_persistent_id(format!(
                    "md_cb_hl_{:p}_{}_{}_{}_{:?}_{:?}",
                    code.as_ptr(),
                    code.len(),
                    (self.font_scale * 100.0) as u32,
                    self.search_query,
                    self.active_match_index,
                    self.theme
                ));

                let layout_job = ui.ctx().data_mut(|d| {
                    if let Some(cached) = d.get_temp::<LayoutJob>(cache_id) {
                        cached.clone()
                    } else {
                        let syntax_set = get_syntax_set();
                        let theme_set = get_theme_set();

                        let syntect_theme = match self.theme {
                            AppTheme::Dark => &theme_set.themes["base16-eighties.dark"],
                            AppTheme::Light => &theme_set.themes["InspiredGitHub"],
                        };

                        let lang_lower = lang.to_lowercase();
                        let syntax = find_syntax_by_lang(&lang_lower, syntax_set);
                        let mut highlighter = HighlightLines::new(syntax, syntect_theme);
                        let font_id = FontId::monospace(13.0 * self.font_scale);
                        let mut job = LayoutJob::default();

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
                                append_highlighted_text(
                                    &mut job,
                                    text,
                                    self.search_query,
                                    base_fmt,
                                    hl_bg,
                                    hl_fg,
                                    act_bg,
                                    act_fg,
                                    self.active_match_index,
                                    &mut self.match_counter,
                                );
                            }
                        }

                        d.insert_temp(cache_id, job.clone());
                        job
                    }
                });

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

        let border_color = self.theme.border_color();
        let header_bg = self.theme.card_bg_color();
        let even_row_bg = self.theme.bg_color();
        let odd_row_bg = match self.theme {
            AppTheme::Dark => Color32::from_rgba_unmultiplied(255, 255, 255, 6),
            AppTheme::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 8),
        };

        let (hl_bg, hl_fg, act_bg, act_fg) = self.hl_colors();

        ui.add_space(4.0_f32);
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                Frame::none()
                    .fill(self.theme.card_bg_color())
                    .rounding(Rounding::same(6.0_f32))
                    .stroke(Stroke::new(1.0_f32, border_color))
                    .inner_margin(Margin::same(6.0_f32))
                    .show(ui, |ui| {
                        egui::Grid::new(ui.next_auto_id())
                            .striped(false)
                            .min_col_width(70.0_f32 * self.font_scale)
                            .spacing(Vec2::new(12.0_f32 * self.font_scale, 6.0_f32 * self.font_scale))
                            .show(ui, |ui| {
                                // Header
                                if !self.table_headers.is_empty() {
                                    for header in &self.table_headers {
                                        Frame::none()
                                            .fill(header_bg)
                                            .stroke(Stroke::new(1.0_f32, border_color))
                                            .rounding(Rounding::same(4.0_f32))
                                            .inner_margin(Margin::symmetric(10.0_f32 * self.font_scale, 6.0_f32 * self.font_scale))
                                            .show(ui, |ui| {
                                                let mut job = LayoutJob::default();
                                                let base_fmt = egui::TextFormat {
                                                    font_id: FontId::proportional(13.5_f32 * self.font_scale),
                                                    color: self.theme.accent_color(),
                                                    valign: egui::Align::Center,
                                                    ..Default::default()
                                                };
                                                let mut counter = 0;
                                                append_highlighted_text(
                                                    &mut job,
                                                    header,
                                                    self.search_query,
                                                    base_fmt,
                                                    hl_bg,
                                                    hl_fg,
                                                    act_bg,
                                                    act_fg,
                                                    self.active_match_index,
                                                    &mut counter,
                                                );
                                                ui.label(job);
                                            });
                                    }
                                    ui.end_row();
                                }

                                // Rows
                                for (row_idx, row) in self.table_rows.iter().enumerate() {
                                    let row_bg = if row_idx % 2 == 0 { even_row_bg } else { odd_row_bg };

                                    for col_idx in 0..num_cols {
                                        let cell = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                                        Frame::none()
                                            .fill(row_bg)
                                            .stroke(Stroke::new(0.5_f32, border_color))
                                            .rounding(Rounding::same(4.0_f32))
                                            .inner_margin(Margin::symmetric(10.0_f32 * self.font_scale, 6.0_f32 * self.font_scale))
                                            .show(ui, |ui| {
                                                let mut job = LayoutJob::default();
                                                let base_fmt = egui::TextFormat {
                                                    font_id: FontId::proportional(13.0_f32 * self.font_scale),
                                                    color: self.theme.text_primary(),
                                                    line_height: Some(19.0_f32 * self.font_scale),
                                                    valign: egui::Align::Center,
                                                    ..Default::default()
                                                };
                                                let mut counter = 0;
                                                append_highlighted_text(
                                                    &mut job,
                                                    cell,
                                                    self.search_query,
                                                    base_fmt,
                                                    hl_bg,
                                                    hl_fg,
                                                    act_bg,
                                                    act_fg,
                                                    self.active_match_index,
                                                    &mut counter,
                                                );
                                                ui.label(job);
                                            });
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
        ui.add_space(6.0_f32);
    }
}

/// 依語言副檔名或標記尋找最佳 Syntect 語法定義 (包含多層備援機制)
pub fn find_syntax_by_lang<'a>(lang_lower: &str, syntax_set: &'a SyntaxSet) -> &'a syntect::parsing::SyntaxReference {
    syntax_set
        .find_syntax_by_token(lang_lower)
        .or_else(|| syntax_set.find_syntax_by_extension(lang_lower))
        .or_else(|| {
            match lang_lower {
                "rs" | "rust" => syntax_set.find_syntax_by_name("Rust"),
                "py" | "python" => syntax_set.find_syntax_by_name("Python"),
                "js" | "mjs" | "cjs" | "javascript" => syntax_set.find_syntax_by_name("JavaScript"),
                "jsx" => syntax_set.find_syntax_by_name("JavaScript (JSX)").or_else(|| syntax_set.find_syntax_by_name("JavaScript")),
                "ts" | "typescript" => syntax_set.find_syntax_by_name("TypeScript").or_else(|| syntax_set.find_syntax_by_name("JavaScript")),
                "tsx" => syntax_set.find_syntax_by_name("TypeScript (TSX)").or_else(|| syntax_set.find_syntax_by_name("JavaScript (JSX)")).or_else(|| syntax_set.find_syntax_by_name("JavaScript")),
                "toml" => syntax_set.find_syntax_by_name("TOML").or_else(|| syntax_set.find_syntax_by_name("YAML")),
                "ini" | "conf" | "cfg" | "env" => syntax_set.find_syntax_by_name("INI").or_else(|| syntax_set.find_syntax_by_name("YAML")),
                "yaml" | "yml" => syntax_set.find_syntax_by_name("YAML"),
                "json" | "json5" | "jsonc" => syntax_set.find_syntax_by_name("JSON"),
                "c" | "h" => syntax_set.find_syntax_by_name("C"),
                "cpp" | "cc" | "cxx" | "hpp" => syntax_set.find_syntax_by_name("C++"),
                "cs" | "csharp" => syntax_set.find_syntax_by_name("C#"),
                "go" | "golang" => syntax_set.find_syntax_by_name("Go"),
                "java" => syntax_set.find_syntax_by_name("Java"),
                "kt" | "kts" | "kotlin" => syntax_set.find_syntax_by_name("Kotlin").or_else(|| syntax_set.find_syntax_by_name("Java")),
                "html" | "htm" | "xhtml" => syntax_set.find_syntax_by_name("HTML"),
                "css" => syntax_set.find_syntax_by_name("CSS"),
                "scss" | "sass" | "less" => syntax_set.find_syntax_by_name("Sass").or_else(|| syntax_set.find_syntax_by_name("CSS")),
                "sql" => syntax_set.find_syntax_by_name("SQL"),
                "sh" | "bash" | "zsh" | "fish" | "shell" => {
                    syntax_set.find_syntax_by_name("Bourne Again Shell (bash)")
                        .or_else(|| syntax_set.find_syntax_by_name("Shell-Unix-Generic"))
                }
                "ps1" | "psm1" | "psd1" | "powershell" | "pwsh" | "ps" => {
                    syntax_set.find_syntax_by_name("PowerShell")
                        .or_else(|| syntax_set.find_syntax_by_name("Bourne Again Shell (bash)"))
                        .or_else(|| syntax_set.find_syntax_by_name("Shell-Unix-Generic"))
                }
                "bat" | "cmd" | "batch" => {
                    syntax_set.find_syntax_by_name("Batch File")
                        .or_else(|| syntax_set.find_syntax_by_name("Batch File (DOS)"))
                        .or_else(|| syntax_set.find_syntax_by_name("Bourne Again Shell (bash)"))
                }
                "dockerfile" | "containerfile" => {
                    syntax_set.find_syntax_by_name("Dockerfile")
                        .or_else(|| syntax_set.find_syntax_by_name("Bourne Again Shell (bash)"))
                }
                "xml" | "svg" => syntax_set.find_syntax_by_name("XML"),
                "lua" => syntax_set.find_syntax_by_name("Lua"),
                "php" => syntax_set.find_syntax_by_name("PHP"),
                "rb" | "ruby" => syntax_set.find_syntax_by_name("Ruby"),
                "graphql" | "gql" => syntax_set.find_syntax_by_name("JSON"),
                "vue" | "svelte" => syntax_set.find_syntax_by_name("HTML"),
                _ => None,
            }
        })
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

/// 支援全語法高亮 + 行號 + 搜尋高亮的獨立程式碼檢視器 (全量 LayoutJob 快取，秒開 100K 行超大檔案)
pub fn render_code_viewer(
    ui: &mut Ui,
    theme: AppTheme,
    font_scale: f32,
    code: &str,
    extension_or_lang: &str,
    search_query: &str,
    active_match_index: Option<usize>,
) {
    let syntax_set = get_syntax_set();
    let theme_set = get_theme_set();

    let syntect_theme = match theme {
        AppTheme::Dark => &theme_set.themes["base16-eighties.dark"],
        AppTheme::Light => &theme_set.themes["InspiredGitHub"],
    };

    let lang_lower = extension_or_lang.to_lowercase();
    let syntax = find_syntax_by_lang(&lang_lower, syntax_set);

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

    // 快取整個檔案的高亮 LayoutJob，避免每幀在 60 FPS 下反覆進行 syntect 正則運算
    let cache_id = ui.make_persistent_id(format!(
        "code_viewer_full_{:p}_{}_{}_{}_{:?}_{:?}",
        code.as_ptr(),
        code.len(),
        (font_scale * 100.0) as u32,
        search_query,
        active_match_index,
        theme
    ));

    let (gutter_job, code_job, line_count, is_large_file) = ui.ctx().data_mut(|d| {
        if let Some(cached) = d.get_temp::<(LayoutJob, LayoutJob, usize, bool)>(cache_id) {
            cached.clone()
        } else {
            let mut highlighter = HighlightLines::new(syntax, syntect_theme);
            let mut gutter_job = LayoutJob::default();
            let mut code_job = LayoutJob::default();
            let mut line_count = 0;
            let mut match_counter = 0;
            const MAX_HIGHLIGHT_LINES: usize = 3500;
            const MAX_LINE_CHAR_LIMIT: usize = 2500;
            let mut is_large = false;

            let default_text_color = match theme {
                AppTheme::Dark => Color32::from_rgb(226, 232, 240),
                AppTheme::Light => Color32::from_rgb(30, 41, 59),
            };

            for line in syntect::util::LinesWithEndings::from(code) {
                line_count += 1;

                if line_count <= MAX_HIGHLIGHT_LINES {
                    // 超長單行截斷防護 (例如未排版的單行 minified JSON)，避免正則回溯卡頓
                    let (highlight_chunk, remaining_chunk) = if line.len() > MAX_LINE_CHAR_LIMIT {
                        is_large = true;
                        let boundary = line
                            .char_indices()
                            .nth(MAX_LINE_CHAR_LIMIT)
                            .map(|(idx, _)| idx)
                            .unwrap_or(line.len());
                        (&line[..boundary], Some(&line[boundary..]))
                    } else {
                        (line, None)
                    };

                    let ranges = highlighter
                        .highlight_line(highlight_chunk, syntax_set)
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

                        append_highlighted_text(
                            &mut code_job,
                            text,
                            search_query,
                            base_fmt,
                            hl_bg,
                            hl_fg,
                            act_bg,
                            act_fg,
                            active_match_index,
                            &mut match_counter,
                        );
                    }

                    if let Some(rest) = remaining_chunk {
                        let base_fmt = egui::TextFormat {
                            font_id: font_id.clone(),
                            color: default_text_color,
                            line_height: Some(21.0 * font_scale),
                            ..Default::default()
                        };
                        append_highlighted_text(
                            &mut code_job,
                            rest,
                            search_query,
                            base_fmt,
                            hl_bg,
                            hl_fg,
                            act_bg,
                            act_fg,
                            active_match_index,
                            &mut match_counter,
                        );
                    }
                } else {
                    is_large = true;
                    // 超過 3,500 行的超大型檔案，採用極速純文字格式化，免除 syntect 正則狀態機消耗
                    let base_fmt = egui::TextFormat {
                        font_id: font_id.clone(),
                        color: default_text_color,
                        line_height: Some(21.0 * font_scale),
                        ..Default::default()
                    };
                    append_highlighted_text(
                        &mut code_job,
                        line,
                        search_query,
                        base_fmt,
                        hl_bg,
                        hl_fg,
                        act_bg,
                        act_fg,
                        active_match_index,
                        &mut match_counter,
                    );
                }
            }

            let gutter_digits = format!("{}", line_count.max(1)).len().max(2);
            for i in 0..line_count {
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

            let result = (gutter_job, code_job, line_count, is_large);
            d.insert_temp(cache_id, result.clone());
            result
        }
    });

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
                ui.label(
                    RichText::new(format!("•  {} 行", line_count))
                        .size(11.0 * font_scale)
                        .color(theme.text_secondary()),
                );
                if is_large_file {
                    Frame::none()
                        .fill(theme.code_bg_color())
                        .rounding(Rounding::same(3.0))
                        .inner_margin(Margin::symmetric(5.0, 1.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("⚡ 極速模式 (3,500+ 行加速)")
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
                        RichText::new("✓ 已複製")
                            .color(Color32::from_rgb(34, 197, 94))
                            .size(11.5 * font_scale)
                            .strong()
                    } else {
                        RichText::new("📋 複製程式碼")
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
                let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, (line_count as f32) * 21.0 * font_scale), egui::Sense::hover());
                ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0_f32, border_color));
                ui.add_space(8.0);

                // 2. 程式碼語法高亮區域 (使用快取的 LayoutJob，瞬時渲染)
                ui.vertical(|ui| {
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
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "json" | "json5" | "jsonc" | "toml" | "yaml" | "yml"
            | "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "cs" | "go" | "java" | "kt" | "kts"
            | "html" | "htm" | "xhtml" | "css" | "scss" | "sass" | "sql" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "psm1" | "psd1" | "powershell" | "pwsh" | "ps" | "bat"
            | "cmd" | "xml" | "lua" | "php" | "rb" | "swift" | "dart" | "vue" | "svelte"
            | "csv" | "tsv" | "ini" | "conf" | "env" | "dockerfile" | "graphql" | "gql"
            | "diff" | "patch" | "log" | "r" | "scala" | "zig" | "proto"
    )
}

/// 取得語言的美觀顯示名稱與 Emoji 徽章
pub fn get_language_badge(ext: &str) -> (String, &'static str) {
    match ext.to_lowercase().as_str() {
        "rs" => ("Rust".to_string(), "🦀"),
        "py" => ("Python".to_string(), "🐍"),
        "js" | "mjs" | "cjs" => ("JavaScript".to_string(), "⚡"),
        "jsx" => ("React JSX".to_string(), "⚛️"),
        "ts" => ("TypeScript".to_string(), "🔷"),
        "tsx" => ("React TSX".to_string(), "⚛️"),
        "json" | "json5" | "jsonc" => ("JSON".to_string(), "📦"),
        "toml" => ("TOML".to_string(), "⚙️"),
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
        "sql" => ("SQL".to_string(), "🗄️"),
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
        "ini" | "conf" | "env" => ("Config".to_string(), "⚙️"),
        "diff" | "patch" => ("Diff".to_string(), "🔄"),
        "log" => ("Log 記錄".to_string(), "📋"),
        "zig" => ("Zig".to_string(), "⚡"),
        "r" => ("R 語言".to_string(), "📈"),
        "scala" => ("Scala".to_string(), "🔴"),
        "proto" => ("Protobuf".to_string(), "📦"),
        _ => (ext.to_uppercase(), "💻"),
    }
}

/// 單元章節大綱項目
#[derive(Debug, Clone)]
pub struct TocItem {
    pub level: u8,
    pub title: String,
    pub line_idx: usize,
}

/// 解析 Markdown 內容提取 H1~H6 章節標題
pub fn extract_markdown_toc(content: &str) -> Vec<TocItem> {
    let mut toc = Vec::new();
    let mut in_code_block = false;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 6 {
                let rest = trimmed[hash_count..].trim();
                if !rest.is_empty() {
                    let clean_title = rest
                        .replace("**", "")
                        .replace('*', "")
                        .replace('`', "")
                        .replace("~~", "");
                    toc.push(TocItem {
                        level: hash_count as u8,
                        title: clean_title,
                        line_idx,
                    });
                }
            }
        }
    }
    toc
}

/// CSV / TSV 資料表格結構體
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CsvTableData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub total_cols: usize,
}

/// 解析 CSV 或 TSV 檔案內容 (支援雙引號轉義與逗號/Tab 欄位分隔)
pub fn parse_csv_or_tsv(content: &str, separator: char) -> CsvTableData {
    let mut all_rows = Vec::new();

    for line in content.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        let mut row = Vec::new();
        let mut current_field = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '"' {
                if in_quotes && chars.peek() == Some(&'"') {
                    current_field.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            } else if ch == separator && !in_quotes {
                row.push(current_field.trim().to_string());
                current_field.clear();
            } else {
                current_field.push(ch);
            }
        }
        row.push(current_field.trim().to_string());
        all_rows.push(row);
    }

    if all_rows.is_empty() {
        return CsvTableData {
            headers: Vec::new(),
            rows: Vec::new(),
            total_rows: 0,
            total_cols: 0,
        };
    }

    let headers = all_rows.remove(0);
    let total_cols = headers.len().max(all_rows.iter().map(|r| r.len()).max().unwrap_or(0));
    let total_rows = all_rows.len();

    CsvTableData {
        headers,
        rows: all_rows,
        total_rows,
        total_cols,
    }
}

/// 渲染現代斑馬紋資料表格
pub fn render_csv_table(
    ui: &mut Ui,
    theme: AppTheme,
    font_scale: f32,
    table: &CsvTableData,
    search_query: &str,
    active_match_index: Option<usize>,
    match_counter: &mut usize,
) {
    if table.headers.is_empty() && table.rows.is_empty() {
        ui.label("表格內容為空");
        return;
    }

    let header_bg = theme.card_bg_color();
    let border_color = theme.border_color();
    let even_row_bg = theme.bg_color();
    let odd_row_bg = match theme {
        AppTheme::Dark => Color32::from_rgba_unmultiplied(255, 255, 255, 6),
        AppTheme::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 8),
    };

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

    egui::Grid::new("csv_grid_table")
        .striped(false)
        .spacing(Vec2::new(14.0 * font_scale, 8.0 * font_scale))
        .show(ui, |ui| {
            // 表頭行 (Header)
            for header in &table.headers {
                Frame::none()
                    .fill(header_bg)
                    .stroke(Stroke::new(1.0_f32, border_color))
                    .inner_margin(Margin::symmetric(10.0 * font_scale, 6.0 * font_scale))
                    .show(ui, |ui| {
                        let mut job = LayoutJob::default();
                        let base_fmt = egui::TextFormat {
                            font_id: FontId::proportional(13.0 * font_scale),
                            color: theme.accent_color(),
                            ..Default::default()
                        };
                        append_highlighted_text(
                            &mut job,
                            header,
                            search_query,
                            base_fmt,
                            hl_bg,
                            hl_fg,
                            act_bg,
                            act_fg,
                            active_match_index,
                            match_counter,
                        );
                        ui.label(job);
                    });
            }
            ui.end_row();

            // 資料行 (Data Rows with Zebra Striping)
            for (row_idx, row) in table.rows.iter().enumerate() {
                let row_bg = if row_idx % 2 == 0 { even_row_bg } else { odd_row_bg };

                for col_idx in 0..table.total_cols {
                    let cell_text = row.get(col_idx).map(|s| s.as_str()).unwrap_or("");

                    Frame::none()
                        .fill(row_bg)
                        .stroke(Stroke::new(0.5_f32, border_color))
                        .inner_margin(Margin::symmetric(10.0 * font_scale, 5.0 * font_scale))
                        .show(ui, |ui| {
                            let mut job = LayoutJob::default();
                            let base_fmt = egui::TextFormat {
                                font_id: FontId::proportional(12.5 * font_scale),
                                color: theme.text_primary(),
                                line_height: Some(18.0 * font_scale),
                                ..Default::default()
                            };
                            append_highlighted_text(
                                &mut job,
                                cell_text,
                                search_query,
                                base_fmt,
                                hl_bg,
                                hl_fg,
                                act_bg,
                                act_fg,
                                active_match_index,
                                match_counter,
                            );
                            ui.label(job);
                        });
                }
                ui.end_row();
            }
        });
}

/// JSON 零依賴極速排版美化 (Pretty Print with 2 Spaces，串流零多餘拷貝)
pub fn format_json(input: &str) -> Result<String, String> {
    let mut result = String::with_capacity(input.len() + input.len() / 2);
    let mut indent_level: usize = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            result.push(ch);
            if escape_next {
                escape_next = false;
            } else if ch == '\\' {
                escape_next = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                result.push('"');
            }
            '{' | '[' => {
                result.push(ch);
                // 檢查是否緊接著閉合括號
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Some(&next_ch) = chars.peek() {
                    if (ch == '{' && next_ch == '}') || (ch == '[' && next_ch == ']') {
                        // 空物件/陣列保持單行 {} 或 []
                        continue;
                    }
                }
                indent_level += 1;
                result.push('\n');
                append_indent_spaces(&mut result, indent_level);
            }
            '}' | ']' => {
                indent_level = indent_level.saturating_sub(1);
                if !result.ends_with('\n') && !result.ends_with('{') && !result.ends_with('[') {
                    result.push('\n');
                    append_indent_spaces(&mut result, indent_level);
                }
                result.push(ch);
            }
            ',' => {
                result.push(',');
                result.push('\n');
                append_indent_spaces(&mut result, indent_level);
            }
            ':' => {
                result.push(':');
                result.push(' ');
            }
            c if c.is_whitespace() => {
                // 忽略字串外部空白
            }
            c => {
                result.push(c);
            }
        }
    }
    Ok(result)
}

#[inline(always)]
fn append_indent_spaces(buf: &mut String, level: usize) {
    for _ in 0..level {
        buf.push_str("  ");
    }
}

/// JSON 零依賴壓縮為單行 (Minify，串流高效版)
pub fn minify_json(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escape_next = false;
    for ch in input.chars() {
        if in_string {
            result.push(ch);
            if escape_next {
                escape_next = false;
            } else if ch == '\\' {
                escape_next = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else {
            match ch {
                '"' => {
                    in_string = true;
                    result.push(ch);
                }
                c if c.is_whitespace() => {}
                c => result.push(c),
            }
        }
    }
    result
}

/// 判斷特定副檔名是否為 PDF 文件
pub fn is_pdf_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("pdf")
}

/// 自 PDF 二進制資料中即時擷取純文字與分頁結構，轉換為 Markdown 格式
pub fn extract_text_from_pdf_bytes(bytes: &[u8]) -> Result<(String, usize), String> {
    let doc = lopdf::Document::load_mem(bytes).map_err(|e| format!("PDF 解析失敗: {}", e))?;
    let page_numbers: Vec<u32> = doc.get_pages().keys().cloned().collect();
    let mut sorted_pages = page_numbers;
    sorted_pages.sort();

    let total_pages = sorted_pages.len();
    if total_pages == 0 {
        return Ok(("（此 PDF 文件為空或無頁面）".to_string(), 0));
    }

    let mut pages_text = Vec::new();
    for &page_num in &sorted_pages {
        let text = doc.extract_text(&[page_num]).unwrap_or_default();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            pages_text.push(format!("### 📄 第 {} / {} 頁\n\n{}\n", page_num, total_pages, trimmed));
        }
    }

    if pages_text.is_empty() {
        Ok((
            format!("### 📄 PDF 快速預覽 (共 {} 頁)\n\n> ⚠️ 此 PDF 文件的頁面可能為純掃描圖檔或加密內容，未包含可提取的內嵌文字字串。", total_pages),
            total_pages,
        ))
    } else {
        Ok((pages_text.join("\n---\n\n"), total_pages))
    }
}

/// Markdown / 文本統計數據
#[derive(Debug, Clone, Copy, Default)]
pub struct TextStats {
    pub cjk_chars: usize,
    pub words: usize,
    pub total_chars: usize,
    pub lines: usize,
    pub reading_time_mins: usize,
}

/// 快速計算中英文統計字數與預估閱讀時間
pub fn calculate_text_stats(text: &str) -> TextStats {
    let mut cjk_chars = 0;
    let mut words = 0;
    let mut total_chars = 0;
    let mut in_word = false;

    for ch in text.chars() {
        if !ch.is_whitespace() {
            total_chars += 1;
        }

        // CJK 統一表意文字、注音、假名、諺文與常用 CJK 標點
        let is_cjk = matches!(ch as u32,
            0x4E00..=0x9FFF | // CJK 統一表意符號
            0x3400..=0x4DBF | // CJK 擴展 A
            0x20000..=0x2A6DF | // CJK 擴展 B
            0x3040..=0x309F | // 日文平假名
            0x30A0..=0x30FF | // 日文片假名
            0xAC00..=0xD7AF | // 韓文音節
            0x3100..=0x312F | // 注音符號
            0x3000..=0x303F   // CJK 符號與標點
        );

        if is_cjk {
            cjk_chars += 1;
            if in_word {
                words += 1;
                in_word = false;
            }
        } else if ch.is_alphanumeric() {
            in_word = true;
        } else if in_word {
            words += 1;
            in_word = false;
        }
    }

    if in_word {
        words += 1;
    }

    let lines = text.lines().count();

    // 閱讀時間計算：中文字約每分鐘 350 字，英文字約每分鐘 220 字
    let total_reading_units = (cjk_chars as f32) + (words as f32) * 1.5;
    let reading_time_mins = (total_reading_units / 350.0).ceil() as usize;

    TextStats {
        cjk_chars,
        words,
        total_chars,
        lines,
        reading_time_mins: reading_time_mins.max(1),
    }
}



