use super::*;
use egui::{epaint::Shape, Rect};

fn input(frame: usize, width: f32, height: f32) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(width, height),
        )),
        time: Some(frame as f64 / 60.0_f64),
        ..Default::default()
    }
}

fn text_rects(output: &egui::FullOutput) -> Vec<(String, Rect)> {
    fn collect(shape: &Shape, texts: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Text(text) => texts.push((
                text.galley.text().to_string(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, texts);
                }
            }
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut texts);
    }
    texts
}

#[test]
fn markdown_table_columns_align_and_wrapped_rows_do_not_overlap() {
    let long = "long content 中文 ".repeat(24);
    let document = format!(
        "| Left column | Centered | Right column |\n| :--- | :---: | ---: |\n\
         | Left short | mid | 7 |\n| {long} | C | 123 |\n| Next row | center | 9 |"
    );
    for scale in [0.8_f32, 1.0_f32, 1.6_f32] {
        let ctx = egui::Context::default();
        let output = ctx.run(input(0, 1200.0_f32, 1100.0_f32), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                MarkdownRenderer::new(AppTheme::Dark, scale, "", None, None, None)
                    .render(ui, &document);
            });
        });
        let texts = text_rects(&output);
        let rect = |label: &str| texts.iter().find(|(text, _)| text == label).unwrap().1;
        let left = rect("Left short");
        let middle = rect("mid");
        let right = rect("7");
        assert!(left.right() < middle.left());
        assert!(middle.right() < right.left());
        assert!((rect("Left column").left() - left.left()).abs() < 1.0_f32);
        assert!((rect("Centered").center().x - middle.center().x).abs() < 1.0_f32);
        assert!((rect("Right column").right() - right.right()).abs() < 1.0_f32);
        let wrapped = rect(long.trim());
        assert!(wrapped.width() <= 300.0_f32 * scale + 1.0_f32);
        assert!(wrapped.height() > left.height() * 4.0_f32);
        let next = rect("Next row");
        assert!(next.top() > wrapped.bottom());
        assert!((next.top() - rect("center").top()).abs() < 1.0_f32);
        assert!((next.top() - rect("9").top()).abs() < 1.0_f32);
    }
}

#[test]
fn narrow_markdown_tables_keep_readable_widths_on_the_first_frame() {
    let ctx = egui::Context::default();
    let output = ctx.run(input(0, 180.0_f32, 400.0_f32), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            MarkdownRenderer::new(AppTheme::Dark, 1.0_f32, "", None, None, None)
                .render(ui, "| A | B | C |\n| --- | --- | --- |\n| x | y | z |");
        });
    });
    let header = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::Rect(rect) if rect.fill == AppTheme::Dark.card_bg_color() => Some(rect.rect),
            _ => None,
        })
        .unwrap();
    assert!(header.width() >= 288.0_f32);
    assert!(header.width() > 180.0_f32);
    assert!(header.is_finite());
    let texts = text_rects(&output);
    let first = texts.iter().find(|(text, _)| text == "A").unwrap().1;
    let second = texts.iter().find(|(text, _)| text == "B").unwrap().1;
    assert!(second.left() - first.left() >= 95.0_f32);
}

#[test]
fn outline_targets_scroll_past_nested_tables_to_duplicate_and_explicit_headings() {
    let table = "| Name | Value |\n| --- | --- |\n| entry | value |\n\n";
    for (heading, anchor) in [
        ("## Repeat", "repeat-1"),
        ("## 自訂 {#exact-ID}", "exact-ID"),
    ] {
        let document = format!(
            "## Repeat\n\n{table}{}\n{heading}\n\n{table}{}",
            "short paragraph\n\n".repeat(30),
            "trailing paragraph\n\n".repeat(30),
        );
        let toc = crate::parsers::extract_markdown_toc(&document);
        assert_eq!(toc.last().unwrap().anchor, anchor);
        let ctx = egui::Context::default();
        for frame in 0..3 {
            let mut offset = 0.0_f32;
            let mut viewport_top = 0.0_f32;
            let output = ctx.run(input(frame, 520.0_f32, 320.0_f32), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let scroll = egui::ScrollArea::vertical()
                        .id_salt("outline-regression")
                        .animated(false)
                        .show(ui, |ui| {
                            let target = (frame == 0).then_some(anchor);
                            let rendered = MarkdownRenderer::new(
                                AppTheme::Dark,
                                1.0_f32,
                                "",
                                None,
                                target,
                                None,
                            )
                            .render(ui, &document);
                            assert_eq!(rendered.anchor_found, frame == 0);
                        });
                    offset = scroll.state.offset.y;
                    viewport_top = scroll.inner_rect.top();
                });
            });
            assert!(
                offset > 400.0_f32,
                "outline did not scroll on frame {frame}"
            );
            if frame == 2 {
                let title = &toc.last().unwrap().title;
                let texts = text_rects(&output);
                let heading = texts.iter().find(|(text, _)| text == title).unwrap().1;
                assert!((heading.top() - viewport_top).abs() <= 16.0_f32);
            }
        }
    }
}

