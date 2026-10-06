use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::sync::Arc;

pub fn content_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn without_utf8_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

pub fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
}

pub fn cached_events(ctx: &egui::Context, content: &str) -> Arc<Vec<Event<'static>>> {
    let hash = content_hash(content);
    let id = egui::Id::new("flash-md-parsed-events");
    ctx.data_mut(|data| {
        if let Some((old_hash, events)) = data.get_temp::<(u64, Arc<Vec<Event<'static>>>)>(id) {
            if old_hash == hash {
                return events;
            }
        }
        let events: Arc<Vec<Event<'static>>> = Arc::new(
            Parser::new_ext(without_utf8_bom(content), markdown_options())
                .map(Event::into_static)
                .collect(),
        );
        data.insert_temp(id, (hash, events.clone()));
        events
    })
}

pub fn heading_slug(title: &str) -> String {
    let slug: String = title
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| {
            if c.is_whitespace() {
                Some('-')
            } else if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

pub fn unique_heading_slug(title: &str, counts: &mut HashMap<String, usize>) -> String {
    let slug = heading_slug(title);
    let mut count = counts.get(&slug).copied().unwrap_or(0);
    let result = loop {
        let candidate = if count == 0 {
            slug.clone()
        } else {
            format!("{slug}-{count}")
        };
        if !counts.contains_key(&candidate) {
            break candidate;
        }
        count += 1;
    };
    counts.insert(slug, count + 1);
    counts.entry(result.clone()).or_insert(1);
    result
}

#[derive(Debug, Clone)]
pub struct TocItem {
    pub level: u8,
    pub title: String,
    pub line_idx: usize,
    pub anchor: String,
}

pub fn extract_markdown_toc(content: &str) -> Vec<TocItem> {
    let content = without_utf8_bom(content);
    let starts: Vec<_> = std::iter::once(0)
        .chain(content.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut items = Vec::new();
    let mut heading = None;
    let mut title = String::new();
    let mut counts: HashMap<String, usize> = Parser::new_ext(content, markdown_options())
        .filter_map(|event| match event {
            Event::Start(Tag::Heading { id: Some(id), .. }) => Some((id.into_string(), 1)),
            _ => None,
        })
        .collect();
    for (event, range) in Parser::new_ext(content, markdown_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, id, .. }) => {
                heading = Some((
                    level as u8,
                    id.map(|id| id.into_string()),
                    starts
                        .partition_point(|start| *start <= range.start)
                        .saturating_sub(1),
                ));
                title.clear();
            }
            Event::Text(t) | Event::Code(t) if heading.is_some() => title.push_str(&t),
            Event::SoftBreak | Event::HardBreak if heading.is_some() => title.push(' '),
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, id, line_idx)) = heading.take() {
                    let anchor = id.unwrap_or_else(|| unique_heading_slug(&title, &mut counts));
                    items.push(TocItem {
                        level,
                        title: title.clone(),
                        line_idx,
                        anchor,
                    });
                }
            }
            _ => {}
        }
    }
    items
}

pub fn cached_toc(ctx: &egui::Context, content: &str) -> Arc<Vec<TocItem>> {
    let hash = content_hash(content);
    let id = egui::Id::new("flash-md-toc");
    ctx.data_mut(|data| {
        if let Some((old_hash, value)) = data.get_temp::<(u64, Arc<Vec<TocItem>>)>(id) {
            if old_hash == hash {
                return value;
            }
        }
        let value = Arc::new(extract_markdown_toc(content));
        data.insert_temp(id, (hash, value.clone()));
        value
    })
}

#[derive(Debug, Clone)]
pub struct CsvTableData {
    pub fingerprint: u64,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub total_cols: usize,
    pub error: Option<String>,
}

pub fn parse_csv_or_tsv(content: &str, separator: char) -> CsvTableData {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(separator as u8)
        .has_headers(false)
        .flexible(true)
        .from_reader(content.as_bytes());
    let mut records = Vec::new();
    let mut error = None;
    for row in reader.records() {
        match row {
            Ok(row) => records.push(row.iter().map(str::to_owned).collect::<Vec<_>>()),
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        }
    }
    let headers = if records.is_empty() {
        Vec::new()
    } else {
        records.remove(0)
    };
    let total_cols = headers
        .len()
        .max(records.iter().map(Vec::len).max().unwrap_or(0));
    let total_rows = records.len();
    CsvTableData {
        fingerprint: content_hash(content) ^ (separator as u64).rotate_left(32),
        headers,
        rows: records,
        total_rows,
        total_cols,
        error,
    }
}

