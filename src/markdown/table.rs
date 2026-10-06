use super::*;
use egui::{Align, Layout, Rect, UiBuilder};

impl RenderContext<'_> {
    pub(super) fn render_table(&mut self, ui: &mut Ui) {
        let columns = self
            .table_headers
            .len()
            .max(self.table_rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns == 0 {
            return;
        }

        let padding = Vec2::new(10.0_f32, 6.0_f32) * self.font_scale;
        let font = FontId::proportional(13.5_f32 * self.font_scale);
        let widths: Vec<f32> = (0..columns)
            .map(|column| {
                let natural = std::iter::once(&self.table_headers)
                    .chain(self.table_rows.iter())
                    .filter_map(|row| row.get(column))
                    .map(|text| {
                        ui.fonts(|fonts| {
                            fonts
                                .layout_no_wrap(text.clone(), font.clone(), Color32::WHITE)
                                .size()
                                .x
                        })
                    })
                    .fold(0.0_f32, f32::max);
                (natural + 2.0_f32 * padding.x)
                    .clamp(96.0_f32 * self.font_scale, 320.0_f32 * self.font_scale)
            })
            .collect();
        let headers = self.table_headers.clone();
        let rows = self.table_rows.clone();

        ui.add_space(4.0_f32);
        let scroll_id = ui.next_auto_id();
        egui::ScrollArea::horizontal()
            .id_salt(scroll_id)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0_f32;
                if !headers.is_empty() {
                    self.render_table_row(ui, &headers, &widths, padding, true, 0);
                }
                for (index, row) in rows.iter().enumerate() {
                    self.render_table_row(ui, row, &widths, padding, false, index + 1);
                }
            });
        ui.add_space(6.0_f32);
    }

    fn render_table_row(
        &mut self,
        ui: &mut Ui,
        row: &[String],
        widths: &[f32],
        padding: Vec2,
        header: bool,
        row_index: usize,
    ) {
        // Lay out each cell at its final column width before allocating the row.
        // This keeps columns stable on the first frame and makes every cell in
        // a wrapped row share the same height.
        let cells: Vec<_> = widths
            .iter()
            .enumerate()
            .map(|(column, width)| {
                let mut job = LayoutJob::default();
                job.wrap.max_width = width - 2.0_f32 * padding.x;
                job.wrap.break_anywhere = true;
                job.append(
                    row.get(column).map(String::as_str).unwrap_or(""),
                    0.0_f32,
                    egui::TextFormat {
                        font_id: FontId::proportional(13.5_f32 * self.font_scale),
                        color: if header {
                            self.theme.accent_color()
                        } else {
                            self.theme.text_primary()
                        },
                        line_height: Some(19.0_f32 * self.font_scale),
                        ..Default::default()
                    },
                );
                let active = self.highlight_job(&mut job);
                (ui.fonts(|fonts| fonts.layout_job(job)), active)
            })
            .collect();
        let height = cells
            .iter()
            .map(|(galley, _)| galley.size().y)
            .fold(19.0_f32 * self.font_scale, f32::max)
            + 2.0_f32 * padding.y;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(widths.iter().sum(), height), Sense::hover());
        let background = if header {
            self.theme.card_bg_color()
        } else if row_index % 2 == 1 {
            self.theme.bg_color()
        } else {
            match self.theme {
                AppTheme::Dark => Color32::from_rgba_unmultiplied(255, 255, 255, 6),
                AppTheme::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 8),
            }
        };
        let border = Stroke::new(0.5_f32, self.theme.border_color());
        ui.painter().rect_filled(rect, Rounding::ZERO, background);
        ui.painter().rect_stroke(rect, Rounding::ZERO, border);

        let mut x = rect.left();
        for (column, ((galley, active), width)) in cells.into_iter().zip(widths).enumerate() {
            let cell_rect =
                Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(*width, height));
            let align = match self.table_alignments.get(column) {
                Some(Alignment::Center) => Align::Center,
                Some(Alignment::Right) => Align::RIGHT,
                _ => Align::LEFT,
            };
            let inner_rect = cell_rect.shrink2(padding);
            let mut cell_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt(("markdown-cell", row_index, column))
                    .max_rect(inner_rect)
                    .layout(Layout::top_down(align)),
            );
            cell_ui.set_clip_rect(inner_rect.intersect(ui.clip_rect()));
            crate::search::searchable_galley(
                &mut cell_ui,
                galley,
                self.search_query,
                active,
                self.search_jump,
                Sense::hover(),
            );
            x += *width;
            if column + 1 < widths.len() {
                ui.painter().vline(x, rect.y_range(), border);
            }
        }
    }
}
