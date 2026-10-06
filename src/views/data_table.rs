use crate::parsers::CsvTableData;
use crate::theme::AppTheme;
use egui::text::{CCursor, LayoutJob};
use egui::{Color32, FontId, Rect, Rounding, Sense, Stroke, Ui, Vec2};
use std::sync::Arc;

#[derive(Clone)]
struct Cell {
    job: LayoutJob,
    galley: Arc<egui::Galley>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_thousand_rows_only_paint_the_viewport_and_search_reaches_the_last_row() {
        let source = format!(
            "name,value\n{}",
            (0..10_000)
                .map(|index| format!("row{index},value{index}\n"))
                .collect::<String>()
        );
        let data = crate::parsers::parse_csv_or_tsv(&source, ',');
        let ctx = egui::Context::default();
        for frame in 0..3 {
            let mut matches = 0;
            let mut offset = 0.0_f32;
            let mut content_height = 0.0_f32;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0_f32, 400.0_f32),
                    )),
                    time: Some(frame as f64 * 0.1_f64),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let result = egui::ScrollArea::both().animated(false).show(ui, |ui| {
                            render_csv_table(
                                ui,
                                AppTheme::Light,
                                1.0_f32,
                                &data,
                                "row9999",
                                Some(0),
                                &mut matches,
                                frame == 0,
                            );
                        });
                        offset = result.state.offset.y;
                        content_height = result.content_size.y;
                    });
                },
            );
            assert_eq!(matches, 1);
            assert!(content_height > 300_000.0_f32);
            assert!(offset > 300_000.0_f32);
            let text_count = output
                .shapes
                .iter()
                .filter(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(_)))
                .count();
            assert!(text_count < 80, "painted {text_count} text shapes");
            if frame > 0 {
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "row9999")));
            }
        }
    }

    #[test]
    fn table_layout_cache_keeps_multiline_row_height_and_distinguishes_delimiters() {
        let ctx = egui::Context::default();
        let source = "name,note\nJoe,\"first\nsecond\nthird\"\nAmy,short";
        let csv = crate::parsers::parse_csv_or_tsv(source, ',');
        let tsv = crate::parsers::parse_csv_or_tsv(source, '\t');
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let first = prepare_table(ui, &csv, AppTheme::Light, 1.0_f32);
                let cached = prepare_table(ui, &csv, AppTheme::Light, 1.0_f32);
                assert!(Arc::ptr_eq(&first, &cached));
                assert!(first.offsets[2] - first.offsets[1] > first.offsets[3] - first.offsets[2]);
                let other = prepare_table(ui, &tsv, AppTheme::Light, 1.0_f32);
                assert_eq!(other.widths.len(), 1);
                assert_eq!(first.widths.len(), 2);
                assert!(!Arc::ptr_eq(&first, &other));
            });
        });
    }
}

#[derive(Clone)]
struct TableLayout {
    widths: Vec<f32>,
    offsets: Vec<f32>,
    rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Default)]
struct TableMatches {
    bases: Vec<Vec<usize>>,
    locations: Vec<(usize, usize, usize)>,
}

fn prepare_table(ui: &Ui, data: &CsvTableData, theme: AppTheme, scale: f32) -> Arc<TableLayout> {
    let key = (data.fingerprint, scale.to_bits(), theme as u8);
    let id = egui::Id::new("flash-md-csv-layout");
    if let Some((old, value)) = ui
        .ctx()
        .data(|store| store.get_temp::<((u64, u32, u8), Arc<TableLayout>)>(id))
    {
        if old == key {
            return value;
        }
    }
    let font = FontId::proportional(13.5_f32 * scale);
    let padding = Vec2::new(10.0_f32, 6.0_f32) * scale;
    let source = || std::iter::once(&data.headers).chain(data.rows.iter());
    let widths: Vec<_> = (0..data.total_cols)
        .map(|column| {
            let natural = source()
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
            (natural + 2.0_f32 * padding.x).clamp(96.0_f32 * scale, 320.0_f32 * scale)
        })
        .collect();
    let mut offsets = vec![0.0_f32];
    let rows = source()
        .enumerate()
        .map(|(row, values)| {
            let cells: Vec<_> = widths
                .iter()
                .enumerate()
                .map(|(column, width)| {
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = width - 2.0_f32 * padding.x;
                    job.wrap.break_anywhere = true;
                    job.append(
                        values.get(column).map(String::as_str).unwrap_or(""),
                        0.0_f32,
                        egui::TextFormat {
                            font_id: font.clone(),
                            color: if row == 0 {
                                theme.accent_color()
                            } else {
                                theme.text_primary()
                            },
                            line_height: Some(19.0_f32 * scale),
                            ..Default::default()
                        },
                    );
                    let galley = ui.fonts(|fonts| fonts.layout_job(job.clone()));
                    Cell { job, galley }
                })
                .collect();
            let height = cells
                .iter()
                .map(|cell| cell.galley.size().y)
                .fold(19.0_f32 * scale, f32::max)
                + 2.0_f32 * padding.y;
            offsets.push(offsets.last().copied().unwrap() + height);
            cells
        })
        .collect();
    let value = Arc::new(TableLayout {
        widths,
        offsets,
        rows,
    });
    ui.ctx()
        .data_mut(|store| store.insert_temp(id, (key, value.clone())));
    value
}

