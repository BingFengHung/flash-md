mod code;
mod mermaid;
#[cfg(test)]
mod navigation_tests;
mod pdf;
mod table;

use code::find_syntax_by_lang;
pub use code::{
    get_image_badge, get_language_badge, is_code_extension, is_image_extension, render_code_viewer,
};
use mermaid::{extract_attr_str, get_or_render_mermaid_diagram};
pub use pdf::{extract_text_from_pdf_bytes, is_pdf_extension};

pub use crate::parsers::{format_json, minify_json};
use crate::theme::AppTheme;
use egui::{
    text::LayoutJob, Align, Align2, Color32, FontId, Frame, Layout, Margin, RichText, Rounding,
    Sense, Stroke, Ui, Vec2,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
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
#[allow(clippy::too_many_arguments)]
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
    let mut plain = LayoutJob::default();
    plain.append(text, 0.0, base_format);
    crate::search::highlight_job(
        &mut plain,
        search_query,
        active_match_idx,
        match_counter,
        normal_hl_bg,
        normal_hl_fg,
        active_hl_bg,
        active_hl_fg,
    );
    let offset = job.text.len();
    job.text.push_str(&plain.text);
    job.sections
        .extend(plain.sections.into_iter().map(|mut section| {
            section.byte_range.start += offset;
            section.byte_range.end += offset;
            section
        }));
}

/// 精確比對 URL 解碼後的錨點，避免子字串跳到錯誤或重複標題。
pub fn is_anchor_match(heading_slug: &str, anchor: &str) -> bool {
    heading_slug
        == crate::explorer::url_decode(anchor)
            .trim()
            .trim_start_matches('#')
}

pub struct RenderOutput {
    pub clicked_anchor: Option<String>,
    pub match_count: usize,
    pub anchor_found: bool,
}

pub struct MarkdownRenderer<'a> {
    pub theme: AppTheme,
    pub font_scale: f32,
    pub search_query: &'a str,
    pub active_match_index: Option<usize>,
    pub search_jump: bool,
    pub target_anchor: Option<&'a str>,
    pub base_dir: Option<&'a std::path::Path>,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> MarkdownRenderer<'a> {
    pub fn new(
        theme: AppTheme,
        font_scale: f32,
        search_query: &'a str,
        active_match_index: Option<usize>,
        target_anchor: Option<&'a str>,
        base_dir: Option<&'a std::path::Path>,
    ) -> Self {
        Self {
            theme,
            font_scale,
            search_query,
            active_match_index,
            search_jump: false,
            target_anchor,
            base_dir,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn render(&self, ui: &mut Ui, markdown_text: &str) -> RenderOutput {
        let events = crate::parsers::cached_events(ui.ctx(), markdown_text);
        let mut context = RenderContext::new(
            self.theme,
            self.font_scale,
            self.search_query,
            self.active_match_index,
            self.target_anchor,
            self.base_dir,
        );
        context.search_jump = self.search_jump;
        for event in events.iter().cloned() {
            context.process_event(ui, event);
        }
        context.flush_inline(ui);
        // Nested horizontal table areas consume both scroll axes. Submit the
        // heading target only after every nested area has finished rendering.
        if let Some(rect) = context.anchor_rect {
            ui.scroll_to_rect(rect, Some(egui::Align::TOP));
        } else if let Some(rect) = context.search_rect {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }
        RenderOutput {
            clicked_anchor: context.clicked_anchor,
            match_count: context.match_counter,
            anchor_found: context.anchor_rect.is_some(),
        }
    }
}

struct InlineSpan {
    text: String,
    #[allow(dead_code)]
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
    base_dir: Option<&'a std::path::Path>,
    clicked_anchor: Option<String>,
    anchor_rect: Option<egui::Rect>,
    search_rect: Option<egui::Rect>,
    match_counter: usize,
    search_jump: bool,
    heading_counts: std::collections::HashMap<String, usize>,
    heading_id: Option<String>,
    inlines: Vec<InlineSpan>,
    current_bold: bool,
    current_italic: bool,
    current_strikethrough: bool,
    current_link: Option<String>,
    current_image_url: Option<String>,
    current_image_alt: String,
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

fn decode_uri_component(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let src = s.as_bytes();
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'%' && i + 2 < src.len() {
            if let Ok(hex_str) = std::str::from_utf8(&src[i + 1..i + 3]) {
                if let Ok(byte_val) = u8::from_str_radix(hex_str, 16) {
                    bytes.push(byte_val);
                    i += 3;
                    continue;
                }
            }
        }
        bytes.push(src[i]);
        i += 1;
    }
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).to_string())
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let clean = input.trim();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in clean.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

fn detect_image_format_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        Some("png")
    } else if bytes.len() >= 3 && &bytes[0..3] == b"\xFF\xD8\xFF" {
        Some("jpeg")
    } else if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() >= 2 && &bytes[0..2] == b"BM" {
        Some("bmp")
    } else if bytes.len() >= 4 && (&bytes[0..4] == b"MM\x00*" || &bytes[0..4] == b"II*\x00") {
        Some("tiff")
    } else if bytes.len() >= 4 && &bytes[0..4] == b"\x00\x00\x01\x00" {
        Some("ico")
    } else if bytes.iter().take(300).any(|&b| b == b'<') {
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(500)]).to_lowercase();
        if text.contains("<svg") {
            Some("svg")
        } else {
            None
        }
    } else {
        None
    }
}

