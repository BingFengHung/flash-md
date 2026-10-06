use super::*;
use egui::Rect;
use std::ops::Range;

pub(super) struct InlineDecoration {
    range: Range<usize>,
    link: Option<String>,
    emoji: Option<(String, &'static str, f32)>,
}

/// Text, links and image-backed Emoji all occupy the same galley. Separate
/// widgets would center a short span against the height of a wrapped span.
pub(super) fn append_inline(
    job: &mut LayoutJob,
    decorations: &mut Vec<InlineDecoration>,
    text: &str,
    format: &egui::TextFormat,
    link: Option<&str>,
    emoji_size: f32,
) {
    for segment in crate::emoji::split_text_emojis(text) {
        let (text, emoji) = match segment {
            crate::emoji::TextOrEmoji::Text(text) => (text, None),
            crate::emoji::TextOrEmoji::Emoji(text, svg) => {
                (text, Some((text.to_string(), svg, emoji_size)))
            }
        };
        let start = job.text.len();
        let mut format = format.clone();
        if emoji.is_some() {
            // Keep the original character for wrapping, selection and search.
            // Its native glyph is covered by the color SVG at its laid-out slot.
            format.color = Color32::TRANSPARENT;
        }
        job.append(text, 0.0, format);
        if emoji.is_some() || link.is_some() {
            decorations.push(InlineDecoration {
                range: start..job.text.len(),
                link: link.map(str::to_owned),
                emoji,
            });
        }
    }
}

impl RenderContext<'_> {
    pub(super) fn label_inline(
        &mut self,
        ui: &mut Ui,
        mut job: LayoutJob,
        decorations: Vec<InlineDecoration>,
    ) -> egui::Response {
        if decorations.is_empty() {
            return self.label_job(ui, job, Sense::hover());
        }
        let active = self.highlight_job(&mut job);
        job.wrap.max_width = ui.available_width();
        let galley = ui.fonts(|fonts| fonts.layout_job(job));
        let (response, target) = crate::search::searchable_galley_target(
            ui,
            galley.clone(),
            self.search_query,
            active,
            self.search_jump,
            Sense::hover(),
        );
        if target.is_some() {
            self.search_rect = target;
        }

        let mut byte = 0;
        let mut decoration = 0;
        for (row_index, row) in galley.rows.iter().enumerate() {
            let mut bounds: Vec<(usize, Rect, f32)> = Vec::new();
            for glyph in &row.glyphs {
                while decoration < decorations.len() && decorations[decoration].range.end <= byte {
                    decoration += 1;
                }
                if decoration < decorations.len() && decorations[decoration].range.contains(&byte) {
                    let rect = Rect::from_min_max(
                        egui::pos2(glyph.pos.x, row.rect.top()),
                        egui::pos2(glyph.max_x(), row.rect.bottom()),
                    )
                    .translate(response.rect.min.to_vec2());
                    let center = response.rect.top() + glyph.pos.y - glyph.font_ascent
                        + 0.5 * glyph.font_height;
                    if let Some((last, previous, _)) = bounds.last_mut() {
                        if *last == decoration {
                            *previous = previous.union(rect);
                        } else {
                            bounds.push((decoration, rect, center));
                        }
                    } else {
                        bounds.push((decoration, rect, center));
                    }
                }
                byte += glyph.chr.len_utf8();
            }
            if row.ends_with_newline {
                byte += 1;
            }
            for (index, rect, center) in bounds {
                let decoration = &decorations[index];
                if let Some((emoji, svg, size)) = &decoration.emoji {
                    let size = size.min(rect.width()).min(rect.height());
                    let image_rect = Rect::from_center_size(
                        egui::pos2(rect.center().x, center),
                        Vec2::splat(size),
                    );
                    egui::Image::from_bytes(format!("bytes://emoji_{emoji}.svg"), svg.as_bytes())
                        .paint_at(ui, image_rect);
                }
                if let Some(url) = &decoration.link {
                    let link = ui.interact(
                        rect,
                        response.id.with(("inline-link", row_index, index)),
                        Sense::click(),
                    );
                    if link.hovered() {
                        ui.output_mut(|output| output.cursor_icon = egui::CursorIcon::PointingHand);
                    }
                    if link.clicked() {
                        if let Some(anchor) = url.strip_prefix('#') {
                            self.clicked_anchor = Some(anchor.to_string());
                        } else {
                            let _ = open::that(url);
                        }
                    }
                    link.on_hover_text(url);
                }
            }
        }
        response
    }
}