pub fn cached_csv(ctx: &egui::Context, content: &str, separator: char) -> Arc<CsvTableData> {
    let key = (content_hash(content), separator);
    let id = egui::Id::new("flash-md-csv");
    ctx.data_mut(|data| {
        if let Some((old_key, value)) = data.get_temp::<((u64, char), Arc<CsvTableData>)>(id) {
            if old_key == key {
                return value;
            }
        }
        let value = Arc::new(parse_csv_or_tsv(content, separator));
        data.insert_temp(id, (key, value.clone()));
        value
    })
}

pub fn format_json(input: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(without_utf8_bom(input)).map_err(|e| e.to_string())?;
    let formatted = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    Ok(if input.starts_with('\u{feff}') {
        format!("\u{feff}{formatted}")
    } else {
        formatted
    })
}

pub fn minify_json(input: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(without_utf8_bom(input)).map_err(|e| e.to_string())?;
    let formatted = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    Ok(if input.starts_with('\u{feff}') {
        format!("\u{feff}{formatted}")
    } else {
        formatted
    })
}

pub fn extract_slides(content: &str) -> Vec<String> {
    let content = without_utf8_bom(content);
    if content.trim().is_empty() {
        return vec!["# 📽 簡報模式\n\n此文件暫無內容。".to_string()];
    }
    let mut body = content;
    if content
        .lines()
        .next()
        .is_some_and(|line| line.trim() == "---")
    {
        let mut offset = content.find('\n').map_or(content.len(), |n| n + 1);
        for line in content[offset..].split_inclusive('\n') {
            offset += line.len();
            if line.trim() == "---" {
                body = &content[offset..];
                break;
            }
        }
    }
    let mut slides = Vec::new();
    let mut start = 0;
    let mut depth: usize = 0;
    for (event, range) in Parser::new_ext(body, markdown_options()).into_offset_iter() {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Rule if depth == 0 => {
                let slide = body[start..range.start].trim();
                if !slide.is_empty() {
                    slides.push(slide.to_string());
                }
                start = range.end;
            }
            _ => {}
        }
    }
    let final_slide = body[start..].trim();
    if !final_slide.is_empty() {
        slides.push(final_slide.to_string());
    }
    if slides.is_empty() {
        slides.push(body.trim().to_string());
    }
    slides
}

