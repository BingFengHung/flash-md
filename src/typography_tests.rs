use crate::markdown::{render_code_viewer, MarkdownRenderer};
use crate::theme::{setup_system_cjk_fonts, AppTheme};
use egui::{epaint::TextShape, Context, Event, Pos2, RawInput, Rect, Vec2};

fn context(dpi: f32) -> Context {
    let ctx = Context::default();
    setup_system_cjk_fonts(&ctx);
    egui_extras::install_image_loaders(&ctx);
    ctx.set_pixels_per_point(dpi);
    let _ = ctx.run(Default::default(), |_| {});
    assert!((ctx.pixels_per_point() - dpi).abs() < 0.001);
    ctx
}

fn input(time: f64, width: f32, events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 1200.0))),
        time: Some(time),
        events,
        focused: true,
        ..Default::default()
    }
}

fn texts(output: &egui::FullOutput) -> Vec<TextShape> {
    fn visit(shape: &egui::epaint::Shape, result: &mut Vec<TextShape>) {
        match shape {
            egui::epaint::Shape::Text(text) => result.push(text.clone()),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for shape in &output.shapes {
        visit(&shape.shape, &mut result);
    }
    result
}

fn assert_baselines(galley: &egui::Galley, dpi: f32) {
    for row in &galley.rows {
        let Some(first) = row.glyphs.first() else {
            continue;
        };
        for glyph in &row.glyphs {
            assert!(
                (glyph.pos.y - first.pos.y).abs() * dpi <= 1.01,
                "baseline drift {:?} / {:?} at dpi={dpi}",
                (first.chr, first.pos.y),
                (glyph.chr, glyph.pos.y)
            );
        }
    }
}

#[test]
fn typography_code_line_numbers_and_mixed_scripts_share_baselines() {
    let code = "let 中文_12 = \"ABC🙂🚀\";\n// 第二行 comment 42\nfn main() {}";
    for dpi in [1.0_f32, 1.25, 2.0] {
        let ctx = context(dpi);
        for (index, scale) in [0.7_f32, 1.0, 1.35, 1.8].into_iter().enumerate() {
            let output = ctx.run(input(index as f64 + 1.0, 1200.0, vec![]), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_code_viewer(
                        ui,
                        AppTheme::Dark,
                        scale,
                        code,
                        "rs",
                        "中文",
                        Some(0),
                        false,
                    );
                });
            });
            let texts = texts(&output);
            for (index, source) in code.split('\n').enumerate() {
                let text = texts
                    .iter()
                    .find(|text| text.galley.text() == source)
                    .unwrap();
                let number = texts
                    .iter()
                    .find(|text| text.galley.text() == (index + 1).to_string())
                    .unwrap();
                assert_baselines(&text.galley, dpi);
                let baseline = |text: &TextShape| text.pos.y + text.galley.rows[0].glyphs[0].pos.y;
                assert!((baseline(text) - baseline(number)).abs() * dpi < 0.01);
                assert_eq!(
                    text.galley.rows[0].rect.height(),
                    number.galley.rows[0].rect.height()
                );
            }
        }
    }
    println!("TYPOGRAPHY code gutters: exact painted baseline equality at 3 DPIs / 4 font scales");
}

#[test]
fn typography_markdown_and_csv_cells_align_short_and_wrapped_first_lines() {
    let long = "中文 ABC 123 🙂 ".repeat(20);
    let markdown = format!("| CJK | Latin | Digits | Emoji |\n| :--- | :---: | ---: | --- |\n| 中文A | English | 1234 | 🙂🚀 |\n| {long} | Next | 7654 | 🚀 |");
    let csv = crate::parsers::parse_csv_or_tsv(
        &format!("CJK,Latin,Digits,Emoji\n中文A,English,1234,🙂🚀\n{long},Next,7654,🚀"),
        ',',
    );
    for dpi in [1.0_f32, 1.25, 2.0] {
        let ctx = context(dpi);
        for (index, scale) in [0.7_f32, 1.0, 1.35, 1.8].into_iter().enumerate() {
            for (variant, theme) in [AppTheme::Dark, AppTheme::Light].into_iter().enumerate() {
                for format in 0..2 {
                    let output = ctx.run(
                        input(
                            (index * 4 + variant * 2 + format + 1) as f64,
                            1800.0,
                            vec![],
                        ),
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                if format == 0 {
                                    MarkdownRenderer::new(theme, scale, "", None, None, None)
                                        .render(ui, &markdown);
                                } else {
                                    crate::views::data_table::render_csv_table(
                                        ui, theme, scale, &csv, "", None, &mut 0, false,
                                    );
                                }
                            });
                        },
                    );
                    let texts = texts(&output);
                    for labels in [
                        ["中文A", "English", "1234", "🙂🚀"],
                        [long.trim(), "Next", "7654", "🚀"],
                    ] {
                        let cells: Vec<_> = labels
                            .iter()
                            .map(|label| {
                                texts
                                    .iter()
                                    .find(|text| text.galley.text().trim() == *label)
                                    .unwrap()
                            })
                            .collect();
                        let reference = cells[0].pos.y + cells[0].galley.rows[0].glyphs[0].pos.y;
                        for cell in cells {
                            assert_baselines(&cell.galley, dpi);
                            let baseline = cell.pos.y + cell.galley.rows[0].glyphs[0].pos.y;
                            assert!(
                                (baseline - reference).abs() * dpi <= 1.01,
                                "table format={format}, dpi={dpi}, scale={scale}: {}",
                                cell.galley.text()
                            );
                        }
                    }
                }
            }
        }
    }
    println!(
        "TYPOGRAPHY tables: real first-line baselines checked for short/wrapped cells, both themes"
    );
}