#[test]
fn markdown_table_search_keeps_global_match_counts_on_cached_frames() {
    let ctx = egui::Context::default();
    let document = "needle\n\n| needle | Other |\n| --- | --- |\n| needle | needle |";
    for frame in 0..2 {
        let _ = ctx.run(input(frame, 520.0_f32, 320.0_f32), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rendered =
                    MarkdownRenderer::new(AppTheme::Dark, 1.0_f32, "needle", Some(2), None, None)
                        .render(ui, document);
                assert_eq!(rendered.match_count, 4);
            });
        });
    }
}

#[test]
fn headings_with_images_and_reserved_explicit_ids_scroll_to_the_correct_block() {
    for (heading, anchor) in [
        ("## Title ![Badge](missing.png)", "title-badge"),
        ("## Target\n\n## Manual {#target}", "target-1"),
    ] {
        let document = format!(
            "# Intro\n\n{}{heading}\n\n{}",
            "Paragraph.\n\n".repeat(45),
            "After.\n\n".repeat(45)
        );
        let ctx = egui::Context::default();
        for frame in 0..3 {
            let mut offset = 0.0_f32;
            let _ = ctx.run(input(frame, 520.0_f32, 320.0_f32), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let scroll = egui::ScrollArea::vertical()
                        .id_salt("image-heading")
                        .animated(false)
                        .show(ui, |ui| {
                            let rendered = MarkdownRenderer::new(
                                AppTheme::Dark,
                                1.0_f32,
                                "",
                                None,
                                (frame == 0).then_some(anchor),
                                None,
                            )
                            .render(ui, &document);
                            assert_eq!(rendered.anchor_found, frame == 0);
                        });
                    offset = scroll.state.offset.y;
                });
            });
            assert!(
                offset > 600.0_f32,
                "image/explicit heading failed to scroll"
            );
        }
    }
}

#[test]
fn searching_a_wide_markdown_table_reveals_the_last_column_and_row() {
    let document = format!("# Intro\n\n{}| A | B | C | D | E | F | G | H |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n| one | two | three | four | five | six | seven | NEEDLE |\n\n{}", "Paragraph.\n\n".repeat(45), "After.\n\n".repeat(45));
    let ctx = egui::Context::default();
    for frame in 0..3 {
        let mut offset = 0.0_f32;
        let output = ctx.run(input(frame, 520.0_f32, 320.0_f32), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("wide-table-search")
                    .animated(false)
                    .show(ui, |ui| {
                        let mut renderer = MarkdownRenderer::new(
                            AppTheme::Dark,
                            1.0_f32,
                            "NEEDLE",
                            Some(0),
                            None,
                            None,
                        );
                        renderer.search_jump = frame == 0;
                        let rendered = renderer.render(ui, &document);
                        assert_eq!(rendered.match_count, 1);
                    });
                offset = scroll.state.offset.y;
            });
        });
        assert!(
            offset > 600.0_f32,
            "table search failed to scroll vertically"
        );
        if frame == 2 {
            let shape = output.shapes.iter().find(|shape| matches!(&shape.shape, Shape::Text(text) if text.galley.text() == "NEEDLE")).unwrap();
            let Shape::Text(text) = &shape.shape else {
                unreachable!()
            };
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            assert!(
                shape.clip_rect.contains_rect(rect),
                "table search is outside the visible cell: {rect:?} / {:?}",
                shape.clip_rect
            );
        }
    }
}