/// Markdown / 文本統計數據
#[derive(Debug, Clone, Copy, Default)]
pub struct TextStats {
    pub cjk_chars: usize,
    pub words: usize,
    pub total_chars: usize,
    #[allow(dead_code)]
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

pub fn cached_stats(ctx: &egui::Context, text: &str) -> TextStats {
    let hash = content_hash(text);
    let id = egui::Id::new("flash-md-text-stats");
    ctx.data_mut(|data| {
        if let Some((old_hash, value)) = data.get_temp::<(u64, TextStats)>(id) {
            if hash == old_hash {
                return value;
            }
        }
        let value = calculate_text_stats(text);
        data.insert_temp(id, (hash, value));
        value
    })
}

pub fn cached_slides(ctx: &egui::Context, text: &str) -> Arc<Vec<String>> {
    let hash = content_hash(text);
    let id = egui::Id::new("flash-md-slides");
    ctx.data_mut(|data| {
        if let Some((old_hash, value)) = data.get_temp::<(u64, Arc<Vec<String>>)>(id) {
            if hash == old_hash {
                return value;
            }
        }
        let value = Arc::new(extract_slides(text));
        data.insert_temp(id, (hash, value.clone()));
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_preserves_first_heading_line_numbers_frontmatter_and_json_roundtrips() {
        let document = "\u{feff}# First\n\n## Second";
        let toc = extract_markdown_toc(document);
        assert_eq!(toc.len(), 2);
        assert_eq!((&*toc[0].anchor, toc[0].line_idx), ("first", 0));
        assert_eq!((&*toc[1].anchor, toc[1].line_idx), ("second", 2));
        let events = cached_events(&egui::Context::default(), document);
        assert!(matches!(
            events.first(),
            Some(Event::Start(Tag::Heading { .. }))
        ));
        let slides = extract_slides("\u{feff}---\ntitle: Fixture\n---\n# First\n\n---\n\n# Second");
        assert_eq!(slides, vec!["# First".to_string(), "# Second".to_string()]);
        let source = "\u{feff}{\"text\":\"中文\",\"count\":42}";
        let pretty = format_json(source).unwrap();
        assert!(pretty.starts_with('\u{feff}'));
        assert_eq!(minify_json(&pretty).unwrap(), source);
    }

    #[test]
    fn automatic_heading_anchors_reserve_explicit_ids_even_when_defined_later() {
        let toc = extract_markdown_toc(
            "# Target\n# Explicit {#target}\n# Explicit\n# Target\n# Target-1",
        );
        let anchors: Vec<_> = toc.iter().map(|item| item.anchor.as_str()).collect();
        assert_eq!(
            anchors,
            ["target-1", "target", "explicit", "target-2", "target-1-1"]
        );
        let toc = extract_markdown_toc("# Title ![Badge](missing.png)");
        assert_eq!(toc[0].anchor, "title-badge");
    }

    #[test]
    fn csv_preserves_quoted_newlines_spaces_and_escaped_quotes() {
        let data = parse_csv_or_tsv(
            "name,note\r\nJoe,\" first\nsecond \"\r\nAmy,\"said \"\"hello\"\"\"",
            ',',
        );
        assert_eq!(data.total_rows, 2);
        assert_eq!(data.rows[0][1], " first\nsecond ");
        assert_eq!(data.rows[1][1], "said \"hello\"");
        assert!(data.error.is_none());
    }

    #[test]
    fn json_rejects_invalid_structure_and_preserves_strings() {
        for invalid in [
            "{\"a\":}",
            "{]",
            "true false",
            "{\"a\":1,}",
            "\"unterminated",
        ] {
            assert!(format_json(invalid).is_err(), "{invalid}");
            assert!(minify_json(invalid).is_err(), "{invalid}");
        }
        let input =
            r#"{"text":"繁體 中文","number":123456789012345678901234567890,"values":[true,null]}"#;
        let pretty = format_json(input).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&pretty).unwrap(),
            serde_json::from_str::<serde_json::Value>(input).unwrap()
        );
    }

    #[test]
    fn slides_do_not_split_fenced_code_or_setext_headings() {
        let slides =
            extract_slides("# one\n\n```yaml\n---\n***\n___\n```\n\n---\n\nTitle\n-----\n\nbody");
        assert_eq!(slides.len(), 2);
        assert!(slides[0].contains("```yaml\n---\n***\n___\n```"));
        assert!(slides[1].contains("Title\n-----"));
    }

    #[test]
    fn toc_uses_markdown_rules_and_unique_heading_anchors() {
        let toc = extract_markdown_toc(
            "#No heading\n\n# Intro\n\n## Intro\n\nSetext\n=====\n\n```\n# not heading\n```\n",
        );
        assert_eq!(toc.len(), 3);
        assert_eq!(toc[0].anchor, "intro");
        assert_eq!(toc[1].anchor, "intro-1");
        assert_eq!(toc[2].title, "Setext");
        let toc = extract_markdown_toc("# Intro\n# Intro\n# Intro-1\n# Custom {#target}");
        assert_eq!(toc[2].anchor, "intro-1-1");
        assert_eq!(toc[3].anchor, "target");
    }

    #[test]
    fn cached_parse_refreshes_after_same_length_changes() {
        let ctx = egui::Context::default();
        let old = cached_toc(&ctx, "# OLD");
        let new = cached_toc(&ctx, "# NEW");
        assert_eq!(old[0].title, "OLD");
        assert_eq!(new[0].title, "NEW");
        let old = cached_events(&ctx, "OLD");
        let new = cached_events(&ctx, "NEW");
        assert_ne!(format!("{old:?}"), format!("{new:?}"));
    }
}
