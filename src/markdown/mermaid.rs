use super::*;

#[derive(Debug, Clone)]
pub struct MermaidTextNode {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: Color32,
    pub align: Align2,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct MermaidDiagramData {
    pub svg_bytes: Vec<u8>,
    pub width: f32,
    pub height: f32,
    pub text_nodes: Vec<MermaidTextNode>,
}

fn extract_attr_f32(s: &str, attr: &str) -> Option<f32> {
    let pos = s.find(attr)?;
    let after = &s[pos + attr.len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after[1..];
    let end = rest.find(quote)?;
    rest[..end].trim().parse::<f32>().ok()
}

pub(super) fn extract_attr_str(s: &str, attr: &str) -> Option<String> {
    let pos = s.find(attr)?;
    let after = &s[pos + attr.len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn parse_hex_color(s: &str) -> Option<Color32> {
    let hex = s.trim().trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color32::from_rgb(r, g, b))
    } else if hex.len() == 3 {
        let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
        Some(Color32::from_rgb(r, g, b))
    } else {
        None
    }
}

/// 快取 Mermaid 圖表解析與向量文字節點資料
pub fn get_or_render_mermaid_diagram(code: &str) -> Option<MermaidDiagramData> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<u64, Option<MermaidDiagramData>>>> = Mutex::new(None);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    let key = hasher.finish();

    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);

    if let Some(cached) = map.get(&key) {
        return cached.clone();
    }
    drop(guard);

    let parsed = (|| -> Option<MermaidDiagramData> {
        let svg_str = mermaid_rs_renderer::render(code).ok()?;

        // 1. 解析 viewBox 或 width / height
        let mut vb_w = 400.0_f32;
        let mut vb_h = 300.0_f32;

        if let Some(pos) = svg_str.find("viewBox=\"") {
            let after = &svg_str[pos + 9..];
            if let Some(end) = after.find('"') {
                let parts: Vec<&str> = after[..end].split_whitespace().collect();
                if parts.len() == 4 {
                    if let (Ok(w), Ok(h)) = (parts[2].parse::<f32>(), parts[3].parse::<f32>()) {
                        if w > 1.0_f32 && h > 1.0_f32 {
                            vb_w = w;
                            vb_h = h;
                        }
                    }
                }
            }
        } else if let (Some(w_str), Some(h_str)) = (
            extract_attr_str(&svg_str, "width="),
            extract_attr_str(&svg_str, "height="),
        ) {
            let w_val = w_str.trim().trim_end_matches("px").parse::<f32>();
            let h_val = h_str.trim().trim_end_matches("px").parse::<f32>();
            if let (Ok(w), Ok(h)) = (w_val, h_val) {
                if w > 1.0_f32 && h > 1.0_f32 {
                    vb_w = w;
                    vb_h = h;
                }
            }
        }

        // 2. 提取所有 <text ...>內容</text> 節點
        let mut text_nodes = Vec::new();
        let mut search_idx = 0;
        while let Some(start_tag) = svg_str[search_idx..].find("<text") {
            let text_start = search_idx + start_tag;
            if let Some(tag_close) = svg_str[text_start..].find('>') {
                let tag_attrs = &svg_str[text_start..text_start + tag_close];
                let content_start = text_start + tag_close + 1;
                if let Some(end_tag) = svg_str[content_start..].find("</text>") {
                    let raw_content = &svg_str[content_start..content_start + end_tag];

                    let mut clean_text = raw_content.to_string();
                    while let Some(s) = clean_text.find('<') {
                        if let Some(e) = clean_text[s..].find('>') {
                            clean_text.replace_range(s..s + e + 1, "");
                        } else {
                            break;
                        }
                    }
                    clean_text = clean_text
                        .replace("&amp;", "&")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&quot;", "\"")
                        .replace("&#39;", "'")
                        .trim()
                        .to_string();

                    if !clean_text.is_empty() {
                        let x = extract_attr_f32(tag_attrs, "x=").unwrap_or(0.0_f32);
                        let y = extract_attr_f32(tag_attrs, "y=").unwrap_or(0.0_f32);
                        let font_size =
                            extract_attr_f32(tag_attrs, "font-size=").unwrap_or(14.0_f32);

                        let align = if tag_attrs.contains("text-anchor=\"middle\"") {
                            Align2::CENTER_CENTER
                        } else if tag_attrs.contains("text-anchor=\"end\"") {
                            Align2::RIGHT_CENTER
                        } else {
                            Align2::LEFT_CENTER
                        };

                        let color = if let Some(fill_str) = extract_attr_str(tag_attrs, "fill=") {
                            parse_hex_color(&fill_str).unwrap_or(Color32::from_rgb(31, 41, 55))
                        } else {
                            Color32::from_rgb(31, 41, 55)
                        };

                        text_nodes.push(MermaidTextNode {
                            x,
                            y,
                            font_size,
                            color,
                            align,
                            text: clean_text,
                        });
                    }
                    search_idx = content_start + end_tag + 7;
                    continue;
                }
            }
            search_idx += start_tag + 5;
        }

        Some(MermaidDiagramData {
            svg_bytes: svg_str.into_bytes(),
            width: vb_w,
            height: vb_h,
            text_nodes,
        })
    })();

    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    if map.len() >= 64 {
        map.clear();
    }
    map.insert(key, parsed.clone());
    parsed
}
