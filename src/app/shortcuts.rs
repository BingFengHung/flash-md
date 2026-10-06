use super::*;

impl MdPreviewApp {
    pub fn navigate_search_match(&mut self, next: bool) {
        let count = self.search_match_count;
        if count == 0 {
            return;
        }
        self.search_match_index = if next {
            (self.search_match_index + 1) % count
        } else {
            (self.search_match_index + count - 1) % count
        };
        self.search_jump_requested = true;
    }

    pub(super) fn open_file_dialog(&mut self) {
        if let Some(path) = rfd_open_file() {
            self.load_file(&path);
        }
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &Context) {
        let input = ctx.input(|input| input.clone());
        let command = input.modifiers.command;
        let plain = !command && !input.modifiers.alt;
        if self
            .loading_request
            .as_ref()
            .is_some_and(|request| request.reset_view)
        {
            if input.key_pressed(egui::Key::Escape) {
                self.cancel_document_load();
                self.set_toast("已取消載入".to_string());
            } else if plain && input.key_pressed(egui::Key::ArrowRight) {
                self.navigate_sibling_file(true);
            } else if plain && input.key_pressed(egui::Key::ArrowLeft) {
                self.navigate_sibling_file(false);
            }
            return;
        }
        if input.key_pressed(egui::Key::Escape) {
            if self.settings_open {
                self.settings_open = false;
            } else if self.is_slides_mode {
                self.is_slides_mode = false;
                if self.is_slides_fullscreen {
                    self.is_slides_fullscreen = false;
                    self.set_fullscreen_state(ctx, false);
                }
            } else if self.is_editing {
                self.is_editing = false;
            } else if self.search_open {
                self.search_open = false;
                self.search_query.clear();
                self.search_match_index = 0;
            } else if input.viewport().fullscreen.unwrap_or(false) {
                self.set_fullscreen_state(ctx, false);
            } else {
                self.request_action(PendingAction::Close);
            }
            self.held_scroll_started = None;
            return;
        }
        if self.settings_open {
            self.held_scroll_started = None;
            return;
        }
        if command && input.key_pressed(egui::Key::S) {
            self.save_current_file(false);
        }
        if command && input.key_pressed(egui::Key::E) {
            self.toggle_edit_mode();
        }
        if command && input.key_pressed(egui::Key::P) {
            self.always_on_top = !self.always_on_top;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            self.set_toast(
                if self.always_on_top {
                    "視窗置頂: 已開啟 📌"
                } else {
                    "視窗置頂: 已關閉"
                }
                .to_string(),
            );
        }
        if command && input.key_pressed(egui::Key::O) {
            if input.modifiers.shift {
                self.locate_current_file_in_explorer();
            } else if let Some(path) = &self.current_file {
                let _ = open::that(path);
            }
        }
        if command && input.modifiers.shift && input.key_pressed(egui::Key::C) {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let text = if self.content.is_empty() {
                    self.current_file
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    self.content.clone()
                };
                let _ = clipboard.set_text(text);
                self.set_toast("已複製至剪貼簿 📋".to_string());
            }
        }
        if command {
            let zoom = if input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)
            {
                1
            } else if input.key_pressed(egui::Key::Minus) {
                -1
            } else {
                0
            };
            if matches!(self.view_mode, ViewMode::Image { .. }) {
                if zoom != 0 {
                    self.image_zoom = (self.image_zoom
                        * if zoom > 0 { 1.2_f32 } else { 1.0_f32 / 1.2_f32 })
                    .clamp(0.1_f32, 10.0_f32);
                    self.image_fit_mode = false;
                }
                if input.key_pressed(egui::Key::Num0) {
                    self.image_zoom = 1.0_f32;
                    self.image_fit_mode = false;
                }
            } else {
                if zoom != 0 {
                    self.font_scale =
                        (self.font_scale + zoom as f32 * 0.1_f32).clamp(0.6_f32, 2.5_f32);
                }
                if input.key_pressed(egui::Key::Num0) {
                    self.font_scale = 1.0_f32;
                }
            }
        }
        if !self.is_editing && command && input.key_pressed(egui::Key::M) {
            self.cycle_view_mode();
        }
        if !self.is_editing && input.key_pressed(egui::Key::F6) {
            match self.view_mode {
                ViewMode::Markdown if !self.content.is_empty() => {
                    self.view_mode = ViewMode::Mindmap
                }
                ViewMode::Mindmap => self.view_mode = ViewMode::Markdown,
                _ => {}
            }
        }
        if command && input.key_pressed(egui::Key::F) {
            self.search_open = true;
            self.search_focus_requested = true;
        }
        if input.key_pressed(egui::Key::F3) {
            if self.search_open {
                self.navigate_search_match(!input.modifiers.shift);
            } else {
                self.search_open = true;
                self.search_focus_requested = true;
            }
        }
        if !self.is_editing
            && command
            && input.key_pressed(egui::Key::T)
            && matches!(self.view_mode, ViewMode::Markdown)
        {
            self.toc_open = !self.toc_open;
        }
        if !self.is_editing
            && !self.search_open
            && matches!(self.view_mode, ViewMode::Markdown)
            && (input.key_pressed(egui::Key::F5)
                || (plain && !ctx.wants_keyboard_input() && input.key_pressed(egui::Key::P)))
        {
            self.is_slides_mode = !self.is_slides_mode;
            self.is_slides_fullscreen = self.is_slides_mode;
            if self.is_slides_mode {
                self.current_slide_index = 0;
            }
            self.set_fullscreen_state(ctx, self.is_slides_mode);
        }
        if input.key_pressed(egui::Key::F11) {
            let fullscreen = !input.viewport().fullscreen.unwrap_or(false);
            if self.is_slides_mode {
                self.is_slides_fullscreen = fullscreen;
            }
            self.set_fullscreen_state(ctx, fullscreen);
        }
        if self.is_slides_mode {
            let total = crate::parsers::cached_slides(ctx, &self.content).len();
            if plain
                && (input.key_pressed(egui::Key::ArrowRight)
                    || input.key_pressed(egui::Key::PageDown)
                    || input.key_pressed(egui::Key::Space)
                    || input.key_pressed(egui::Key::Enter)
                    || input.key_pressed(egui::Key::L))
            {
                self.current_slide_index =
                    (self.current_slide_index + 1).min(total.saturating_sub(1));
            }
            if plain
                && (input.key_pressed(egui::Key::ArrowLeft)
                    || input.key_pressed(egui::Key::PageUp)
                    || input.key_pressed(egui::Key::Backspace)
                    || input.key_pressed(egui::Key::H))
            {
                self.current_slide_index = self.current_slide_index.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::Home) {
                self.current_slide_index = 0;
            }
            if input.key_pressed(egui::Key::End) {
                self.current_slide_index = total.saturating_sub(1);
            }
            if plain && input.key_pressed(egui::Key::F) {
                self.is_slides_fullscreen = !self.is_slides_fullscreen;
                self.set_fullscreen_state(ctx, self.is_slides_fullscreen);
            }
            return;
        }
        if self.is_editing || self.search_open || ctx.wants_keyboard_input() {
            self.held_scroll_started = None;
            return;
        }
        if plain && input.key_pressed(egui::Key::E) {
            self.toggle_edit_mode();
            return;
        }
        if plain && input.key_pressed(egui::Key::Slash) {
            self.search_open = true;
            self.search_focus_requested = true;
            return;
        }
        if plain && input.key_pressed(egui::Key::N) {
            self.navigate_search_match(!input.modifiers.shift);
        }
        if plain && (input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::H)) {
            self.navigate_sibling_file(false);
            return;
        }
        if plain && (input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::L)) {
            self.navigate_sibling_file(true);
            return;
        }
        let direction = if plain
            && (input.key_down(egui::Key::ArrowDown)
                || (!input.modifiers.shift && input.key_down(egui::Key::J)))
        {
            1
        } else if plain
            && (input.key_down(egui::Key::ArrowUp)
                || (!input.modifiers.shift && input.key_down(egui::Key::K)))
        {
            -1
        } else {
            0
        };
        let mut delta = 0.0_f32;
        if direction == 0 {
            self.held_scroll_started = None;
        } else {
            let first = self
                .held_scroll_started
                .is_none_or(|(_, previous)| previous != direction);
            if first {
                self.held_scroll_started = Some((input.time, direction));
                delta = direction as f32 * 48.0_f32 * self.font_scale;
            } else if self
                .held_scroll_started
                .is_some_and(|(start, _)| input.time - start >= 0.25_f64)
            {
                delta = direction as f32
                    * 720.0_f32
                    * self.font_scale
                    * input.unstable_dt.clamp(0.0_f32, 0.05_f32);
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let page = (ctx.available_rect().height() * 0.85_f32).max(120.0_f32);
        let text = |value: &str| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Text(text) if text == value))
        };
        let top = plain
            && (input.key_pressed(egui::Key::Home)
                || (input.key_pressed(egui::Key::G) && !input.modifiers.shift)
                || text("g"));
        let bottom = plain
            && (input.key_pressed(egui::Key::End)
                || (input.key_pressed(egui::Key::G) && input.modifiers.shift)
                || text("G"));
        if top {
            self.target_scroll_offset = Some(0.0_f32);
        } else if bottom {
            self.target_scroll_offset = Some(self.max_scroll_offset);
        } else {
            if (plain && input.key_pressed(egui::Key::PageDown))
                || (command && input.key_pressed(egui::Key::D))
            {
                delta += page;
            }
            if (plain && input.key_pressed(egui::Key::PageUp))
                || (command && input.key_pressed(egui::Key::U))
            {
                delta -= page;
            }
            if plain && input.key_pressed(egui::Key::Space) {
                delta += if input.modifiers.shift { -page } else { page };
            }
            if plain && direction == 0 {
                if text("j") {
                    delta += 48.0_f32 * self.font_scale;
                }
                if text("k") {
                    delta -= 48.0_f32 * self.font_scale;
                }
            }
            if delta != 0.0_f32 {
                self.target_scroll_offset = Some((self.current_scroll_offset + delta).max(0.0_f32));
            }
        }
        if let Some(target) = self.target_scroll_offset {
            self.current_scroll_offset = target;
            ctx.request_repaint();
        }
    }

    pub(super) fn cycle_view_mode(&mut self) {
        let extension = self
            .current_file
            .as_ref()
            .and_then(|path| path.extension())
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let modes = preview_modes(self.document_kind, &extension);
        let index = modes.iter().position(|mode| *mode == self.view_mode);
        self.view_mode = modes[index.map_or(0, |index| (index + 1) % modes.len())].clone();
        self.reset_scroll_to_top = true;
        self.current_scroll_offset = 0.0_f32;
        self.target_scroll_offset = None;
        self.target_anchor = None;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.search_match_index = 0;
        self.search_jump_requested = !self.search_query.trim().is_empty();
    }
}

