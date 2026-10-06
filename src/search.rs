use egui::text::{CCursor, LayoutJob, LayoutSection};
use egui::{Align, Color32, Response, Sense, Ui};
use std::ops::Range;
use std::sync::Arc;

/// Map normalized matches back to original UTF-8 byte ranges, including
/// characters whose lowercase form expands to more than one character.
pub fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    if text.is_ascii() && needle.is_ascii() {
        return text
            .to_ascii_lowercase()
            .match_indices(&needle)
            .map(|(start, value)| start..start + value.len())
            .collect();
    }
    let mut normalized = String::new();
    let mut offsets = Vec::new();
    for (start, ch) in text.char_indices() {
        let end = start + ch.len_utf8();
        for lower in ch.to_lowercase() {
            normalized.push(lower);
            offsets.extend(std::iter::repeat_n((start, end), lower.len_utf8()));
        }
    }
    let mut matches = Vec::new();
    for (index, value) in normalized.match_indices(&needle) {
        let range = offsets[index].0..offsets[index + value.len() - 1].1;
        if matches
            .last()
            .is_none_or(|last: &Range<usize>| last.end <= range.start)
        {
            matches.push(range);
        }
    }
    matches
}

#[allow(clippy::too_many_arguments)]
pub fn highlight_job(
    job: &mut LayoutJob,
    query: &str,
    active: Option<usize>,
    counter: &mut usize,
    normal_bg: Color32,
    normal_fg: Color32,
    active_bg: Color32,
    active_fg: Color32,
) {
    let matches = find_matches(&job.text, query);
    let base = *counter;
    *counter += matches.len();
    let matches: Vec<_> = matches
        .into_iter()
        .enumerate()
        .map(|(index, range)| (base + index, range))
        .collect();
    highlight_ranges(
        job,
        &matches,
        active,
        (normal_bg, normal_fg, active_bg, active_fg),
    );
}

/// Color already-indexed source matches, retaining their global match number
/// when only a visible line of a virtual document is laid out.
pub fn highlight_ranges(
    job: &mut LayoutJob,
    matches: &[(usize, Range<usize>)],
    active: Option<usize>,
    colors: (Color32, Color32, Color32, Color32),
) {
    if matches.is_empty() {
        return;
    }
    let mut sections = Vec::new();
    for section in &job.sections {
        let mut position = section.byte_range.start;
        let first = matches.partition_point(|(_, range)| range.end <= section.byte_range.start);
        for (index, range) in matches[first..]
            .iter()
            .take_while(|(_, range)| range.start < section.byte_range.end)
        {
            let start = range.start.max(section.byte_range.start);
            let end = range.end.min(section.byte_range.end);
            if start >= end {
                continue;
            }
            if position < start {
                sections.push(LayoutSection {
                    leading_space: if position == section.byte_range.start {
                        section.leading_space
                    } else {
                        0.0
                    },
                    byte_range: position..start,
                    format: section.format.clone(),
                });
            }
            let mut format = section.format.clone();
            let selected = active == Some(*index);
            format.background = if selected { colors.2 } else { colors.0 };
            format.color = if selected { colors.3 } else { colors.1 };
            sections.push(LayoutSection {
                leading_space: if start == section.byte_range.start {
                    section.leading_space
                } else {
                    0.0_f32
                },
                byte_range: start..end,
                format,
            });
            position = end;
        }
        if position < section.byte_range.end {
            sections.push(LayoutSection {
                leading_space: if position == section.byte_range.start {
                    section.leading_space
                } else {
                    0.0_f32
                },
                byte_range: position..section.byte_range.end,
                format: section.format.clone(),
            });
        }
    }
    job.sections = sections;
}

/// Scroll using the actual galley cursor position, respecting wrapping,
/// font size, images and preceding blocks rather than estimated line heights.
pub fn searchable_label(
    ui: &mut Ui,
    job: LayoutJob,
    query: &str,
    active_local: Option<usize>,
    jump: bool,
    sense: Sense,
    wrap: bool,
) -> Response {
    let (response, target) =
        searchable_label_target(ui, job, query, active_local, jump, sense, wrap);
    if let Some(target) = target {
        ui.scroll_to_rect(target, Some(Align::Center));
    }
    response
}

pub fn searchable_label_target(
    ui: &mut Ui,
    mut job: LayoutJob,
    query: &str,
    active_local: Option<usize>,
    jump: bool,
    sense: Sense,
    wrap: bool,
) -> (Response, Option<egui::Rect>) {
    job.wrap.max_width = if wrap {
        ui.available_width()
    } else {
        f32::INFINITY
    };
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    searchable_galley_target(ui, galley, query, active_local, jump, sense)
}

pub fn searchable_galley_target(
    ui: &mut Ui,
    galley: Arc<egui::Galley>,
    query: &str,
    active_local: Option<usize>,
    jump: bool,
    sense: Sense,
) -> (Response, Option<egui::Rect>) {
    let match_start = if jump {
        active_local.and_then(|index| {
            find_matches(galley.text(), query)
                .get(index)
                .map(|range| range.start)
        })
    } else {
        None
    };
    let response = ui.add(egui::Label::new(galley.clone()).sense(sense));
    let target = match_start.map(|start| {
        let chars = galley.text()[..start].chars().count();
        let cursor = galley.from_ccursor(CCursor::new(chars));
        galley
            .pos_from_cursor(&cursor)
            .translate(response.rect.min.to_vec2())
    });
    (response, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_plain_text_can_keep_physical_lines_while_prose_wraps() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(96.0, 600.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(ui.available_width() <= 96.0);
                let mut job = LayoutJob::default();
                job.append(&"long_text ".repeat(30), 0.0, Default::default());
                let wrapped =
                    searchable_label(ui, job.clone(), "", None, false, Sense::hover(), true);
                let unwrapped = searchable_label(ui, job, "", None, false, Sense::hover(), false);
                assert!(wrapped.rect.height() > unwrapped.rect.height());
            });
        });
    }

    #[test]
    fn unicode_matches_preserve_original_byte_boundaries() {
        let text = "İstanbul 中文 TEST";
        let matches = find_matches(text, "i");
        assert_eq!(&text[matches[0].clone()], "İ");
        assert_eq!(&text[find_matches(text, "中文")[0].clone()], "中文");
        assert_eq!(&text[find_matches(text, "test")[0].clone()], "TEST");
    }

    #[test]
    fn matching_across_syntax_or_inline_style_sections_is_one_match() {
        let mut job = LayoutJob::default();
        job.append("hel", 0.0_f32, Default::default());
        job.append("lo", 0.0_f32, Default::default());
        let mut count = 0;
        highlight_job(
            &mut job,
            "hello",
            Some(0),
            &mut count,
            Color32::YELLOW,
            Color32::BLACK,
            Color32::RED,
            Color32::WHITE,
        );
        assert_eq!(count, 1);
        assert_eq!(job.text, "hello");
        assert!(job
            .sections
            .iter()
            .all(|section| section.format.background == Color32::RED));
    }
}
