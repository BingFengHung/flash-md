use super::tests::{app, settle};
use super::*;
use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect};

fn model() -> (MdPreviewApp, Context) {
    let app = app();
    let ctx = app.ctx_holder.lock().unwrap().clone().unwrap();
    egui_extras::install_image_loaders(&ctx);
    (app, ctx)
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn frame(
    app: &mut MdPreviewApp,
    ctx: &Context,
    time: f64,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> egui::FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(940.0_f32, 700.0_f32),
            )),
            time: Some(time),
            events,
            modifiers,
            focused: true,
            ..Default::default()
        },
        |ctx| app.update_ui(ctx),
    )
}

fn content(app: &mut MdPreviewApp, value: String, path: &str, mode: ViewMode) {
    app.content = value;
    app.original_content = app.content.clone();
    app.current_file = Some(path.into());
    app.document_kind = DocumentKind::Text;
    app.view_mode = mode;
    app.invalidate_content();
    app.reset_scroll_to_top = true;
    app.status_toast = None;
}

fn texts(output: &egui::FullOutput) -> Vec<(String, Rect, Rect)> {
    fn visit(shape: &egui::epaint::Shape, clip: Rect, result: &mut Vec<(String, Rect, Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => result.push((
                text.galley.text().to_string(),
                Rect::from_min_size(text.pos, text.galley.size()),
                clip,
            )),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, clip, result);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for shape in &output.shapes {
        visit(&shape.shape, shape.clip_rect, &mut result);
    }
    result
}

#[test]
fn ctrl_p_only_pins_the_window_and_settings_block_document_navigation() {
    let (mut app, ctx) = model();
    content(
        &mut app,
        "# Title\n\n---\n\n# Next".to_string(),
        "fixture.md",
        ViewMode::Markdown,
    );
    let output = frame(
        &mut app,
        &ctx,
        0.0,
        vec![key(Key::P, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    assert!(app.always_on_top);
    assert!(!app.is_slides_mode);
    assert!(!output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::Fullscreen(_))));
    app.settings_open = true;
    let path = app.current_file.clone();
    frame(
        &mut app,
        &ctx,
        0.1,
        vec![key(Key::ArrowRight, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(app.current_file, path);
    assert!(app.loading_request.is_none());
}

#[test]
fn all_supported_modes_cycle_back_without_getting_stuck_in_csv_or_empty_images() {
    for (extension, kind) in [
        ("md", DocumentKind::Text),
        ("csv", DocumentKind::Text),
        ("tsv", DocumentKind::Text),
        ("rs", DocumentKind::Text),
        ("txt", DocumentKind::Text),
        ("svg", DocumentKind::Svg),
        ("png", DocumentKind::Image),
        ("pdf", DocumentKind::Pdf),
    ] {
        let mut app = app();
        app.current_file = Some(format!("fixture.{extension}").into());
        app.document_kind = kind;
        let modes = super::shortcuts::preview_modes(kind, extension);
        app.view_mode = modes[0].clone();
        for expected in modes.iter().skip(1).chain(std::iter::once(&modes[0])) {
            app.cycle_view_mode();
            assert_eq!(&app.view_mode, expected, "{extension}");
        }
    }
}

#[test]
fn held_keyboard_scrolling_has_the_same_speed_at_30_and_120_fps() {
    fn scroll(fps: usize) -> f32 {
        let (mut app, ctx) = model();
        for index in 0..=fps {
            let events = if index == 0 {
                vec![key(Key::ArrowDown, Modifiers::NONE)]
            } else {
                Vec::new()
            };
            let _ = ctx.run(
                RawInput {
                    time: Some(1.0_f64 + index as f64 / fps as f64),
                    events,
                    ..Default::default()
                },
                |ctx| app.handle_shortcuts(ctx),
            );
        }
        app.current_scroll_offset
    }
    let slow = scroll(30);
    let fast = scroll(120);
    assert!(
        (slow - fast).abs() < 30.0_f32,
        "30 fps: {slow}, 120 fps: {fast}"
    );
    assert!((530.0_f32..650.0_f32).contains(&fast));
}

#[test]
fn image_zoom_shortcuts_change_the_image_and_keep_text_scale_unchanged() {
    let (mut app, ctx) = model();
    app.view_mode = ViewMode::Image {
        format: "png".to_string(),
    };
    let _ = ctx.run(
        RawInput {
            events: vec![key(Key::Plus, Modifiers::COMMAND)],
            modifiers: Modifiers::COMMAND,
            ..Default::default()
        },
        |ctx| app.handle_shortcuts(ctx),
    );
    assert!(app.image_zoom > 1.0_f32);
    assert!(!app.image_fit_mode);
    assert_eq!(app.font_scale, 1.0_f32);
    let _ = ctx.run(
        RawInput {
            events: vec![key(Key::Num0, Modifiers::COMMAND)],
            modifiers: Modifiers::COMMAND,
            ..Default::default()
        },
        |ctx| app.handle_shortcuts(ctx),
    );
    assert_eq!(app.image_zoom, 1.0_f32);
}

#[test]
fn normal_chinese_text_and_paste_keep_enter_while_ime_confirmation_consumes_it_once() {
    let (mut app, ctx) = model();
    app.is_editing = true;
    for text in [
        Event::Text("中文".to_string()),
        Event::Paste("中文\n第二行".to_string()),
    ] {
        let _ = ctx.run(
            RawInput {
                events: vec![text, key(Key::Enter, Modifiers::NONE)],
                ..Default::default()
            },
            |ctx| {
                app.handle_ime_input(ctx);
                assert!(ctx.input(|input| input.key_pressed(Key::Enter)));
            },
        );
        let _ = ctx.run(
            RawInput {
                events: vec![Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| {},
        );
    }
    let _ = ctx.run(
        RawInput {
            events: vec![
                Event::Ime(egui::ImeEvent::Commit("中文".to_string())),
                key(Key::Enter, Modifiers::NONE),
            ],
            ..Default::default()
        },
        |ctx| {
            app.handle_ime_input(ctx);
            assert!(!ctx.input(|input| input.key_pressed(Key::Enter)));
            assert!(ctx.input(|input| input
                .events
                .iter()
                .any(|event| matches!(event, Event::Ime(egui::ImeEvent::Commit(_))))));
        },
    );
    let _ = ctx.run(
        RawInput {
            events: vec![key(Key::Enter, Modifiers::NONE)],
            ..Default::default()
        },
        |ctx| {
            app.handle_ime_input(ctx);
            assert!(ctx.input(|input| input.key_pressed(Key::Enter)));
        },
    );
}

#[test]
fn json_format_autosaves_without_more_input_and_toasts_expire() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.json");
    fs::write(&path, "{\"a\":1}").unwrap();
    let (mut app, ctx) = model();
    app.open_document(&path);
    settle(&mut app);
    app.config.save_mode = SaveMode::AutoDebounce;
    app.format_json_content();
    app.last_edit_instant = Some(std::time::Instant::now() - Duration::from_millis(500));
    let output = frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    assert!(app.is_modified);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT].repaint_delay <= Duration::from_millis(300)
    );
    app.last_edit_instant = Some(std::time::Instant::now() - Duration::from_millis(850));
    frame(&mut app, &ctx, 0.9, Vec::new(), Modifiers::NONE);
    assert!(!app.is_modified);
    assert_eq!(fs::read_to_string(&path).unwrap(), app.content);
    app.status_toast = Some((
        "expired".to_string(),
        std::time::Instant::now() - Duration::from_secs(5),
    ));
    app.process_timers(&ctx);
    assert!(app.status_toast.is_none());
}

#[test]
fn clicking_the_actual_toc_and_searching_before_tables_moves_the_document() {
    let (mut app, ctx) = model();
    let markdown = format!(
        "# Intro\n\n{}\n\n## Target\n\nNEEDLE\n\n| Name | Value |\n| --- | --- |\n| A | B |\n\n{}",
        "Paragraph.\n\n".repeat(70),
        "After.\n\n".repeat(50)
    );
    content(&mut app, markdown, "fixture.md", ViewMode::Markdown);
    app.toc_open = true;
    frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    let output = frame(&mut app, &ctx, 0.05, Vec::new(), Modifiers::NONE);
    let position = texts(&output)
        .into_iter()
        .find(|(text, _, clip)| text == "Target" && clip.right() < 400.0_f32)
        .unwrap()
        .1
        .center();
    for (time, pressed) in [(0.1, true), (0.15, false)] {
        frame(
            &mut app,
            &ctx,
            time,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            Modifiers::NONE,
        );
    }
    assert!(app.current_scroll_offset > 500.0_f32);
    assert!(app.target_anchor.is_none());
    app.target_scroll_offset = Some(0.0_f32);
    frame(&mut app, &ctx, 0.2, Vec::new(), Modifiers::NONE);
    app.search_open = true;
    app.search_query = "NEEDLE".to_string();
    app.search_jump_requested = true;
    frame(&mut app, &ctx, 0.3, Vec::new(), Modifiers::NONE);
    assert_eq!(app.search_match_count, 1);
    assert!(app.current_scroll_offset > 500.0_f32);
}

#[test]
fn slides_use_navigation_keys_without_switching_files_and_end_reaches_the_bottom() {
    let (mut app, ctx) = model();
    content(
        &mut app,
        format!("# First\n\n{}\n\n---\n\n# Second", "Line\n\n".repeat(100)),
        "fixture.md",
        ViewMode::Markdown,
    );
    frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    frame(
        &mut app,
        &ctx,
        0.1,
        vec![key(Key::End, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(app.current_scroll_offset > 1000.0_f32);
    assert!((app.current_scroll_offset - app.max_scroll_offset).abs() < 1.0_f32);
    frame(
        &mut app,
        &ctx,
        0.2,
        vec![key(Key::F5, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(app.is_slides_mode);
    frame(
        &mut app,
        &ctx,
        0.3,
        vec![key(Key::ArrowRight, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(app.current_slide_index, 1);
    assert!(app.loading_request.is_none());
    frame(
        &mut app,
        &ctx,
        0.4,
        vec![key(Key::Escape, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(!app.is_slides_mode);
    assert!(!app.is_slides_fullscreen);
}

#[test]
fn closing_or_editing_during_a_background_load_never_reopens_or_overwrites_the_draft() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.md");
    fs::write(&path, "# New").unwrap();
    let (mut app, _) = model();
    app.open_document(&path);
    app.execute_action(PendingAction::Close);
    app.poll_document_loads();
    assert!(!app.visible);
    assert!(app.current_file.is_none());
    app.open_document(&path);
    app.content = "draft".to_string();
    app.is_modified = true;
    settle(&mut app);
    assert_eq!(app.content, "draft");
    assert!(app.is_modified);
    assert!(app.current_file.is_none());
}

#[test]
fn search_enter_and_shift_enter_keep_focus_and_scroll_between_real_matches() {
    let (mut app, ctx) = model();
    content(
        &mut app,
        format!(
            "# Intro\n\nNEEDLE\n\n{}NEEDLE\n\n{}",
            "Paragraph.\n\n".repeat(70),
            "After.\n\n".repeat(50)
        ),
        "fixture.md",
        ViewMode::Markdown,
    );
    app.search_query = "NEEDLE".to_string();
    app.open_search();
    frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    frame(&mut app, &ctx, 0.05, Vec::new(), Modifiers::NONE);
    assert_eq!(app.search_match_count, 2);
    assert!(ctx.wants_keyboard_input());
    frame(
        &mut app,
        &ctx,
        0.1,
        vec![key(Key::Enter, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(app.search_match_index, 1);
    assert!(app.current_scroll_offset > 800.0_f32);
    assert!(ctx.wants_keyboard_input());
    let mut release = key(Key::Enter, Modifiers::NONE);
    if let Event::Key { pressed, .. } = &mut release {
        *pressed = false;
    }
    frame(&mut app, &ctx, 0.15, vec![release], Modifiers::NONE);
    frame(
        &mut app,
        &ctx,
        0.2,
        vec![key(Key::Enter, Modifiers::SHIFT)],
        Modifiers::SHIFT,
    );
    assert_eq!(app.search_match_index, 0);
    assert!(app.current_scroll_offset < 200.0_f32);
    assert!(ctx.wants_keyboard_input());
}

#[test]
fn pending_file_loads_do_not_steal_search_arrows_or_settings_escape() {
    let (mut app, ctx) = model();
    content(
        &mut app,
        "# Current".to_string(),
        "fixture.md",
        ViewMode::Markdown,
    );
    app.search_query = "Current".to_string();
    app.open_search();
    frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    frame(&mut app, &ctx, 0.05, Vec::new(), Modifiers::NONE);
    app.siblings = vec!["a.md".into(), "b.md".into()];
    app.loading_request = Some(LoadRequest {
        id: u64::MAX,
        path: "a.md".into(),
        reset_view: true,
        revision: app.content_revision,
        scan_directory: false,
    });
    frame(
        &mut app,
        &ctx,
        0.1,
        vec![key(Key::ArrowRight, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(app.loading_request.as_ref().unwrap().id, u64::MAX);
    app.search_open = false;
    app.settings_open = true;
    frame(
        &mut app,
        &ctx,
        0.2,
        vec![key(Key::Escape, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(!app.settings_open);
    assert_eq!(app.loading_request.as_ref().unwrap().id, u64::MAX);
    app.cancel_document_load();
}

#[test]
fn leaving_slides_for_editing_mindmap_search_or_outline_clears_fullscreen_state() {
    for (shortcut, modifiers) in [
        (Key::E, Modifiers::COMMAND),
        (Key::M, Modifiers::COMMAND),
        (Key::F6, Modifiers::NONE),
        (Key::F, Modifiers::COMMAND),
        (Key::T, Modifiers::COMMAND),
    ] {
        let (mut app, ctx) = model();
        content(
            &mut app,
            "# First\n\n---\n\n# Second".to_string(),
            "fixture.md",
            ViewMode::Markdown,
        );
        frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
        frame(
            &mut app,
            &ctx,
            0.1,
            vec![key(Key::F5, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert!(app.is_slides_mode && app.is_slides_fullscreen);
        let output = frame(
            &mut app,
            &ctx,
            0.2,
            vec![key(shortcut, modifiers)],
            modifiers,
        );
        assert!(
            !app.is_slides_mode && !app.is_slides_fullscreen,
            "{shortcut:?}"
        );
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Fullscreen(false))),
            "{shortcut:?}"
        );
        match shortcut {
            Key::E => assert!(app.is_editing),
            Key::M | Key::F6 => assert!(matches!(app.view_mode, ViewMode::Mindmap)),
            Key::F => assert!(app.search_open && ctx.wants_keyboard_input()),
            Key::T => assert!(app.toc_open),
            _ => unreachable!(),
        }
    }
}

#[test]
fn outline_is_hidden_in_editor_and_slides_and_search_preserves_the_draft() {
    let (mut app, ctx) = model();
    content(
        &mut app,
        "# Original\n\nBody".to_string(),
        "fixture.md",
        ViewMode::Markdown,
    );
    app.toc_open = true;
    app.toggle_edit_mode();
    let output = frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    assert!(!texts(&output)
        .iter()
        .any(|(text, _, _)| text.contains("目錄大綱")));
    app.content = "# Draft\n\nNEEDLE".to_string();
    app.is_modified = true;
    app.invalidate_content();
    frame(
        &mut app,
        &ctx,
        0.1,
        vec![key(Key::F, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    assert!(!app.is_editing && app.search_open && app.is_modified);
    assert_eq!(app.content, "# Draft\n\nNEEDLE");
    app.search_open = false;
    app.view_mode = ViewMode::Mindmap;
    app.open_search();
    assert!(matches!(app.view_mode, ViewMode::Markdown));
    app.search_open = false;
    app.toggle_slides_mode(&ctx);
    let output = frame(&mut app, &ctx, 0.2, Vec::new(), Modifiers::NONE);
    assert!(!texts(&output)
        .iter()
        .any(|(text, _, _)| text.contains("目錄大綱")));
    app.execute_action(PendingAction::Clear);
    assert!(!app.is_slides_mode && !app.is_slides_fullscreen);
}

#[test]
fn clicking_toolbar_close_keeps_unsaved_changes_until_the_user_chooses() {
    let (mut app, ctx) = model();
    content(
        &mut app,
        "# Draft".to_string(),
        "fixture.md",
        ViewMode::Markdown,
    );
    app.original_content = "# Original".to_string();
    app.is_modified = true;
    frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
    let output = frame(&mut app, &ctx, 0.05, Vec::new(), Modifiers::NONE);
    let position = texts(&output)
        .iter()
        .find(|(text, _, _)| text == "✕ 關閉")
        .unwrap()
        .1
        .center();
    for (time, pressed) in [(0.1, true), (0.15, false)] {
        frame(
            &mut app,
            &ctx,
            time,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            Modifiers::NONE,
        );
    }
    assert!(matches!(app.pending_action, Some(PendingAction::Close)));
    assert!(app.visible && app.is_modified && !app.close_confirmed);
    assert_eq!(app.content, "# Draft");
    app.resolve_pending_action(UnsavedChoice::Cancel);
    assert!(app.visible && app.is_modified);
    app.request_action(PendingAction::Close);
    app.resolve_pending_action(UnsavedChoice::Discard);
    assert!(!app.visible && app.close_confirmed);
}

#[test]
fn json_toolbar_does_not_offer_standard_json_rewrites_for_json_lines_or_comments() {
    for extension in ["json", "jsonl", "jsonc", "json5"] {
        let (mut app, ctx) = model();
        content(
            &mut app,
            "{\"value\":1}".to_string(),
            &format!("fixture.{extension}"),
            ViewMode::Code {
                lang: extension.to_string(),
            },
        );
        frame(&mut app, &ctx, 0.0, Vec::new(), Modifiers::NONE);
        let output = frame(&mut app, &ctx, 0.05, Vec::new(), Modifiers::NONE);
        let labels: Vec<_> = texts(&output)
            .into_iter()
            .map(|(text, _, _)| text)
            .collect();
        assert_eq!(
            labels.iter().any(|text| text == "⚡ 格式化"),
            extension == "json",
            "{extension}: {labels:?}"
        );
    }
}

#[test]
#[ignore = "Run optimized frame measurements in Windows CI"]
fn performance_preview_frames() {
    let markdown = format!("# Performance\n\n{}\n| Name | Value |\n| --- | --- |\n{}\n```mermaid\nflowchart TD\n A[Read] --> B[Preview]\n```", "Readable text 中文測試 with **style**.\n\n".repeat(200), "| Data | wrapped content |\n".repeat(30));
    let csv = format!(
        "id,value\n{}",
        (0..10_000)
            .map(|index| format!("row{index},value{index}\n"))
            .collect::<String>()
    );
    let code = (0..5000)
        .map(|index| format!("let value_{index} = {index}; // comment\n"))
        .collect::<String>();
    let cases = [
        (
            "markdown",
            "fixture.md",
            markdown.clone(),
            ViewMode::Markdown,
            false,
            false,
        ),
        (
            "csv-10000",
            "fixture.csv",
            csv,
            ViewMode::Table { separator: ',' },
            false,
            false,
        ),
        (
            "code-5000",
            "fixture.rs",
            code.clone(),
            ViewMode::Code {
                lang: "rs".to_string(),
            },
            false,
            false,
        ),
        (
            "plain-5000",
            "fixture.txt",
            code.clone(),
            ViewMode::PlainText,
            false,
            false,
        ),
        (
            "mindmap",
            "fixture.md",
            markdown.clone(),
            ViewMode::Mindmap,
            false,
            false,
        ),
        (
            "editor-1000",
            "fixture.md",
            "# Title\n\nText 中文\n".repeat(333),
            ViewMode::Markdown,
            true,
            false,
        ),
        (
            "slides",
            "fixture.md",
            "# First\n\nText\n\n---\n\n# Second".to_string(),
            ViewMode::Markdown,
            false,
            true,
        ),
        (
            "image",
            "fixture.png",
            String::new(),
            ViewMode::Image {
                format: "png".to_string(),
            },
            false,
            false,
        ),
    ];
    for (name, path, value, mode, editing, slides) in cases {
        let start = std::time::Instant::now();
        crate::markdown::prepare_document_rendering(&value);
        println!(
            "PERF {name} background_prepare_ms={:.2}",
            start.elapsed().as_secs_f64() * 1000.0_f64
        );
        let (mut app, ctx) = model();
        setup_system_cjk_fonts(&ctx);
        content(&mut app, value, path, mode);
        app.is_editing = editing;
        app.is_slides_mode = slides;
        if name == "image" {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgba8(1500, 1500)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            app.image_bytes = Some(bytes.into_inner());
            app.image_uri = Some("bytes://performance.png".to_string());
        }
        let mut times = Vec::new();
        for index in 0..65 {
            let start = std::time::Instant::now();
            let output = frame(
                &mut app,
                &ctx,
                index as f64 / 60.0_f64,
                Vec::new(),
                Modifiers::NONE,
            );
            let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
            let ms = start.elapsed().as_secs_f64() * 1000.0_f64;
            if index == 0 {
                println!("PERF {name} cold_ms={ms:.2}");
            }
            if index >= 5 {
                times.push(ms);
            }
        }
        times.sort_by(f64::total_cmp);
        let average = times.iter().sum::<f64>() / times.len() as f64;
        let p95 = times[(times.len() * 95 / 100).min(times.len() - 1)];
        println!(
            "PERF {name} avg_ms={average:.2} p95_ms={p95:.2} frames={}",
            times.len()
        );
        assert!(p95 < 50.0_f64, "{name} warm frame regression: {p95:.2} ms");
    }
}
