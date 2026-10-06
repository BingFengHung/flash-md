use crate::parsers::CsvTableData;
use crate::theme::AppTheme;
use egui::text::{CCursor, LayoutJob};
use egui::{Color32, FontId, Rect, Rounding, Sense, Stroke, Ui, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

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

    #[test]
    fn lazy_cells_keep_the_same_widths_and_row_heights_as_full_layout() {
        let source = format!("key,note,extra\nAVATAR,short,\nwide,\"{}\",tail\nJoe,\"first\nsecond\nthird\",end\nAmy", "AV 中文 Wa emoji 😀 long text ".repeat(40));
        let data = crate::parsers::parse_csv_or_tsv(&source, ',');
        let ctx = egui::Context::default();
        crate::theme::setup_system_cjk_fonts(&ctx);
        let mut previous = None;
        for (pixels, scale) in [(1.0, 1.0), (1.25, 1.0), (1.25, 1.35)] {
            ctx.set_pixels_per_point(pixels);
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let table = prepare_table(ui, &data, AppTheme::Dark, scale);
                    if let Some(old) = previous.take() {
                        assert!(!Arc::ptr_eq(&old, &table));
                    }
                    for column in 0..data.total_cols {
                        let width = (0..=data.rows.len())
                            .map(|row| {
                                ui.fonts(|fonts| {
                                    fonts.layout_no_wrap(
                                        cell_text(&data, row, column).into(),
                                        FontId::proportional(13.5 * scale),
                                        Color32::WHITE,
                                    )
                                })
                                .size()
                                .x
                            })
                            .fold(0.0, f32::max);
                        let expected = (width + 20.0 * scale).clamp(96.0 * scale, 320.0 * scale);
                        assert!((table.widths[column] - expected).abs() < 0.01);
                    }
                    for row in 0..=data.rows.len() {
                        let height = (0..data.total_cols)
                            .map(|column| {
                                let galley = ui.fonts(|fonts| {
                                    fonts.layout_job(cell_job(
                                        cell_text(&data, row, column),
                                        row,
                                        table.widths[column],
                                        AppTheme::Dark,
                                        scale,
                                    ))
                                });
                                assert!(
                                    (cell_galley(
                                        ui,
                                        &table,
                                        &data,
                                        row,
                                        column,
                                        AppTheme::Dark,
                                        scale
                                    )
                                    .size()
                                    .y - galley.size().y)
                                        .abs()
                                        < 0.01
                                );
                                galley.size().y
                            })
                            .fold(19.0 * scale, f32::max)
                            + 12.0 * scale;
                        assert!(
                            (table.offsets[row + 1] - table.offsets[row] - height).abs() < 0.01
                        );
                    }
                    assert!(table.wrapped.len() < data.total_cols * (data.rows.len() + 1));
                    previous = Some(table);
                });
            });
        }
    }
}

#[derive(Clone)]
struct TableLayout {
    widths: Vec<f32>,
    offsets: Vec<f32>,
    wrapped: HashMap<(usize, usize), Arc<egui::Galley>>,
}

#[derive(Clone, Default)]
struct TableMatches {
    bases: Vec<Vec<usize>>,
    locations: Vec<(usize, usize, usize)>,
}

