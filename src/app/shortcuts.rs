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

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        // ESC: 隱藏或關閉視窗 (若簡報模式/設定/搜尋列/編輯模式開啟則優先關閉或退出)
        if input.key_pressed(egui::Key::Escape) {
            if self.is_slides_mode {
                self.is_slides_mode = false;
                if self.is_slides_fullscreen {
                    self.is_slides_fullscreen = false;
                    self.set_fullscreen_state(ctx, false);
                }
                self.set_toast("👁 已退出簡報投影模式".to_string());
            } else if self.settings_open {
                self.settings_open = false;
            } else if self.is_editing {
                self.is_editing = false;
                self.set_toast("👁 已退出編輯模式，回到預覽".to_string());
            } else if self.search_open {
                self.search_open = false;
                self.search_query.clear();
                self.search_match_index = 0;
            } else if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                self.set_fullscreen_state(ctx, false);
                self.set_toast("🗗 已退出全螢幕模式".to_string());
            } else {
                self.request_action(PendingAction::Close);
            }
        }

        // Ctrl + S: 手動保存檔案
        if input.modifiers.command && input.key_pressed(egui::Key::S) {
            self.save_current_file(false);
        }

        // Ctrl + E: 切換就地編輯模式與預覽模式
        if input.modifiers.command && input.key_pressed(egui::Key::E) {
            self.toggle_edit_mode();
        }

        // F3 / Shift + F3: 搜尋結果上一筆 / 下一筆跳轉
        if input.key_pressed(egui::Key::F3) {
            if !self.search_open {
                self.search_open = true;
                self.search_focus_requested = true;
            } else if input.modifiers.shift {
                self.navigate_search_match(false);
            } else {
                self.navigate_search_match(true);
            }
        }

        // 鍵盤導航與平滑捲動操作 (非文字編輯/搜尋輸入/簡報模式下觸發)
        if !self.is_editing && !self.search_open && !self.is_slides_mode {
            // E: 就地編輯模式切換快速鍵
            if input.key_pressed(egui::Key::E) && !input.modifiers.command && !input.modifiers.alt {
                self.toggle_edit_mode();
            }
            // / : Vim 搜尋快捷鍵 (開啟搜尋並聚焦輸入框)
            if input.key_pressed(egui::Key::Slash) {
                self.search_open = true;
                self.search_focus_requested = true;
            }

            // n / N : Vim 搜尋跳轉 (n 下一筆，N / Shift+n 上一筆)
            if input.key_pressed(egui::Key::N) && !input.modifiers.command && !input.modifiers.alt {
                if input.modifiers.shift {
                    self.navigate_search_match(false);
                } else {
                    self.navigate_search_match(true);
                }
            }

            // ← / → 或 h / l (Vim): 切換同目錄上一個 / 下一個檔案
            if self.current_file.is_some() {
                if input.key_pressed(egui::Key::ArrowLeft)
                    || (input.key_pressed(egui::Key::H)
                        && !input.modifiers.command
                        && !input.modifiers.alt)
                {
                    self.navigate_sibling_file(false);
                } else if input.key_pressed(egui::Key::ArrowRight)
                    || (input.key_pressed(egui::Key::L)
                        && !input.modifiers.command
                        && !input.modifiers.alt)
                {
                    self.navigate_sibling_file(true);
                }
            }

            // ↑ / ↓ 或 j / k (Vim): 捲動瀏覽當前文件內容 (支援單擊、長按連續平滑捲動、Vim g/G 置頂置底、PageUp/PageDown/Space 翻頁)
            let mut is_down =
                input.key_pressed(egui::Key::ArrowDown) || input.key_down(egui::Key::ArrowDown);
            let mut is_up =
                input.key_pressed(egui::Key::ArrowUp) || input.key_down(egui::Key::ArrowUp);
            let mut is_j = (input.key_pressed(egui::Key::J) || input.key_down(egui::Key::J))
                && !input.modifiers.command
                && !input.modifiers.alt
                && !input.modifiers.shift;
            let mut is_k = (input.key_pressed(egui::Key::K) || input.key_down(egui::Key::K))
                && !input.modifiers.command
                && !input.modifiers.alt
                && !input.modifiers.shift;
            let mut is_g = input.key_pressed(egui::Key::G)
                && !input.modifiers.command
                && !input.modifiers.alt
                && !input.modifiers.shift;
            let mut is_big_g = (input.key_pressed(egui::Key::G)
                && input.modifiers.shift
                && !input.modifiers.command
                && !input.modifiers.alt)
                || input.key_pressed(egui::Key::End);
            let mut is_page_down = input.key_pressed(egui::Key::PageDown)
                || (input.modifiers.command && input.key_pressed(egui::Key::D));
            let mut is_page_up = input.key_pressed(egui::Key::PageUp)
                || (input.modifiers.command && input.key_pressed(egui::Key::U));
            let mut is_space = input.key_pressed(egui::Key::Space)
                && !input.modifiers.alt
                && !input.modifiers.command;
            let is_shift = input.modifiers.shift;

            // 雙重保險：檢查所有原始輸入事件 (Text events 如使用者鍵入 'j', 'k', 'g', 'G')
            for ev in &input.events {
                match ev {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if !modifiers.command && !modifiers.alt {
                            if *key == egui::Key::ArrowDown {
                                is_down = true;
                            }
                            if *key == egui::Key::ArrowUp {
                                is_up = true;
                            }
                            if *key == egui::Key::J && !modifiers.shift {
                                is_j = true;
                            }
                            if *key == egui::Key::K && !modifiers.shift {
                                is_k = true;
                            }
                            if *key == egui::Key::G && !modifiers.shift {
                                is_g = true;
                            }
                            if *key == egui::Key::G && modifiers.shift {
                                is_big_g = true;
                            }
                            if *key == egui::Key::PageDown {
                                is_page_down = true;
                            }
                            if *key == egui::Key::PageUp {
                                is_page_up = true;
                            }
                            if *key == egui::Key::Home {
                                is_g = true;
                            }
                            if *key == egui::Key::End {
                                is_big_g = true;
                            }
                        }
                    }
                    egui::Event::Text(s) => {
                        if !input.modifiers.command && !input.modifiers.alt {
                            if s == "j" {
                                is_j = true;
                            }
                            if s == "k" {
                                is_k = true;
                            }
                            if s == "g" {
                                is_g = true;
                            }
                            if s == "G" {
                                is_big_g = true;
                            }
                        }
                    }
                    _ => {}
                }
            }

            let mut scroll_triggered = false;

            if input.key_pressed(egui::Key::Home) || is_g {
                // 置頂
                self.current_scroll_offset = 0.0_f32;
                self.target_scroll_offset = Some(0.0_f32);
                self.reset_scroll_to_top = true;
                scroll_triggered = true;
            } else if is_big_g {
                // 置底
                self.current_scroll_offset = 999999.0_f32;
                self.target_scroll_offset = Some(999999.0_f32);
                scroll_triggered = true;
            } else if is_down || is_j {
                // 向下捲動 (單行)
                let step = 48.0_f32 * self.font_scale;
                self.current_scroll_offset += step;
                self.target_scroll_offset = Some(self.current_scroll_offset);
                scroll_triggered = true;
            } else if is_up || is_k {
                // 向上捲動 (單行)
                let step = 48.0_f32 * self.font_scale;
                self.current_scroll_offset = (self.current_scroll_offset - step).max(0.0_f32);
                self.target_scroll_offset = Some(self.current_scroll_offset);
                scroll_triggered = true;
            } else if is_page_down {
                // 向下翻頁
                let step = 420.0_f32 * self.font_scale;
                self.current_scroll_offset += step;
                self.target_scroll_offset = Some(self.current_scroll_offset);
                scroll_triggered = true;
            } else if is_page_up {
                // 向上翻頁
                let step = 420.0_f32 * self.font_scale;
                self.current_scroll_offset = (self.current_scroll_offset - step).max(0.0_f32);
                self.target_scroll_offset = Some(self.current_scroll_offset);
                scroll_triggered = true;
            } else if is_space {
                // 空白鍵翻頁
                let step = 420.0_f32 * self.font_scale;
                if is_shift {
                    self.current_scroll_offset = (self.current_scroll_offset - step).max(0.0_f32);
                } else {
                    self.current_scroll_offset += step;
                }
                self.target_scroll_offset = Some(self.current_scroll_offset);
                scroll_triggered = true;
            }

            if scroll_triggered {
                ctx.request_repaint();
            }
        }

        // F5 或 P: 切換全螢幕簡報投影模式 (非編輯/搜尋輸入狀態下)
        if !self.is_editing && !self.search_open && matches!(self.view_mode, ViewMode::Markdown) {
            if input.key_pressed(egui::Key::F5)
                || (input.key_pressed(egui::Key::P)
                    && !input.modifiers.command
                    && !input.modifiers.alt
                    && !ctx.wants_keyboard_input())
                || (input.modifiers.command && input.key_pressed(egui::Key::P))
            {
                self.is_slides_mode = !self.is_slides_mode;
                if self.is_slides_mode {
                    self.current_slide_index = 0;
                    self.is_slides_fullscreen = true;
                    self.set_fullscreen_state(ctx, true);
                    self.set_toast(
                        "📽 已進入全螢幕簡報投影模式 (F5/Esc 退出，左右鍵翻頁)".to_string(),
                    );
                } else {
                    if self.is_slides_fullscreen {
                        self.is_slides_fullscreen = false;
                        self.set_fullscreen_state(ctx, false);
                    }
                    self.set_toast("👁 已退出簡報投影模式".to_string());
                }
            }
        }

        // 簡報投影模式專屬鍵盤導航 (左右/Page/Space/Enter/翻頁/全螢幕)
        if self.is_slides_mode {
            let total_slides = crate::parsers::cached_slides(ctx, &self.content).len();
            if input.key_pressed(egui::Key::ArrowRight)
                || input.key_pressed(egui::Key::PageDown)
                || input.key_pressed(egui::Key::Space)
                || input.key_pressed(egui::Key::Enter)
                || (input.key_pressed(egui::Key::L)
                    && !input.modifiers.command
                    && !input.modifiers.alt)
            {
                if self.current_slide_index + 1 < total_slides {
                    self.current_slide_index += 1;
                    ctx.request_repaint();
                }
            }
            if input.key_pressed(egui::Key::ArrowLeft)
                || input.key_pressed(egui::Key::PageUp)
                || input.key_pressed(egui::Key::Backspace)
                || (input.key_pressed(egui::Key::H)
                    && !input.modifiers.command
                    && !input.modifiers.alt)
            {
                if self.current_slide_index > 0 {
                    self.current_slide_index -= 1;
                    ctx.request_repaint();
                }
            }
            if input.key_pressed(egui::Key::Home) {
                self.current_slide_index = 0;
                ctx.request_repaint();
            }
            if input.key_pressed(egui::Key::End) {
                self.current_slide_index = total_slides.saturating_sub(1);
                ctx.request_repaint();
            }
            if input.key_pressed(egui::Key::F) && !input.modifiers.command && !input.modifiers.alt {
                self.is_slides_fullscreen = !self.is_slides_fullscreen;
                self.set_fullscreen_state(ctx, self.is_slides_fullscreen);
            }
        }

        // F11: 全域切換全螢幕模式 (一般預覽與簡報模式均支援)
        if input.key_pressed(egui::Key::F11) {
            if self.is_slides_mode {
                self.is_slides_fullscreen = !self.is_slides_fullscreen;
                self.set_fullscreen_state(ctx, self.is_slides_fullscreen);
            } else {
                let is_fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                let next_fs = !is_fs;
                self.set_fullscreen_state(ctx, next_fs);
                self.set_toast(if next_fs {
                    "⛶ 已進入全螢幕模式 (F11 退出)".to_string()
                } else {
                    "🗗 已退出全螢幕模式".to_string()
                });
            }
        }

        // Ctrl + F: 啟動搜尋列並自動聚焦輸入框
        if input.modifiers.command && input.key_pressed(egui::Key::F) {
            if !self.search_open {
                self.search_open = true;
            }
            self.search_focus_requested = true;
        }

        // Ctrl + T: 開啟/收起 Markdown 目錄大綱側邊欄
        if input.modifiers.command && input.key_pressed(egui::Key::T) {
            if matches!(self.view_mode, ViewMode::Markdown) {
                self.toc_open = !self.toc_open;
                self.set_toast(if self.toc_open {
                    "已開啟目錄大綱 📑".to_string()
                } else {
                    "已收起目錄大綱".to_string()
                });
            }
        }

        // F6: 快速切換 Markdown 與 互動心智圖模式
        if !self.is_editing && input.key_pressed(egui::Key::F6) {
            if matches!(self.view_mode, ViewMode::Mindmap) {
                self.view_mode = ViewMode::Markdown;
                self.set_toast("已切換回 Markdown 渲染模式 📄".to_string());
            } else if matches!(self.view_mode, ViewMode::Markdown) && !self.content.is_empty() {
                self.view_mode = ViewMode::Mindmap;
                self.set_toast("已切換至 🧠 互動心智圖模式".to_string());
            }
        }

        // Ctrl + Shift + O: 在 Windows 檔案總管中高亮定位目前檔案
        if input.modifiers.command && input.modifiers.shift && input.key_pressed(egui::Key::O) {
            self.locate_current_file_in_explorer();
        }

        // Ctrl + O: 在外部預設編輯器開啟
        if input.modifiers.command && !input.modifiers.shift && input.key_pressed(egui::Key::O) {
            if let Some(ref path) = self.current_file {
                let _ = open::that(path);
            }
        }

        // Ctrl + M: 切換 Markdown 預覽 / 程式碼語法高亮 / 斑馬紋表格 / 純文字模式 / 圖片檢視模式
        if input.modifiers.command && input.key_pressed(egui::Key::M) {
            let ext = self
                .current_file
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            self.view_mode = match self.view_mode {
                ViewMode::Markdown => {
                    if matches!(ext.as_str(), "md" | "markdown" | "mdown" | "mkdn")
                        || (!self.content.is_empty()
                            && self.content.lines().any(|l| l.trim().starts_with('#')))
                    {
                        ViewMode::Mindmap
                    } else if is_image_extension(&ext) {
                        ViewMode::Image { format: ext }
                    } else if ext == "csv" {
                        ViewMode::Table { separator: ',' }
                    } else if ext == "tsv" {
                        ViewMode::Table { separator: '\t' }
                    } else if is_code_extension(&ext) {
                        ViewMode::Code { lang: ext }
                    } else {
                        ViewMode::PlainText
                    }
                }
                ViewMode::Mindmap => {
                    if is_image_extension(&ext) {
                        ViewMode::Image { format: ext }
                    } else if ext == "csv" {
                        ViewMode::Table { separator: ',' }
                    } else if ext == "tsv" {
                        ViewMode::Table { separator: '\t' }
                    } else if is_code_extension(&ext) {
                        ViewMode::Code { lang: ext }
                    } else {
                        ViewMode::PlainText
                    }
                }
                ViewMode::Table { separator } => ViewMode::Code {
                    lang: if separator == '\t' {
                        "tsv".to_string()
                    } else {
                        "csv".to_string()
                    },
                },
                ViewMode::Code { .. } => {
                    if is_image_extension(&ext) {
                        ViewMode::Image { format: ext }
                    } else if ext == "csv" {
                        ViewMode::Table { separator: ',' }
                    } else if ext == "tsv" {
                        ViewMode::Table { separator: '\t' }
                    } else {
                        ViewMode::PlainText
                    }
                }
                ViewMode::PlainText => {
                    if is_image_extension(&ext) {
                        ViewMode::Image { format: ext }
                    } else if ext == "csv" {
                        ViewMode::Table { separator: ',' }
                    } else if ext == "tsv" {
                        ViewMode::Table { separator: '\t' }
                    } else {
                        ViewMode::Markdown
                    }
                }
                ViewMode::Image { .. } => {
                    if ext == "svg" || !self.content.is_empty() {
                        ViewMode::Code {
                            lang: "xml".to_string(),
                        }
                    } else {
                        ViewMode::PlainText
                    }
                }
            };

            self.reset_scroll_to_top = true;
            self.current_scroll_offset = 0.0_f32;

            self.set_toast(match self.view_mode {
                ViewMode::Markdown => "已切換至 Markdown 渲染模式 📄".to_string(),
                ViewMode::Mindmap => "已切換至 🧠 互動心智圖模式".to_string(),
                ViewMode::Table { separator } => {
                    if separator == '\t' {
                        "已切換至 TSV 資料表格模式 📊".to_string()
                    } else {
                        "已切換至 CSV 資料表格模式 📊".to_string()
                    }
                }
                ViewMode::Code { ref lang } => {
                    let (name, emoji) = get_language_badge(lang);
                    format!("已切換至 {} {} 語法高亮模式", emoji, name)
                }
                ViewMode::PlainText => "已切換至純文字模式 📝".to_string(),
                ViewMode::Image { ref format } => {
                    let (name, emoji) = get_image_badge(format);
                    format!("已切換至 {} {} 預覽模式", emoji, name)
                }
            });
        }

        // Ctrl + Shift + C: 複製全文
        if input.modifiers.command && input.modifiers.shift && input.key_pressed(egui::Key::C) {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(self.content.clone());
                self.set_toast("已複製全文至剪貼簿 📋".to_string());
            }
        }

        // Ctrl + + / Ctrl + - : 縮放字體
        if input.modifiers.command
            && (input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals))
        {
            self.font_scale = (self.font_scale + 0.1).min(2.0);
        }
        if input.modifiers.command && input.key_pressed(egui::Key::Minus) {
            self.font_scale = (self.font_scale - 0.1).max(0.6);
        }
        if input.modifiers.command && input.key_pressed(egui::Key::Num0) {
            self.font_scale = 1.0;
        }

        // Ctrl + P: 置頂切換
        if input.modifiers.command && input.key_pressed(egui::Key::P) {
            self.always_on_top = !self.always_on_top;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            self.set_toast(if self.always_on_top {
                "視窗置頂: 已開啟 📌".to_string()
            } else {
                "視窗置頂: 已關閉".to_string()
            });
        }
    }
}