fn table_matches(ui: &Ui, table: &TableLayout, fingerprint: u64, query: &str) -> Arc<TableMatches> {
    let key = (fingerprint, query.to_string());
    let id = egui::Id::new("flash-md-csv-matches");
    if let Some((old, value)) = ui
        .ctx()
        .data(|store| store.get_temp::<((u64, String), Arc<TableMatches>)>(id))
    {
        if old == key {
            return value;
        }
    }
    let mut value = TableMatches::default();
    for (row, cells) in table.rows.iter().enumerate() {
        let mut bases = Vec::new();
        for (column, cell) in cells.iter().enumerate() {
            bases.push(value.locations.len());
            for (local, _) in crate::search::find_matches(&cell.job.text, query)
                .iter()
                .enumerate()
            {
                value.locations.push((row, column, local));
            }
        }
        value.bases.push(bases);
    }
    let value = Arc::new(value);
    ui.ctx()
        .data_mut(|store| store.insert_temp(id, (key, value.clone())));
    value
}

/// Allocate the full table, but paint only rows and columns inside the viewport.
#[allow(clippy::too_many_arguments)]
pub fn render_csv_table(
    ui: &mut Ui,
    theme: AppTheme,
    font_scale: f32,
    data: &CsvTableData,
    search_query: &str,
    active_match_index: Option<usize>,
    match_counter: &mut usize,
    search_jump: bool,
) {
    if data.headers.is_empty() && data.rows.is_empty() {
        ui.label("表格內容為空");
        return;
    }
    ui.label(format!(
        "{} 筆資料 · {} 欄",
        data.total_rows, data.total_cols
    ));
    if let Some(error) = &data.error {
        ui.colored_label(theme.accent_color(), format!("CSV 解析失敗：{error}"));
    }
    let table = prepare_table(ui, data, theme, font_scale);
    let matches = table_matches(ui, &table, data.fingerprint, search_query);
    let base = *match_counter;
    *match_counter += matches.locations.len();
    let local_active = active_match_index.and_then(|index| index.checked_sub(base));
    let padding = Vec2::new(10.0_f32, 6.0_f32) * font_scale;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(table.widths.iter().sum(), *table.offsets.last().unwrap()),
        Sense::hover(),
    );
    let clip = ui.clip_rect();
    let start_y = (clip.top() - rect.top()).max(0.0_f32);
    let end_y = clip.bottom() - rect.top();
    let first = table
        .offsets
        .partition_point(|offset| *offset <= start_y)
        .saturating_sub(1);
    let end = table
        .offsets
        .partition_point(|offset| *offset < end_y)
        .min(table.rows.len());
    let colors = match theme {
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
    for row in first..end {
        let row_rect = Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + table.offsets[row]),
            egui::pos2(rect.right(), rect.top() + table.offsets[row + 1]),
        );
        let background = if row == 0 {
            theme.card_bg_color()
        } else if row % 2 == 1 {
            theme.bg_color()
        } else {
            match theme {
                AppTheme::Dark => Color32::from_rgba_unmultiplied(255, 255, 255, 6),
                AppTheme::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 8),
            }
        };
        let border = Stroke::new(0.5_f32, theme.border_color());
        ui.painter()
            .rect_filled(row_rect, Rounding::ZERO, background);
        ui.painter().rect_stroke(row_rect, Rounding::ZERO, border);
        let mut x = row_rect.left();
        for (column, cell) in table.rows[row].iter().enumerate() {
            let cell_rect = Rect::from_min_size(
                egui::pos2(x, row_rect.top()),
                Vec2::new(table.widths[column], row_rect.height()),
            );
            if cell_rect.intersects(clip) {
                let inner = cell_rect.shrink2(padding);
                let galley = if search_query.trim().is_empty() {
                    cell.galley.clone()
                } else {
                    let mut job = cell.job.clone();
                    let mut counter = base + matches.bases[row][column];
                    crate::search::highlight_job(
                        &mut job,
                        search_query,
                        active_match_index,
                        &mut counter,
                        colors.0,
                        colors.1,
                        colors.2,
                        colors.3,
                    );
                    ui.fonts(|fonts| fonts.layout_job(job))
                };
                ui.painter().with_clip_rect(inner.intersect(clip)).galley(
                    inner.min,
                    galley,
                    theme.text_primary(),
                );
            }
            x += table.widths[column];
            if column + 1 < table.widths.len() {
                ui.painter().vline(x, row_rect.y_range(), border);
            }
        }
    }
    if search_jump {
        if let Some(&(row, column, local)) =
            local_active.and_then(|index| matches.locations.get(index))
        {
            let cell = &table.rows[row][column];
            let range = &crate::search::find_matches(&cell.job.text, search_query)[local];
            let cursor = cell
                .galley
                .from_ccursor(CCursor::new(cell.job.text[..range.start].chars().count()));
            let origin = rect.min
                + Vec2::new(
                    table.widths[..column].iter().sum::<f32>() + padding.x,
                    table.offsets[row] + padding.y,
                );
            ui.scroll_to_rect(
                cell.galley
                    .pos_from_cursor(&cursor)
                    .translate(origin.to_vec2()),
                Some(egui::Align::Center),
            );
        }
    }
}