fn prepare_table(ui: &Ui, data: &CsvTableData, theme: AppTheme, scale: f32) -> Arc<TableLayout> {
    let key = (
        data.fingerprint,
        scale.to_bits(),
        theme as u8,
        ui.ctx().pixels_per_point().to_bits(),
    );
    let id = egui::Id::new("flash-md-csv-layout");
    if let Some((old, value)) = ui
        .ctx()
        .data(|store| store.get_temp::<((u64, u32, u8, u32), Arc<TableLayout>)>(id))
    {
        if old == key {
            return value;
        }
    }
    let font = FontId::proportional(13.5_f32 * scale);
    let padding = Vec2::new(10.0_f32, 6.0_f32) * scale;
    let source = || std::iter::once(&data.headers).chain(data.rows.iter());
    let mut metrics = crate::text_metrics::TextMetrics::new(ui, font);
    let mut widths = vec![0.0_f32; data.total_cols];
    let natural: Vec<Vec<f32>> = source()
        .map(|row| {
            widths
                .iter_mut()
                .enumerate()
                .map(|(column, max)| {
                    let value =
                        metrics.width(row.get(column).map(String::as_str).unwrap_or(""), false);
                    *max = max.max(value.round());
                    value
                })
                .collect()
        })
        .collect();
    for width in &mut widths {
        *width = (*width + 2.0 * padding.x).clamp(96.0 * scale, 320.0 * scale);
    }
    let single_height = ui
        .fonts(|fonts| fonts.layout_job(cell_job("X", 0, widths[0], theme, scale)))
        .size()
        .y;
    let mut offsets = vec![0.0_f32];
    let mut wrapped = HashMap::new();
    for (row, values) in source().enumerate() {
        let mut height = single_height.max(19.0 * scale);
        for (column, width) in widths.iter().enumerate() {
            let text = values.get(column).map(String::as_str).unwrap_or("");
            if text.contains('\n') || natural[row][column] > width - 2.0 * padding.x {
                let galley =
                    ui.fonts(|fonts| fonts.layout_job(cell_job(text, row, *width, theme, scale)));
                height = height.max(galley.size().y);
                wrapped.insert((row, column), galley);
            }
        }
        offsets.push(offsets.last().copied().unwrap() + height + 2.0 * padding.y);
    }
    let value = Arc::new(TableLayout {
        widths,
        offsets,
        wrapped,
    });
    ui.ctx()
        .data_mut(|store| store.insert_temp(id, (key, value.clone())));
    value
}

fn cell_job(text: &str, row: usize, width: f32, theme: AppTheme, scale: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width - 20.0 * scale;
    job.wrap.break_anywhere = true;
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(13.5 * scale),
            color: if row == 0 {
                theme.accent_color()
            } else {
                theme.text_primary()
            },
            line_height: Some(19.0 * scale),
            ..Default::default()
        },
    );
    job
}

fn cell_text(data: &CsvTableData, row: usize, column: usize) -> &str {
    if row == 0 {
        &data.headers
    } else {
        &data.rows[row - 1]
    }
    .get(column)
    .map(String::as_str)
    .unwrap_or("")
}

fn cell_galley(
    ui: &Ui,
    table: &TableLayout,
    data: &CsvTableData,
    row: usize,
    column: usize,
    theme: AppTheme,
    scale: f32,
) -> Arc<egui::Galley> {
    table
        .wrapped
        .get(&(row, column))
        .cloned()
        .unwrap_or_else(|| {
            ui.fonts(|fonts| {
                fonts.layout_job(cell_job(
                    cell_text(data, row, column),
                    row,
                    table.widths[column],
                    theme,
                    scale,
                ))
            })
        })
}

fn table_matches(ui: &Ui, data: &CsvTableData, query: &str) -> Arc<TableMatches> {
    let key = (data.fingerprint, query.to_string());
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
    for cells in std::iter::once(&data.headers).chain(data.rows.iter()) {
        let mut bases = Vec::new();
        let row = value.bases.len();
        for (column, cell) in cells.iter().enumerate() {
            bases.push(value.locations.len());
            for (local, _) in crate::search::find_matches(cell, query).iter().enumerate() {
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
    let matches = table_matches(ui, data, search_query);
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
        .min(data.rows.len() + 1);
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
        for column in 0..table.widths.len() {
            let cell_rect = Rect::from_min_size(
                egui::pos2(x, row_rect.top()),
                Vec2::new(table.widths[column], row_rect.height()),
            );
            if cell_rect.intersects(clip) {
                let inner = cell_rect.shrink2(padding);
                let galley = if search_query.trim().is_empty() {
                    cell_galley(ui, &table, data, row, column, theme, font_scale)
                } else {
                    let mut job = cell_job(
                        cell_text(data, row, column),
                        row,
                        table.widths[column],
                        theme,
                        font_scale,
                    );
                    let mut counter = base
                        + matches.bases[row]
                            .get(column)
                            .copied()
                            .unwrap_or(matches.locations.len());
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
            let text = cell_text(data, row, column);
            let range = &crate::search::find_matches(text, search_query)[local];
            let galley = cell_galley(ui, &table, data, row, column, theme, font_scale);
            let cursor = galley.from_ccursor(CCursor::new(text[..range.start].chars().count()));
            let origin = rect.min
                + Vec2::new(
                    table.widths[..column].iter().sum::<f32>() + padding.x,
                    table.offsets[row] + padding.y,
                );
            ui.scroll_to_rect(
                galley.pos_from_cursor(&cursor).translate(origin.to_vec2()),
                Some(egui::Align::Center),
            );
        }
    }
}
