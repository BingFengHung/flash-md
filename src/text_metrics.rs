use egui::{Color32, FontId, Ui};
use std::collections::HashMap;

/// Measure runs using the font's actual advances, kerning and pixel
/// rounding. Reuse the small set of glyph pairs instead of making a galley
/// (and a text mesh) for every numbered row in a large document.
pub struct TextMetrics<'a> {
    ui: &'a Ui,
    font: FontId,
    pixels_per_point: f32,
    advances: [Option<f32>; 128],
    unicode_advances: HashMap<char, f32>,
    pairs: HashMap<(char, char), f32>,
}

impl<'a> TextMetrics<'a> {
    pub fn new(ui: &'a Ui, font: FontId) -> Self {
        Self {
            ui,
            font,
            pixels_per_point: ui.ctx().pixels_per_point(),
            advances: [None; 128],
            unicode_advances: HashMap::new(),
            pairs: HashMap::new(),
        }
    }

    fn round(&self, value: f32) -> f32 {
        (value * self.pixels_per_point).round() / self.pixels_per_point
    }

    fn full_width(&self, text: &str, section_bound: bool) -> f32 {
        let galley = self.ui.fonts(|fonts| {
            fonts.layout_no_wrap(text.to_string(), self.font.clone(), Color32::WHITE)
        });
        if !section_bound {
            return galley
                .rows
                .iter()
                .map(|row| row.rect.width())
                .fold(0.0, f32::max);
        }
        // Syntax colors split runs and reset kerning at section boundaries.
        // Dropping negative kerning provides a stable width that also fits
        // those sections, without depending on when colors become ready.
        galley
            .rows
            .iter()
            .map(|row| {
                let mut x = 0.0;
                let mut old_end = 0.0;
                let mut width = 0.0;
                for glyph in &row.glyphs {
                    x += (glyph.pos.x - self.round(old_end)).max(0.0);
                    width = x + glyph.advance_width;
                    x = self.round(width);
                    old_end = glyph.pos.x + glyph.advance_width;
                }
                width
            })
            .fold(0.0, f32::max)
    }

    pub fn width(&mut self, text: &str, section_bound: bool) -> f32 {
        if text.contains('\n') {
            return self.full_width(text, section_bound);
        }
        let mut x = 0.0;
        let mut width = 0.0;
        let mut previous = None;
        for ch in text.chars() {
            let cached = if ch.is_ascii() {
                self.advances[ch as usize]
            } else {
                self.unicode_advances.get(&ch).copied()
            };
            let advance = if let Some(value) = cached {
                value
            } else {
                let galley = self.ui.fonts(|fonts| {
                    fonts.layout_no_wrap(ch.to_string(), self.font.clone(), Color32::WHITE)
                });
                let value = galley.rows[0].glyphs[0].advance_width;
                if ch.is_ascii() {
                    self.advances[ch as usize] = Some(value);
                } else {
                    self.unicode_advances.insert(ch, value);
                }
                value
            };
            if let Some(prev) = previous {
                let kerning = if let Some(value) = self.pairs.get(&(prev, ch)) {
                    *value
                } else {
                    // High-diversity prose should not grow an unbounded pair cache.
                    if self.pairs.len() >= 4096 {
                        return self.full_width(text, section_bound);
                    }
                    let mut pair = String::new();
                    pair.push(prev);
                    pair.push(ch);
                    let galley = self.ui.fonts(|fonts| {
                        fonts.layout_no_wrap(pair, self.font.clone(), Color32::WHITE)
                    });
                    let glyphs = &galley.rows[0].glyphs;
                    let value = glyphs[1].pos.x - self.round(glyphs[0].advance_width);
                    self.pairs.insert((prev, ch), value);
                    value
                };
                x += if section_bound {
                    kerning.max(0.0)
                } else {
                    kerning
                };
            }
            width = x + advance;
            x = self.round(width);
            previous = Some(ch);
        }
        width.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_match_real_glyph_positions_at_multiple_scales() {
        for pixels_per_point in [1.0, 1.25, 2.0] {
            let ctx = egui::Context::default();
            crate::theme::setup_system_cjk_fonts(&ctx);
            ctx.set_pixels_per_point(pixels_per_point);
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for font in [FontId::proportional(13.5), FontId::monospace(18.9)] {
                        let mut metrics = TextMetrics::new(ui, font.clone());
                        for text in [
                            "",
                            "row9999",
                            "AVATAR To Wa",
                            "1.00\t2026",
                            "中文 emoji 😀",
                            "one\ntwo",
                        ] {
                            let actual = ui.fonts(|fonts| {
                                fonts.layout_no_wrap(text.into(), font.clone(), Color32::WHITE)
                            });
                            let width = actual
                                .rows
                                .iter()
                                .map(|row| row.rect.width())
                                .fold(0.0, f32::max);
                            assert!(
                                (metrics.width(text, false) - width).abs() < 0.01,
                                "{text:?}: measured width differs at {pixels_per_point}"
                            );
                            assert!(metrics.width(text, true) + 0.01 >= width);
                        }
                    }
                });
            });
        }
    }
}