fn find_local_image_bytes(
    base_dir: Option<&std::path::Path>,
    clean_url: &str,
) -> Option<(std::path::PathBuf, Vec<u8>, &'static str)> {
    let url_without_query = clean_url
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_matches('"')
        .trim_matches('\'')
        .split('?')
        .next()
        .unwrap_or(clean_url)
        .split('#')
        .next()
        .unwrap_or(clean_url);

    let decoded = decode_uri_component(url_without_query);
    let raw_path = decoded
        .trim_start_matches("file:///")
        .trim_start_matches("file://")
        .trim_start_matches("file:")
        .replace('/', "\\");

    let clean_relative = raw_path
        .trim_start_matches(".\\")
        .trim_start_matches("./")
        .trim_start_matches('\\')
        .trim_start_matches('/');

    let direct_path = std::path::PathBuf::from(&raw_path);
    if direct_path.is_absolute() && direct_path.is_file() {
        if let Ok(bytes) = std::fs::read(&direct_path) {
            let fmt = detect_image_format_from_bytes(&bytes).unwrap_or("png");
            return Some((direct_path.clone(), bytes, fmt));
        }
    }

    if let Some(base) = base_dir {
        // 1. 直接候選路徑清單
        let candidates = [
            base.join(clean_relative),
            base.join(&raw_path),
            base.join(&decoded),
            base.join(url_without_query),
            base.join("assets").join(clean_relative),
            base.join(".assets").join(clean_relative),
            base.join("images").join(clean_relative),
            base.join("img").join(clean_relative),
            base.join("attachments").join(clean_relative),
            base.join(".attachments").join(clean_relative),
            base.join("media").join(clean_relative),
            base.join("resources").join(clean_relative),
            base.join("static").join(clean_relative),
            base.join("public").join(clean_relative),
        ];

        for cand in &candidates {
            if cand.is_file() {
                if let Ok(bytes) = std::fs::read(cand) {
                    let fmt = detect_image_format_from_bytes(&bytes).unwrap_or("png");
                    return Some((cand.to_path_buf(), bytes, fmt));
                }
            }
        }

        // 2. 若無副檔名或含有副檔名，嘗試附加常見圖片副檔名
        let extensions = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"];
        for ext in &extensions {
            let with_ext = format!("{}.{}", clean_relative, ext);
            let candidates_ext = [
                base.join(&with_ext),
                base.join("assets").join(&with_ext),
                base.join(".assets").join(&with_ext),
                base.join("images").join(&with_ext),
                base.join("attachments").join(&with_ext),
            ];
            for cand in &candidates_ext {
                if cand.is_file() {
                    if let Ok(bytes) = std::fs::read(cand) {
                        let fmt = detect_image_format_from_bytes(&bytes).unwrap_or(ext);
                        return Some((cand.to_path_buf(), bytes, fmt));
                    }
                }
            }
        }

        // 3. 遍歷 base 目錄及其直接子目錄，尋找檔名相符的檔案 (包含 Typora/Obsidian 的 *.assets 資料夾)
        let target_filename = std::path::Path::new(clean_relative)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(clean_relative);

        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let sub_cand = p.join(target_filename);
                    if sub_cand.is_file() {
                        if let Ok(bytes) = std::fs::read(&sub_cand) {
                            let fmt = detect_image_format_from_bytes(&bytes).unwrap_or("png");
                            return Some((sub_cand.clone(), bytes, fmt));
                        }
                    }
                    for ext in &extensions {
                        let sub_cand_ext = p.join(format!("{}.{}", target_filename, ext));
                        if sub_cand_ext.is_file() {
                            if let Ok(bytes) = std::fs::read(&sub_cand_ext) {
                                let fmt = detect_image_format_from_bytes(&bytes).unwrap_or(ext);
                                return Some((sub_cand_ext.clone(), bytes, fmt));
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

impl<'a> RenderContext<'a> {
    fn new(
        theme: AppTheme,
        font_scale: f32,
        search_query: &'a str,
        active_match_index: Option<usize>,
        target_anchor: Option<&'a str>,
        base_dir: Option<&'a std::path::Path>,
    ) -> Self {
        Self {
            theme,
            font_scale,
            search_query,
            active_match_index,
            target_anchor,
            base_dir,
            clicked_anchor: None,
            anchor_rect: None,
            search_rect: None,
            match_counter: 0,
            search_jump: false,
            heading_counts: Default::default(),
            heading_id: None,
            inlines: Vec::new(),
            current_bold: false,
            current_italic: false,
            current_strikethrough: false,
            current_link: None,
            current_image_url: None,
            current_image_alt: String::new(),
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
                Color32::from_rgb(254, 240, 138), // 普通相符：柔和檸檬黃底
                Color32::from_rgb(113, 63, 18),   // 普通相符：深褐色字
                Color32::from_rgb(234, 88, 12),   // 當前 Focus 相符：深橘紅底
                Color32::WHITE,                   // 當前 Focus 相符：純白字
            ),
        }
    }

    fn highlight_job(&mut self, job: &mut LayoutJob) -> Option<usize> {
        let base = self.match_counter;
        let (bg, fg, active_bg, active_fg) = self.hl_colors();
        crate::search::highlight_job(
            job,
            self.search_query,
            self.active_match_index,
            &mut self.match_counter,
            bg,
            fg,
            active_bg,
            active_fg,
        );
        self.active_match_index.and_then(|index| {
            (base..self.match_counter)
                .contains(&index)
                .then(|| index - base)
        })
    }

    fn label_job(&mut self, ui: &mut Ui, mut job: LayoutJob, sense: Sense) -> egui::Response {
        let local = self.highlight_job(&mut job);
        let (response, target) = crate::search::searchable_label_target(
            ui,
            job,
            self.search_query,
            local,
            self.search_jump,
            sense,
            true,
        );
        if target.is_some() {
            self.search_rect = target;
        }
        response
    }

    fn push_text(&mut self, text: &str) {
        if self.current_image_url.is_some() {
            self.current_image_alt.push_str(text);
        } else if self.in_code_block {
            self.code_block_content.push_str(text);
        } else {
            let clean_text = if text.contains('\u{FE0F}') || text.contains('\u{FE0E}') {
                text.chars()
                    .filter(|&c| c != '\u{FE0F}' && c != '\u{FE0E}')
                    .collect()
            } else {
                text.to_string()
            };
            self.inlines.push(InlineSpan {
                text: clean_text,
                bold: self.current_bold,
                italic: self.current_italic,
                strikethrough: self.current_strikethrough,
                code: false,
                link_url: self.current_link.clone(),
            });
        }
    }

    fn render_image(&mut self, ui: &mut Ui, dest_url: &str, alt_text: &str) {
        self.flush_inline(ui);
        ui.add_space(6.0_f32);

        let clean_url = dest_url.trim().trim_matches('\0');
        let is_web_url = clean_url.starts_with("http://") || clean_url.starts_with("https://");
        let is_data_uri = clean_url.starts_with("data:image/");

        let cached_image = if is_data_uri {
            clean_url
                .find(',')
                .and_then(|comma| decode_base64(&clean_url[comma + 1..]))
                .and_then(|bytes| {
                    let extension = detect_image_format_from_bytes(&bytes).unwrap_or("png");
                    let key = format!(
                        "bytes://data_{:x}.{}",
                        crate::parsers::content_hash(clean_url),
                        extension
                    );
                    crate::textures::cached_image(ui.ctx(), &key, &bytes, extension)
                })
        } else if !is_web_url {
            let key = format!("{:?}|{}", self.base_dir, clean_url);
            crate::textures::local_image(ui.ctx(), &key, || {
                find_local_image_bytes(self.base_dir, clean_url)
            })
        } else {
            None
        };

        let available_w = (ui.available_width() - 16.0_f32).max(100.0_f32);

        Frame::none()
            .fill(self.theme.card_bg_color())
            .rounding(Rounding::same(8.0_f32))
            .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
            .inner_margin(Margin::same(10.0_f32))
            .show(ui, |ui| {
                if let Some(image) = cached_image {
                    let img = image
                        .widget()
                        .rounding(Rounding::same(6.0_f32))
                        .max_width(available_w);
                    ui.vertical_centered(|ui| {
                        ui.add(img);
                    });
                } else if is_web_url {
                    let img = egui::Image::from_uri(clean_url.to_string())
                        .rounding(Rounding::same(6.0_f32))
                        .max_width(available_w);

                    ui.vertical_centered(|ui| {
                        ui.add(img);
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("🖼 [圖片找不到]")
                                .color(self.theme.text_secondary())
                                .italics(),
                        );
                        ui.label(
                            RichText::new(if !alt_text.is_empty() {
                                alt_text
                            } else {
                                clean_url
                            })
                            .color(self.theme.text_primary()),
                        );
                    });
                }

                if !alt_text.is_empty() {
                    ui.add_space(4.0_f32);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(alt_text)
                                .size(11.5_f32 * self.font_scale)
                                .color(self.theme.text_secondary())
                                .italics(),
                        );
                    });
                }
            });

        ui.add_space(6.0_f32);
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
                    let clean_code = if code.contains('\u{FE0F}') || code.contains('\u{FE0E}') {
                        code.chars()
                            .filter(|&c| c != '\u{FE0F}' && c != '\u{FE0E}')
                            .collect()
                    } else {
                        code.to_string()
                    };
                    self.inlines.push(InlineSpan {
                        text: clean_code,
                        bold: self.current_bold,
                        italic: self.current_italic,
                        strikethrough: self.current_strikethrough,
                        code: true,
                        link_url: self.current_link.clone(),
                    });
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let lower = html.to_lowercase();
                if self.in_table && matches!(lower.trim(), "<br>" | "<br/>" | "<br />") {
                    self.push_text("\n");
                } else if lower.contains("<img") {
                    if let Some(src) = extract_attr_str(&html, "src=") {
                        let alt = extract_attr_str(&html, "alt=").unwrap_or_default();
                        self.render_image(ui, &src, &alt);
                    }
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
            Tag::Heading { level, id, .. } => {
                self.heading_id = id.map(|id| id.to_string());
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
                self.current_image_url = Some(dest_url.to_string());
                self.current_image_alt.clear();
            }
            _ => {}
        }
    }

    fn handle_end_tag(&mut self, ui: &mut Ui, tag: TagEnd) {
        match tag {
            TagEnd::Image => {
                if let Some(dest_url) = self.current_image_url.take() {
                    let alt = std::mem::take(&mut self.current_image_alt);
                    self.render_image(ui, &dest_url, &alt);
                }
            }
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
                    self.render_inline_spans(ui, inlines, false);
                });
        } else if self.list_level > 0 {
            self.render_list_item_spans(ui, inlines);
        } else {
            self.render_inline_spans(ui, inlines, false);
        }
    }

    fn render_inline_spans(&mut self, ui: &mut Ui, spans: Vec<InlineSpan>, is_list_item: bool) {
        if spans.is_empty() {
            return;
        }

        let has_hyperlinks = spans.iter().any(|s| s.link_url.is_some());
        let has_emojis = spans.iter().any(|s| {
            crate::emoji::split_text_emojis(&s.text)
                .iter()
                .any(|seg| matches!(seg, crate::emoji::TextOrEmoji::Emoji(..)))
        });

        if !has_hyperlinks && !has_emojis {
            let mut job = LayoutJob::default();
            for (idx, span) in spans.into_iter().enumerate() {
                let color = if (is_list_item && idx == 0) || span.code {
                    self.theme.accent_color()
                } else {
                    self.theme.text_primary()
                };

                let base_fmt = egui::TextFormat {
                    font_id: FontId::proportional(14.5_f32 * self.font_scale),
                    color,
                    italics: span.italic,
                    strikethrough: Stroke::new(
                        if span.strikethrough { 1.5_f32 } else { 0.0_f32 },
                        color,
                    ),
                    line_height: Some(22.0_f32 * self.font_scale),
                    valign: egui::Align::BOTTOM,
                    background: if span.code {
                        self.theme.code_bg_color()
                    } else {
                        Color32::TRANSPARENT
                    },
                    ..Default::default()
                };

                job.append(&span.text, 0.0, base_fmt);
            }
            self.label_job(ui, job, Sense::hover());
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0_f32;

                for (idx, span) in spans.into_iter().enumerate() {
                    let is_link = span.link_url.is_some();

                    let color = if (is_list_item && idx == 0) || span.code || is_link {
                        self.theme.accent_color()
                    } else {
                        self.theme.text_primary()
                    };

                    let underline = if is_link {
                        Stroke::new(1.0_f32, self.theme.accent_color())
                    } else {
                        Stroke::NONE
                    };

                    let background = if span.code {
                        self.theme.code_bg_color()
                    } else {
                        Color32::TRANSPARENT
                    };

                    let base_fmt = egui::TextFormat {
                        font_id: FontId::proportional(14.5_f32 * self.font_scale),
                        color,
                        italics: span.italic,
                        underline,
                        strikethrough: Stroke::new(
                            if span.strikethrough { 1.5_f32 } else { 0.0_f32 },
                            color,
                        ),
                        line_height: Some(22.0_f32 * self.font_scale),
                        valign: egui::Align::BOTTOM,
                        background,
                        ..Default::default()
                    };

                    let segments = crate::emoji::split_text_emojis(&span.text);
                    for seg in segments {
                        match seg {
                            crate::emoji::TextOrEmoji::Emoji(em, svg_str) => {
                                let img_uri = format!("bytes://emoji_{}.svg", em);
                                let img_size = 17.0_f32 * self.font_scale;
                                ui.add_space(2.0_f32);
                                ui.add(
                                    egui::Image::from_bytes(img_uri, svg_str.as_bytes())
                                        .fit_to_exact_size(Vec2::splat(img_size)),
                                );
                                ui.add_space(3.0_f32);
                            }
                            crate::emoji::TextOrEmoji::Text(t) => {
                                if t.is_empty() {
                                    continue;
                                }
                                let mut span_job = LayoutJob::default();
                                span_job.append(t, 0.0, base_fmt.clone());

                                if let Some(ref url) = span.link_url {
                                    let resp = self.label_job(ui, span_job, Sense::click());
                                    if resp.hovered() {
                                        ui.output_mut(|o| {
                                            o.cursor_icon = egui::CursorIcon::PointingHand
                                        });
                                    }
                                    if resp.clicked() {
                                        if url.starts_with('#') {
                                            self.clicked_anchor =
                                                Some(url.trim_start_matches('#').to_string());
                                        } else {
                                            let _ = open::that(url);
                                        }
                                    }
                                    resp.on_hover_text(url);
                                } else {
                                    self.label_job(ui, span_job, Sense::hover());
                                }
                            }
                        }
                    }
                }
            });
        }
    }

    fn render_heading(&mut self, ui: &mut Ui, level: HeadingLevel) {
        let heading_text: String = self.inlines.drain(..).map(|s| s.text).collect();
        let clean_heading =
            if heading_text.contains('\u{FE0F}') || heading_text.contains('\u{FE0E}') {
                heading_text
                    .chars()
                    .filter(|&c| c != '\u{FE0F}' && c != '\u{FE0E}')
                    .collect()
            } else {
                heading_text
            };
        let (size, is_h1_or_h2) = match level {
            HeadingLevel::H1 => (26.0 * self.font_scale, true),
            HeadingLevel::H2 => (21.0 * self.font_scale, true),
            HeadingLevel::H3 => (18.0 * self.font_scale, false),
            HeadingLevel::H4 => (16.0 * self.font_scale, false),
            HeadingLevel::H5 => (14.5 * self.font_scale, false),
            HeadingLevel::H6 => (13.0 * self.font_scale, false),
        };

        let base_fmt = egui::TextFormat {
            font_id: FontId::proportional(size),
            color: self.theme.text_primary(),
            valign: egui::Align::BOTTOM,
            ..Default::default()
        };

        let segments = crate::emoji::split_text_emojis(&clean_heading);
        let has_emojis = segments
            .iter()
            .any(|s| matches!(s, crate::emoji::TextOrEmoji::Emoji(..)));

        let heading_resp = if has_emojis {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0_f32;
                for seg in segments {
                    match seg {
                        crate::emoji::TextOrEmoji::Emoji(em, svg_str) => {
                            let img_uri = format!("bytes://emoji_{}.svg", em);
                            let em_size = size * 0.92_f32;
                            ui.add(
                                egui::Image::from_bytes(img_uri, svg_str.as_bytes())
                                    .fit_to_exact_size(Vec2::splat(em_size)),
                            );
                            ui.add_space(3.0_f32);
                        }
                        crate::emoji::TextOrEmoji::Text(t) => {
                            let mut job = LayoutJob::default();
                            job.append(t, 0.0, base_fmt.clone());
                            self.label_job(ui, job, Sense::hover());
                        }
                    }
                }
            })
            .response
        } else {
            let mut job = LayoutJob::default();
            job.append(&clean_heading, 0.0, base_fmt);
            self.label_job(ui, job, Sense::hover())
        };

        let generated_slug =
            crate::parsers::unique_heading_slug(&clean_heading, &mut self.heading_counts);
        let slug = self.heading_id.take().unwrap_or(generated_slug);
        if let Some(target) = self.target_anchor {
            if self.anchor_rect.is_none() && is_anchor_match(&slug, target) {
                self.anchor_rect = Some(heading_resp.rect);
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
        self.render_list_item_spans(ui, inlines);
    }

    fn render_list_item_spans(&mut self, ui: &mut Ui, inlines: Vec<InlineSpan>) {
        if inlines.is_empty() {
            return;
        }

        let indent = (self.list_level.saturating_sub(1) as f32) * 16.0_f32;
        let bullet = if let Some(idx) = self.ordered_list_index {
            format!("{}. ", idx)
        } else {
            "• ".to_string()
        };

        let mut spans = Vec::with_capacity(inlines.len() + 1);
        spans.push(InlineSpan {
            text: bullet,
            bold: true,
            italic: false,
            strikethrough: false,
            code: false,
            link_url: None,
        });
        spans.extend(inlines);

        if indent > 0.0_f32 {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(indent);
                self.render_inline_spans(ui, spans, true);
            });
        } else {
            self.render_inline_spans(ui, spans, true);
        }

        if let Some(ref mut idx) = self.ordered_list_index {
            *idx += 1;
        }
    }

    fn render_code_block(&mut self, ui: &mut Ui) {
        let lang_owned = self.code_block_lang.trim().to_string();
        let code_owned = self.code_block_content.trim_end().to_string();
        let lang = lang_owned.as_str();
        let code = code_owned.as_str();

        // 1. Mermaid 向量流程圖即時渲染 (具備記憶體快取、原生微軟正黑體字型疊加與 60fps 滑順捲動)
        if lang.eq_ignore_ascii_case("mermaid") && !code.trim().is_empty() {
            if let Some(diagram) = get_or_render_mermaid_diagram(code) {
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
                                let copy_id = ui
                                    .make_persistent_id(format!("md_mermaid_copy_{:x}", code_hash));
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
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(copy_id, std::time::Instant::now())
                                    });
                                }
                            });
                        });

                        ui.add_space(6.0_f32);
                        ui.separator();
                        ui.add_space(8.0_f32);

                        let available_w = ui.available_width().min(diagram.width * self.font_scale);
                        let aspect_ratio = diagram.height / diagram.width.max(1.0_f32);
                        let display_h = available_w * aspect_ratio;
                        let size = egui::vec2(available_w, display_h);

                        let (rect, _response) = ui.allocate_exact_size(size, Sense::hover());
                        if ui.is_rect_visible(rect) {
                            let uri = format!("bytes://mermaid_{:x}.svg", code_hash);
                            let img_src = egui::ImageSource::Bytes {
                                uri: uri.into(),
                                bytes: egui::load::Bytes::from(diagram.svg_bytes.clone()),
                            };
                            let image = egui::Image::new(img_src)
                                .fit_to_exact_size(size)
                                .rounding(Rounding::same(4.0_f32));
                            image.paint_at(ui, rect);

                            let painter = ui.painter();
                            let scale_x = available_w / diagram.width.max(1.0_f32);
                            let scale_y = display_h / diagram.height.max(1.0_f32);

                            for node in &diagram.text_nodes {
                                let screen_pos = egui::pos2(
                                    rect.min.x + node.x * scale_x,
                                    rect.min.y + node.y * scale_y,
                                );
                                let font_size =
                                    (node.font_size * scale_x * self.font_scale).max(9.0_f32);
                                painter.text(
                                    screen_pos,
                                    node.align,
                                    &node.text,
                                    FontId::proportional(font_size),
                                    if self.theme == AppTheme::Dark {
                                        Color32::from_rgb(229, 231, 235)
                                    } else {
                                        node.color
                                    },
                                );
                            }
                        }
                    });
                ui.add_space(4.0_f32);
                return;
            }
        }

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
                        let copy_id = ui.make_persistent_id(format!(
                            "md_cb_copy_{:p}_{}",
                            code.as_ptr(),
                            code.len()
                        ));
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
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(copy_id, std::time::Instant::now()));
                        }
                    });
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // 語法高亮 (快取 LayoutJob 避免每幀重複執行 syntect 正則高亮，零堆疊分配雜湊)
                let cache_id = egui::Id::new((
                    "md_cb_hl",
                    crate::parsers::content_hash(code),
                    lang,
                    (self.font_scale * 100.0_f32) as u32,
                    self.theme as u8,
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
                                job.append(text, 0.0, base_fmt);
                            }
                        }

                        d.insert_temp(cache_id, job.clone());
                        job
                    }
                });

                self.label_job(ui, layout_job, Sense::hover());
            });
    }
}