pub(super) fn preview_modes(kind: DocumentKind, extension: &str) -> Vec<ViewMode> {
    if is_image_extension(extension) {
        let image = ViewMode::Image {
            format: extension.to_string(),
        };
        return if extension == "svg" {
            vec![
                image,
                ViewMode::Code {
                    lang: "xml".to_string(),
                },
                ViewMode::PlainText,
            ]
        } else {
            vec![image]
        };
    }
    if kind == DocumentKind::Pdf
        || matches!(extension, "md" | "markdown" | "mdown" | "mkd" | "mkdn")
    {
        vec![
            ViewMode::Markdown,
            ViewMode::Mindmap,
            ViewMode::Code {
                lang: "md".to_string(),
            },
            ViewMode::PlainText,
        ]
    } else if matches!(extension, "csv" | "tsv") {
        vec![
            ViewMode::Table {
                separator: if extension == "tsv" { '\t' } else { ',' },
            },
            ViewMode::Code {
                lang: extension.to_string(),
            },
            ViewMode::PlainText,
            ViewMode::Markdown,
        ]
    } else if is_code_extension(extension) {
        vec![
            ViewMode::Code {
                lang: extension.to_string(),
            },
            ViewMode::PlainText,
            ViewMode::Markdown,
        ]
    } else {
        vec![ViewMode::PlainText, ViewMode::Markdown, ViewMode::Mindmap]
    }
}