#[test]
fn typography_editor_rows_and_wrapped_color_emoji_links_keep_one_text_layout() {
    let paragraph = format!(
        "中文 ABC 123 ⚡ [內部連結](#target) {} 尾端🚀",
        "寬度測試 words 42 ".repeat(10)
    );
    let markdown = format!("# 中文 Title 123 ⚡\n\n{paragraph}\n\n## Target");
    let editor = format!("# 中文 Title 123 🙂\n> 引用 ABC 42 🚀\n```rust\nlet 中文 = 123; // 🙂\n```\n- 中文 ABC 42 🙂\n{paragraph}");
    for dpi in [1.0_f32, 1.25, 2.0] {
        let ctx = context(dpi);
        for (index, scale) in [0.7_f32, 1.0, 1.35, 1.8].into_iter().enumerate() {
            let output = ctx.run(input(index as f64 + 1.0, 360.0, vec![]), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let job = crate::views::editor::highlight_markdown_for_editor(
                        &editor,
                        AppTheme::Dark,
                        scale,
                        330.0,
                    );
                    let galley = ui.fonts(|fonts| fonts.layout_job(job));
                    assert_baselines(&galley, dpi);
                    MarkdownRenderer::new(AppTheme::Light, scale, "", None, None, None)
                        .render(ui, &markdown);
                });
            });
            let texts = texts(&output);
            let paragraph = texts
                .iter()
                .find(|text| text.galley.text().starts_with("中文 ABC 123 ⚡ 內部連結"))
                .unwrap();
            assert!(paragraph.galley.rows.len() > 1);
            assert!(paragraph.galley.text().ends_with("尾端🚀"));
            assert_baselines(&paragraph.galley, dpi);
            let heading = texts
                .iter()
                .find(|text| text.galley.text() == "中文 Title 123 ⚡")
                .unwrap();
            assert_baselines(&heading.galley, dpi);
        }
    }
    println!("TYPOGRAPHY editor / headings / narrow linked Emoji paragraphs: one baseline per physical row");
}

#[test]
fn typography_inline_link_click_and_cross_span_search_survive_unified_layout() {
    let ctx = context(1.25);
    let document = "前綴 [連結](#target) 後綴 ⚡ 42\n\n## Target";
    let mut rendered = None;
    let mut render = |events, time| {
        ctx.run(input(time, 700.0, events), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rendered = Some(
                    MarkdownRenderer::new(
                        AppTheme::Dark,
                        1.35,
                        "前綴 連結 後綴",
                        Some(0),
                        None,
                        None,
                    )
                    .render(ui, document),
                );
            });
        })
    };
    let output = render(vec![], 1.0);
    let text = texts(&output)
        .into_iter()
        .find(|text| text.galley.text().starts_with("前綴 連結 後綴"))
        .unwrap();
    let glyph = text.galley.rows[0]
        .glyphs
        .iter()
        .find(|glyph| glyph.chr == '連')
        .unwrap();
    let point = text.pos
        + Vec2::new(
            glyph.pos.x + glyph.advance_width * 0.5,
            text.galley.rows[0].rect.center().y,
        );
    render(vec![Event::PointerMoved(point)], 1.1);
    for (index, pressed) in [true, false].into_iter().enumerate() {
        render(
            vec![Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            1.2 + index as f64 * 0.1,
        );
    }
    let rendered = rendered.unwrap();
    assert_eq!(rendered.match_count, 1);
    assert_eq!(rendered.clicked_anchor.as_deref(), Some("target"));
    println!("TYPOGRAPHY unified inline layout: real anchor click and cross-span search passed");
}