/// 渲染現代斑馬紋資料表格
#[allow(clippy::too_many_arguments)]
pub use crate::views::data_table::render_csv_table;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{
        calculate_text_stats, extract_markdown_toc, extract_slides, parse_csv_or_tsv,
    };

    #[test]
    fn test_extract_markdown_toc() {
        let md = r#"
# Introduction
Here is some intro text.

```rust
# This is a code comment, not a heading
fn main() {}
```

## Features
### Blazing Fast
### Instant Preview
"#;
        let toc = extract_markdown_toc(md);
        assert_eq!(toc.len(), 4);

        assert_eq!(toc[0].title, "Introduction");
        assert_eq!(toc[0].level, 1);

        assert_eq!(toc[1].title, "Features");
        assert_eq!(toc[1].level, 2);

        assert_eq!(toc[2].title, "Blazing Fast");
        assert_eq!(toc[2].level, 3);

        assert_eq!(toc[3].title, "Instant Preview");
        assert_eq!(toc[3].level, 3);
    }

    #[test]
    fn test_extract_slides_with_frontmatter() {
        let md = r#"---
title: My Presentation
author: Developer
---

# Slide 1: Welcome
This is the first slide.

---

# Slide 2: Core Architecture
- High Performance
- Pure Rust

---

# Slide 3: Conclusion
Thank you!
"#;
        let slides = extract_slides(md);
        assert_eq!(slides.len(), 3);
        assert!(slides[0].contains("Slide 1: Welcome"));
        assert!(slides[1].contains("Slide 2: Core Architecture"));
        assert!(slides[2].contains("Slide 3: Conclusion"));
    }

    #[test]
    fn test_calculate_text_stats() {
        let text = "你好，世界！ Welcome to flash-md fast preview tool.\n第二行測試文字。";
        let stats = calculate_text_stats(text);

        // 中文字數統計
        assert!(stats.cjk_chars > 0);
        // 英文字詞統計 (Welcome, to, flash, md, fast, preview, tool)
        assert!(stats.words >= 5);
        assert_eq!(stats.lines, 2);
        assert!(stats.reading_time_mins >= 1);
    }

    #[test]
    fn test_parse_csv_and_tsv() {
        let csv_data =
            "Name,Role,City\nAlice,\"Software Engineer, Lead\",Taipei\nBob,Designer,Tokyo";
        let parsed_csv = parse_csv_or_tsv(csv_data, ',');
        assert_eq!(parsed_csv.headers, vec!["Name", "Role", "City"]);
        assert_eq!(parsed_csv.rows.len(), 2);
        assert_eq!(
            parsed_csv.rows[0],
            vec!["Alice", "Software Engineer, Lead", "Taipei"]
        );
        assert_eq!(parsed_csv.rows[1], vec!["Bob", "Designer", "Tokyo"]);

        let tsv_data = "ID\tScore\tGrade\n101\t95.5\tA+\n102\t88.0\tA";
        let parsed_tsv = parse_csv_or_tsv(tsv_data, '\t');
        assert_eq!(parsed_tsv.headers, vec!["ID", "Score", "Grade"]);
        assert_eq!(parsed_tsv.rows.len(), 2);
        assert_eq!(parsed_tsv.rows[0], vec!["101", "95.5", "A+"]);
    }

    #[test]
    fn test_json_format_and_minify() {
        let raw_json = r#"{"name":"flash-md","version":"1.0.89","features":["preview","mindmap"]}"#;
        let formatted = format_json(raw_json).expect("JSON 排版失敗");
        assert!(formatted.contains('\n'));
        assert!(formatted.contains("\"name\": \"flash-md\""));

        let minified = minify_json(&formatted).unwrap();
        assert!(!minified.contains('\n'));
        assert!(minified.contains("\"features\":[\"preview\",\"mindmap\"]"));
    }

    #[test]
    fn test_append_highlighted_text() {
        let mut job = LayoutJob::default();
        let base_fmt = egui::TextFormat::default();
        let mut match_counter = 0;
        append_highlighted_text(
            &mut job,
            "Rust is fast. Pure Rust power.",
            "Rust",
            base_fmt,
            Color32::YELLOW,
            Color32::BLACK,
            Color32::RED,
            Color32::WHITE,
            Some(0),
            &mut match_counter,
        );
        assert_eq!(match_counter, 2);
        assert_eq!(job.text, "Rust is fast. Pure Rust power.");
    }

    #[test]
    fn test_is_code_extension_and_badges() {
        assert!(is_code_extension("rs"));
        assert!(is_code_extension("py"));
        assert!(is_code_extension("ts"));
        assert!(is_code_extension("json"));
        assert!(!is_code_extension("png"));
        assert!(!is_code_extension("unknown_xyz"));

        let (name_rs, emoji_rs) = get_language_badge("rs");
        assert_eq!(name_rs, "Rust");
        assert_eq!(emoji_rs, "🦀");

        let (name_py, emoji_py) = get_language_badge("py");
        assert_eq!(name_py, "Python");
        assert_eq!(emoji_py, "🐍");
    }

    #[test]
    fn test_highlighted_text_multibyte_and_ascii() {
        let mut job = LayoutJob::default();
        let base_fmt = egui::TextFormat::default();
        let mut match_counter = 0;
        append_highlighted_text(
            &mut job,
            "測試繁體中文與 English text 一同高亮搜尋測試",
            "測試",
            base_fmt,
            Color32::YELLOW,
            Color32::BLACK,
            Color32::RED,
            Color32::WHITE,
            Some(1),
            &mut match_counter,
        );
        assert_eq!(match_counter, 2);
        assert_eq!(job.text, "測試繁體中文與 English text 一同高亮搜尋測試");
    }

    #[test]
    fn markdown_search_crosses_inline_styles_and_cached_code_blocks() {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let renderer =
                        MarkdownRenderer::new(AppTheme::Dark, 1.0, "hello", Some(1), None, None);
                    let result = renderer.render(ui, "hel**lo**\n\n```rust\nlet hello = 1;\n```");
                    assert_eq!(result.match_count, 2);
                });
            });
        }
    }

    #[test]
    fn code_cache_refreshes_same_length_edits_and_search_finds_beyond_preview() {
        let ctx = egui::Context::default();
        let large = "let a = 1;\n".repeat(3100) + "let needle = 2;\n";
        for (code, query, expected) in [
            ("let old = 1;", "old", 1),
            ("let new = 1;", "old", 0),
            (large.as_str(), "needle", 1),
        ] {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert_eq!(
                        render_code_viewer(
                            ui,
                            AppTheme::Dark,
                            1.0,
                            code,
                            "rust",
                            query,
                            Some(0),
                            false
                        ),
                        expected
                    );
                });
            });
        }
    }
}
