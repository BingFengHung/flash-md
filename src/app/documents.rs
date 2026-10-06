use super::*;

impl MdPreviewApp {
    pub fn load_file(&mut self, path: &Path) {
        self.request_action(PendingAction::Open(path.to_path_buf()));
    }

    pub(super) fn open_document(&mut self, path: &Path) {
        match load_document(path) {
            Ok(loaded) => {
                self.apply_document(loaded, true);
                self.visible = true;
                show_and_focus_app_window();
                self.set_toast(format!(
                    "⚡ 已開啟：{}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(error) => self.set_toast(format!("❌ 無法開啟檔案：{}", error)),
        }
    }

    pub(super) fn apply_document(&mut self, loaded: LoadedDocument, reset_view: bool) {
        if reset_view {
            self.view_mode = if loaded.image_bytes.is_some() {
                ViewMode::Image {
                    format: loaded.extension.clone(),
                }
            } else if loaded.kind == DocumentKind::Pdf
                || matches!(
                    loaded.extension.as_str(),
                    "md" | "markdown" | "mdown" | "mkd" | "pdf"
                )
            {
                ViewMode::Markdown
            } else if matches!(loaded.extension.as_str(), "csv" | "tsv") {
                ViewMode::Table {
                    separator: if loaded.extension == "tsv" { '\t' } else { ',' },
                }
            } else if is_code_extension(&loaded.extension) {
                ViewMode::Code {
                    lang: loaded.extension.clone(),
                }
            } else {
                ViewMode::PlainText
            };
            self.reset_scroll_to_top = true;
            self.current_scroll_offset = 0.0_f32;
            self.target_scroll_offset = None;
            self.target_anchor = None;
            self.search_match_index = 0;
            self.mindmap_state = Default::default();
            self.is_editing = false;
            self.is_slides_mode = false;
            self.current_slide_index = 0;
            self.image_zoom = 1.0_f32;
            self.image_fit_mode = true;
        }
        self.document_kind = loaded.kind;
        self.content = loaded.content;
        self.original_content = self.content.clone();
        self.is_modified = false;
        self.last_edit_instant = None;
        self.line_count = self.content.lines().count();
        self.file_size_str = loaded.size;
        self.last_modified_str = loaded.modified;
        self.current_file = Some(loaded.path);
        self.image_bytes = loaded.image_bytes;
        self.refresh_image_uri(&loaded.extension);
        self.invalidate_content();
        self.siblings = self
            .current_file
            .as_deref()
            .map(crate::files::sibling_files)
            .unwrap_or_default();
        self.watched_file = Some(loaded.watch_path.clone());
        self.file_watcher.watch_file(&loaded.watch_path);
    }

    pub(super) fn refresh_image_uri(&mut self, extension: &str) {
        self.image_uri = self.image_bytes.as_ref().map(|bytes| {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hash);
            format!("bytes://viewer_{:x}.{}", hash.finish(), extension)
        });
    }

    pub(super) fn invalidate_content(&mut self) {
        self.content_revision = self.content_revision.wrapping_add(1);
        self.mindmap_root = None;
        self.mindmap_state.collapsed_ids.clear();
        self.mindmap_state.initialized = false;
        self.search_match_count = 0;
        self.search_match_index = 0;
        self.search_jump_requested = !self.search_query.trim().is_empty();
        if self.document_kind == DocumentKind::Svg {
            self.image_bytes = Some(self.content.as_bytes().to_vec());
            self.refresh_image_uri("svg");
        }
    }

    pub fn save_current_file(&mut self, is_auto: bool) -> bool {
        if !self.document_kind.can_save() {
            self.set_toast("ℹ 此預覽為唯讀，無法寫回圖片、PDF 或 ZIP 原始檔案".to_string());
            return false;
        }
        let Some(path) = self.current_file.clone() else {
            return false;
        };
        if !self.is_modified {
            return true;
        }
        match fs::read_to_string(&path) {
            Ok(disk) if disk != self.original_content => {
                self.set_toast("❌ 原始檔案已被其他程式修改；已保留目前編輯，取消覆寫".to_string());
                return false;
            }
            Err(error) => {
                self.set_toast(format!("❌ 無法確認原始檔案：{}", error));
                return false;
            }
            _ => {}
        }
        match crate::document::save_document(&path, self.document_kind, &self.content) {
            Ok(()) => {
                self.original_content = self.content.clone();
                self.is_modified = false;
                self.last_edit_instant = None;
                self.line_count = self.content.lines().count();
                self.file_size_str = crate::files::format_file_size(self.content.len() as u64);
                self.set_toast(
                    if is_auto {
                        "💾 已自動儲存"
                    } else {
                        "💾 檔案已成功儲存"
                    }
                    .to_string(),
                );
                true
            }
            Err(error) => {
                self.set_toast(format!("❌ 儲存檔案失敗：{}", error));
                false
            }
        }
    }

    pub(super) fn request_action(&mut self, action: PendingAction) {
        if self.pending_action.is_some() {
            return;
        }
        if self.is_updating {
            self.set_toast("更新正在進行，請稍候".to_string());
            return;
        }
        if self.is_modified {
            self.pending_action = Some(action);
            self.visible = true;
            show_and_focus_app_window();
        } else {
            self.execute_action(action);
        }
    }

    pub(super) fn execute_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::Open(path) => self.open_document(&path),
            PendingAction::Clear => {
                self.current_file = None;
                self.watched_file = None;
                self.document_kind = DocumentKind::Text;
                self.content.clear();
                self.original_content.clear();
                self.is_modified = false;
                self.is_editing = false;
                self.image_uri = None;
                self.image_bytes = None;
                self.line_count = 0;
                self.file_size_str.clear();
                self.last_modified_str.clear();
                self.siblings.clear();
                self.file_watcher.unwatch();
                self.invalidate_content();
                self.visible = true;
                show_and_focus_app_window();
            }
            PendingAction::Close => {
                self.visible = false;
                hide_app_window();
                if self.is_standalone {
                    self.close_confirmed = true;
                    if let Ok(holder) = self.ctx_holder.lock() {
                        if let Some(ctx) = holder.as_ref() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            }
            PendingAction::Exit => std::process::exit(0),
            PendingAction::Update => self.start_self_update(),
        }
    }

    pub(super) fn render_unsaved_dialog(&mut self, ctx: &egui::Context) {
        let mut choice = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(32.0_f32);
                ui.heading("目前文件有未儲存的修改");
            });
        });
        egui::Window::new("保留你的編輯內容")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("要先儲存目前修改，再繼續操作嗎？");
                if let Some((message, _)) = &self.status_toast {
                    if message.starts_with("❌") {
                        ui.colored_label(Color32::RED, message);
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("儲存並繼續").clicked() {
                        choice = Some(UnsavedChoice::Save);
                    }
                    if ui.button("捨棄修改").clicked() {
                        choice = Some(UnsavedChoice::Discard);
                    }
                    if ui.button("取消").clicked() {
                        choice = Some(UnsavedChoice::Cancel);
                    }
                });
            });
        if let Some(choice) = choice {
            self.resolve_pending_action(choice);
        }
    }

    pub(super) fn resolve_pending_action(&mut self, choice: UnsavedChoice) {
        match choice {
            UnsavedChoice::Save => {
                if self.save_current_file(false) {
                    if let Some(action) = self.pending_action.take() {
                        self.execute_action(action);
                    }
                }
            }
            UnsavedChoice::Discard => {
                self.content = self.original_content.clone();
                self.is_modified = false;
                self.is_editing = false;
                self.last_edit_instant = None;
                self.invalidate_content();
                if let Some(action) = self.pending_action.take() {
                    self.execute_action(action);
                }
            }
            UnsavedChoice::Cancel => self.pending_action = None,
        }
    }

    pub fn toggle_edit_mode(&mut self) {
        if !self.document_kind.can_save() {
            self.set_toast("ℹ 此預覽為唯讀，無法編輯原始檔案".to_string());
            return;
        }
        self.is_editing = !self.is_editing;
        if self.is_editing {
            self.set_toast("✏ 已進入全螢幕就地編輯模式 (Ctrl+S 保存，E 退出)".to_string());
        } else {
            self.set_toast("👁 已切換至美化預覽模式".to_string());
        }
    }

    pub(super) fn render_editor(&mut self, ui: &mut egui::Ui) {
        let out =
            crate::views::editor::render_editor(ui, self.theme, self.font_scale, &mut self.content);
        if out.changed {
            self.line_count = out.new_line_count;
            self.is_modified = self.content != self.original_content;
            self.last_edit_instant = Some(std::time::Instant::now());
            self.invalidate_content();
            if self.config.save_mode == SaveMode::AutoDebounce {
                ui.ctx().request_repaint_after(Duration::from_millis(800));
            }
        }
    }

    pub(super) fn render_slides_mode(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let base_dir = self.current_file.as_ref().and_then(|p| p.parent());
        let out = crate::views::presentation::render_slides_mode(
            ui,
            self.theme,
            self.font_scale,
            &self.content,
            base_dir,
            &mut self.current_slide_index,
            self.is_slides_fullscreen,
        );

        if out.toggle_fullscreen {
            self.is_slides_fullscreen = !self.is_slides_fullscreen;
            self.set_fullscreen_state(ctx, self.is_slides_fullscreen);
        }
        if out.exit_slides {
            self.is_slides_mode = false;
            if self.is_slides_fullscreen {
                self.is_slides_fullscreen = false;
                self.set_fullscreen_state(ctx, false);
            }
            self.set_toast("👁 已退出簡報投影模式".to_string());
        }
    }

    pub fn reload_current_file(&mut self) {
        if self.is_modified {
            return;
        }
        if let Some(path) = self.current_file.clone() {
            match load_document(&path) {
                Ok(loaded) => {
                    self.apply_document(loaded, false);
                    self.set_toast("檔案已即時同步更新 ⚡".to_string());
                }
                Err(error) => self.set_toast(format!("❌ 檔案重載失敗：{}", error)),
            }
        }
    }

    pub fn handle_hotkey_preview(&mut self, target_path: Option<PathBuf>) {
        if self.pending_action.is_some() || self.is_updating {
            self.visible = true;
            show_and_focus_app_window();
            return;
        }
        if let Some(path) = target_path {
            if self.current_file.as_deref() == Some(path.as_path()) {
                self.visible = !self.visible;
                if self.visible {
                    show_and_focus_app_window();
                } else {
                    hide_app_window();
                }
            } else {
                self.load_file(&path);
            }
        } else if self.visible {
            self.visible = false;
            hide_app_window();
        } else {
            self.request_action(PendingAction::Clear);
        }
    }

    pub fn navigate_sibling_file(&mut self, forward: bool) {
        let Some(current) = self.current_file.as_ref() else {
            return;
        };
        let files = crate::files::sibling_files(current);
        if files.is_empty() {
            return;
        }
        let index = files.iter().position(|p| p == current).unwrap_or(0);
        let next = if forward {
            (index + 1) % files.len()
        } else {
            (index + files.len() - 1) % files.len()
        };
        self.load_file(&files[next]);
    }

    pub fn get_sibling_info(&self) -> Option<(usize, usize)> {
        let current = self.current_file.as_ref()?;
        Some((
            self.siblings.iter().position(|p| p == current)? + 1,
            self.siblings.len(),
        ))
    }

    /// 在 Windows 檔案總管中定位目前檔案。
    pub fn locate_current_file_in_explorer(&mut self) {
        if let Some(ref path) = self.current_file {
            let path_str = path.to_string_lossy().to_string();
            let _ = std::process::Command::new("explorer.exe")
                .arg(format!("/select,{}", path_str))
                .spawn();
            self.set_toast("已在檔案總管中定位檔案 📁".to_string());
        }
    }

    /// 一鍵排版美化 JSON / JSON5 / JSONC
    pub fn format_json_content(&mut self) {
        if !self.document_kind.can_save() {
            self.set_toast("ℹ 此預覽為唯讀，無法修改原始檔案".to_string());
            return;
        }
        if let Ok(formatted) = crate::markdown::format_json(&self.content) {
            self.content = formatted;
            self.is_modified = self.content != self.original_content;
            self.last_edit_instant = Some(std::time::Instant::now());
            self.invalidate_content();
            self.line_count = self.content.lines().count();
            self.set_toast("已完成 JSON 排版美化 ⚡".to_string());
        } else {
            self.set_toast("JSON 格式無效或解析失敗 ⚠".to_string());
        }
    }

    /// 一鍵壓縮 JSON 為單行
    pub fn minify_json_content(&mut self) {
        if !self.document_kind.can_save() {
            self.set_toast("ℹ 此預覽為唯讀，無法修改原始檔案".to_string());
            return;
        }
        match crate::markdown::minify_json(&self.content) {
            Ok(content) => {
                self.content = content;
                self.is_modified = self.content != self.original_content;
                self.last_edit_instant = Some(std::time::Instant::now());
                self.invalidate_content();
                if self.config.save_mode == SaveMode::AutoDebounce {
                    if let Ok(holder) = self.ctx_holder.lock() {
                        if let Some(ctx) = holder.as_ref() {
                            ctx.request_repaint_after(Duration::from_millis(800));
                        }
                    }
                }
                self.line_count = self.content.lines().count();
                self.set_toast("已壓縮為單行 JSON 📦".to_string());
            }
            Err(error) => self.set_toast(format!("❌ JSON 格式無效：{}", error)),
        }
    }
}
