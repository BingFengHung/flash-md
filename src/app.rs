mod documents;
mod shortcuts;
mod updates;

use crate::config::{AppConfig, SaveMode};
use crate::document::{DocumentKind, PendingAction, UnsavedChoice};
use crate::explorer::{hide_app_window, show_and_focus_app_window};
use crate::files::{load_document, LoadedDocument};
use crate::hotkey::HotkeyEvent;
use crate::markdown::{
    get_image_badge, get_language_badge, is_code_extension, render_code_viewer, MarkdownRenderer,
};
use crate::theme::{setup_system_cjk_fonts, AppTheme};
use crate::tray::TrayMenuAction;
use crate::updater::{
    check_latest_release, perform_self_update, restart_with_new_version, ReleaseInfo, UpdateEvent,
    CURRENT_VERSION,
};
use crate::views::status_bar::render_nav_button;
use crate::watcher::{FileWatcher, WatcherEvent};
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{
    Align, Color32, Context, FontId, Frame, Layout, Margin, RichText, Rounding, ScrollArea, Stroke,
    TextEdit, Vec2,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Markdown,
    Mindmap,
    Code { lang: String },
    PlainText,
    Table { separator: char },
    Image { format: String },
}

pub struct MdPreviewApp {
    pub document_kind: DocumentKind,
    pub pending_action: Option<PendingAction>,
    pub close_confirmed: bool,
    pub content_revision: u64,
    pub search_match_count: usize,
    pub search_jump_requested: bool,
    pub siblings: Vec<PathBuf>,
    pub current_file: Option<PathBuf>,
    pub watched_file: Option<PathBuf>,
    pub content: String,
    pub original_content: String,
    pub is_modified: bool,
    pub is_editing: bool,
    pub last_edit_instant: Option<std::time::Instant>,
    pub settings_open: bool,
    pub config: AppConfig,

    pub file_size_str: String,
    pub line_count: usize,
    pub last_modified_str: String,
    pub view_mode: ViewMode,

    pub image_uri: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub image_zoom: f32,
    pub image_fit_mode: bool,

    pub theme: AppTheme,
    pub font_scale: f32,
    pub always_on_top: bool,
    pub visible: bool,
    pub is_standalone: bool,

    pub search_open: bool,
    pub search_query: String,
    pub search_focus_requested: bool,
    pub search_match_index: usize,
    pub target_scroll_offset: Option<f32>,
    pub target_anchor: Option<String>,

    pub toc_open: bool,

    pub available_update: Option<ReleaseInfo>,
    pub is_updating: bool,
    pub update_tx: Sender<UpdateEvent>,
    pub update_rx: Receiver<UpdateEvent>,

    pub file_watcher: FileWatcher,
    pub hotkey_rx: Receiver<HotkeyEvent>,
    pub watcher_rx: Receiver<WatcherEvent>,
    pub tray_rx: Receiver<TrayMenuAction>,
    pub ctx_holder: Arc<Mutex<Option<Context>>>,

    pub status_toast: Option<(String, std::time::Instant)>,
    pub reset_scroll_to_top: bool,
    pub keyboard_scroll_delta: f32,
    pub current_scroll_offset: f32,
    pub reading_progress: f32,
    pub is_ime_composing: bool,
    pub last_ime_activity: Option<std::time::Instant>,
    pub is_slides_mode: bool,
    pub current_slide_index: usize,
    pub is_slides_fullscreen: bool,
    pub mindmap_state: crate::views::mindmap::MindmapState,
    pub mindmap_root: Option<crate::views::mindmap::MindmapNode>,
}

impl MdPreviewApp {
    #[allow(clippy::too_many_arguments)]
    fn empty(
        config: AppConfig,
        is_standalone: bool,
        visible: bool,
        file_watcher: FileWatcher,
        hotkey_rx: Receiver<HotkeyEvent>,
        watcher_rx: Receiver<WatcherEvent>,
        tray_rx: Receiver<TrayMenuAction>,
        ctx_holder: Arc<Mutex<Option<Context>>>,
    ) -> Self {
        let theme = config.theme;
        let font_scale = config.font_scale;
        let always_on_top = config.always_on_top;
        let (update_tx, update_rx) = unbounded();
        Self {
            document_kind: DocumentKind::Text,
            pending_action: None,
            close_confirmed: false,
            content_revision: 0,
            search_match_count: 0,
            search_jump_requested: false,
            siblings: Vec::new(),
            current_file: None,
            watched_file: None,
            content: String::new(),
            original_content: String::new(),
            is_modified: false,
            is_editing: false,
            last_edit_instant: None,
            settings_open: false,
            config,
            file_size_str: String::new(),
            line_count: 0,
            last_modified_str: String::new(),
            view_mode: ViewMode::Markdown,
            image_uri: None,
            image_bytes: None,
            image_zoom: 1.0,
            image_fit_mode: true,
            theme,
            font_scale,
            always_on_top,
            visible,
            is_standalone,
            search_open: false,
            search_query: String::new(),
            search_focus_requested: false,
            search_match_index: 0,
            target_scroll_offset: None,
            target_anchor: None,
            toc_open: false,
            available_update: None,
            is_updating: false,
            update_tx: update_tx.clone(),
            update_rx,
            file_watcher,
            hotkey_rx,
            watcher_rx,
            tray_rx,
            ctx_holder: ctx_holder.clone(),
            status_toast: None,
            reset_scroll_to_top: false,
            keyboard_scroll_delta: 0.0,
            current_scroll_offset: 0.0,
            reading_progress: 0.0,
            is_ime_composing: false,
            last_ime_activity: None,
            is_slides_mode: false,
            current_slide_index: 0,
            is_slides_fullscreen: false,
            mindmap_state: Default::default(),
            mindmap_root: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_file: Option<PathBuf>,
        is_standalone: bool,
        hotkey_rx: Receiver<HotkeyEvent>,
        watcher_rx: Receiver<WatcherEvent>,
        tray_rx: Receiver<TrayMenuAction>,
        file_watcher: FileWatcher,
        ctx_holder: Arc<Mutex<Option<Context>>>,
    ) -> Self {
        // 註冊 egui Context 到全域 holder，供快捷鍵與系統匣隨時喚醒
        if let Ok(mut guard) = ctx_holder.lock() {
            *guard = Some(cc.egui_ctx.clone());
        }

        // 安裝 egui_extras 內建的所有圖片與 SVG 向量圖載入器
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // 載入 Windows 繁體中文與 Emoji 系統字型 (徹底解決方塊字問題)
        setup_system_cjk_fonts(&cc.egui_ctx);

        let config = AppConfig::load();
        let theme = config.theme;
        theme.apply_to_ctx(&cc.egui_ctx);
        let always_on_top = config.always_on_top;
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::WindowLevel(if always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));

        let is_visible = initial_file.is_some() || is_standalone;
        let mut app = Self::empty(
            config,
            is_standalone,
            is_visible,
            file_watcher,
            hotkey_rx,
            watcher_rx,
            tray_rx,
            ctx_holder.clone(),
        );

        if !is_visible {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
            hide_app_window();
        }

        // 啟動時在背景默默檢查是否有新版本發布
        let bg_tx = app.update_tx.clone();
        let bg_ctx_holder = ctx_holder.clone();
        thread::spawn(move || {
            let rel = check_latest_release();
            let _ = bg_tx.send(UpdateEvent::Checked(rel));
            {
                if let Ok(guard) = bg_ctx_holder.lock() {
                    if let Some(ref ctx) = *guard {
                        ctx.request_repaint();
                    }
                }
            }
        });

        if let Some(file) = initial_file {
            app.load_file(&file);
        }

        app
    }

    /// 安全切換全螢幕狀態並強制維護 Windows 前景層級 (避免 Windows 樣式轉移時視窗掉落至檔案總管背後)
    pub fn set_fullscreen_state(&mut self, ctx: &egui::Context, fullscreen: bool) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        show_and_focus_app_window();
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(30));
            show_and_focus_app_window();
            std::thread::sleep(std::time::Duration::from_millis(100));
            show_and_focus_app_window();
            std::thread::sleep(std::time::Duration::from_millis(250));
            show_and_focus_app_window();
        });
        ctx.request_repaint();
    }

    pub fn set_toast(&mut self, msg: String) {
        self.status_toast = Some((msg, std::time::Instant::now()));
    }

    /// 切換至同目錄下的上一個 / 下一個檔案 (依檔名自然排序)
    /// 渲染 Markdown 目錄大綱，回傳收起狀態與目標錨點。
    pub fn render_toc_sidebar(&self, ui: &mut egui::Ui) -> (bool, Option<String>) {
        crate::views::toc_sidebar::render_toc_sidebar(
            ui,
            self.theme,
            self.font_scale,
            &self.content,
        )
    }
}

impl eframe::App for MdPreviewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 背景常駐未顯示時低頻輪詢，視窗顯現時由使用者操作與事件驅動，達成 0% CPU 靜止待機
        if !self.visible {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // 確保 context holder 隨時保持最新
        if let Ok(mut guard) = self.ctx_holder.lock() {
            if guard.is_none() {
                *guard = Some(ctx.clone());
            }
        }

        while let Ok(event) = self.update_rx.try_recv() {
            self.handle_update_event(event);
        }

        // 處理全域快捷鍵事件
        while let Ok(event) = self.hotkey_rx.try_recv() {
            match event {
                HotkeyEvent::TriggerPreviewWithFile(maybe_path) => {
                    self.handle_hotkey_preview(maybe_path);
                    ctx.request_repaint();
                }
            }
        }

        // 處理檔案監視變更事件
        while let Ok(event) = self.watcher_rx.try_recv() {
            match event {
                WatcherEvent::DirectoryChanged => {
                    ctx.request_repaint();
                    self.siblings = self
                        .current_file
                        .as_deref()
                        .map(crate::files::sibling_files)
                        .unwrap_or_default();
                }
                WatcherEvent::FileChanged(path) => {
                    if self.watched_file.as_deref() == Some(&path) {
                        self.reload_current_file();
                        ctx.request_repaint();
                    }
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) && !self.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_action(PendingAction::Close);
        }

        // 處理系統匣選單事件
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                TrayMenuAction::OpenFile => {
                    self.open_file_dialog();
                }
                TrayMenuAction::ToggleTheme => {
                    self.theme.toggle();
                    self.theme.apply_to_ctx(ctx);
                }
                TrayMenuAction::ToggleAlwaysOnTop => {
                    self.always_on_top = !self.always_on_top;
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        if self.always_on_top {
                            egui::WindowLevel::AlwaysOnTop
                        } else {
                            egui::WindowLevel::Normal
                        },
                    ));
                }
                TrayMenuAction::CheckUpdate => {
                    self.check_update_manually();
                    self.visible = true;
                    show_and_focus_app_window();
                }
                TrayMenuAction::About => {
                    self.set_toast(format!(
                        "flash-md v{} - 快捷鍵 Alt+Space 閃電預覽 ⚡",
                        CURRENT_VERSION
                    ));
                    self.visible = true;
                    show_and_focus_app_window();
                }
                TrayMenuAction::Exit => {
                    self.request_action(PendingAction::Exit);
                }
            }
        }

        if self.pending_action.is_some() {
            self.render_unsaved_dialog(ctx);
            return;
        }

        if self.is_updating {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("正在下載並安裝更新…");
                ui.spinner();
            });
            return;
        }

        // 自動防抖保存檢查 (打字停止 800ms 後自動寫回檔案)
        if self.config.save_mode == SaveMode::AutoDebounce && self.is_modified {
            if let Some(instant) = self.last_edit_instant {
                if instant.elapsed() >= Duration::from_millis(800) {
                    self.save_current_file(true);
                    self.last_edit_instant = None;
                }
            }
        }

        // IME (注音/拼音/日文輸入法) 組字與選字確認 Enter 防誤換行過濾：
        // 1. 偵測 egui::Event::Ime(Preedit / Commit) 與 Event::Text (含 CJK 漢字或注音符號)
        // 2. 當處於 IME 組字中 (Preedit) 或剛進行組字/選字 (400ms 內) 時，
        //    Windows 會發送 Key::Enter 與 Text("\n") 來結束組字或確認候選字。
        // 3. 自動自 i.events 與 i.keys_down 中徹底吞噬該次 Enter，防止編輯器直接換行！
        // 4. 組字確認後，使用者再次按下 Enter 即可正常進行段落換行，英數模式輸入亦完全不受影響。
        let now = std::time::Instant::now();
        let was_recent_ime = if let Some(instant) = self.last_ime_activity {
            instant.elapsed() < Duration::from_millis(400)
        } else {
            false
        };

        let mut ime_event_this_frame = false;
        let mut enter_was_swallowed = false;

        ctx.input_mut(|i| {
            for ev in &i.events {
                match ev {
                    egui::Event::Ime(egui::ImeEvent::Preedit(s)) => {
                        ime_event_this_frame = true;
                        self.is_ime_composing = !s.is_empty();
                    }
                    egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                        ime_event_this_frame = true;
                        self.is_ime_composing = false;
                    }
                    egui::Event::Ime(egui::ImeEvent::Disabled) => {
                        self.is_ime_composing = false;
                    }
                    egui::Event::Text(ref s) => {
                        // 偵測是否包含 CJK 漢字、注音符號或非 ASCII 輸入法字元
                        if s.chars().any(|c| c >= '\u{2E80}') {
                            ime_event_this_frame = true;
                        }
                    }
                    _ => {}
                }
            }

            if ime_event_this_frame {
                self.last_ime_activity = Some(now);
            }

            let should_filter_enter =
                self.is_ime_composing || was_recent_ime || ime_event_this_frame;

            if should_filter_enter {
                let mut found_enter = false;
                i.events.retain(|ev| match ev {
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        ..
                    } => {
                        found_enter = true;
                        false
                    }
                    egui::Event::Text(s) if s == "\n" || s == "\r" || s == "\r\n" => {
                        found_enter = true;
                        false
                    }
                    _ => true,
                });
                if found_enter || i.keys_down.contains(&egui::Key::Enter) {
                    enter_was_swallowed = true;
                    i.keys_down.remove(&egui::Key::Enter);
                }
            }
        });

        if enter_was_swallowed {
            // 已成功吞噬組字確認 Enter，重置計時器，使下一次 Enter 能正常進行段落換行
            self.last_ime_activity = None;
            self.is_ime_composing = false;
        } else {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Backspace) || i.pointer.any_click() {
                    self.last_ime_activity = None;
                    self.is_ime_composing = false;
                }
            });
        }

        // 快捷鍵監聽
        self.handle_shortcuts(ctx);

        // 如果視窗處於隱藏狀態，則確保 OS 視窗不顯現並直接 return 節省資源
        if !self.visible && !self.is_standalone {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            return;
        }

        // 頂部新版本升級橫幅 (若有新版本)
        let mut dismiss_update = false;
        let mut do_self_update = false;
        if let Some(ref release) = self.available_update {
            let release_tag = release.tag_name.clone();
            let release_notes = release.changelog.clone();
            let is_updating = self.is_updating;

            let banner_bg = match self.theme {
                AppTheme::Dark => Color32::from_rgb(20, 30, 48), // 質感暗夜深藍底
                AppTheme::Light => Color32::from_rgb(238, 246, 255), // 清爽透亮淺藍底
            };
            let banner_border = match self.theme {
                AppTheme::Dark => Color32::from_rgb(56, 189, 248), // 科技青藍
                AppTheme::Light => Color32::from_rgb(186, 230, 253), // 柔和淺天藍
            };
            let text_color = match self.theme {
                AppTheme::Dark => Color32::from_rgb(224, 242, 254), // 明亮淺白藍
                AppTheme::Light => Color32::from_rgb(12, 74, 110),  // 高對比深海軍藍 (極度清晰可讀)
            };
            let btn_primary_bg = match self.theme {
                AppTheme::Dark => Color32::from_rgb(14, 165, 233), // 亮天藍
                AppTheme::Light => Color32::from_rgb(2, 132, 199), // 深天藍
            };
            let btn_dismiss_bg = match self.theme {
                AppTheme::Dark => Color32::from_rgb(30, 41, 59),
                AppTheme::Light => Color32::from_rgb(255, 255, 255),
            };

            egui::TopBottomPanel::top("update_banner")
                .frame(
                    Frame::none()
                        .fill(banner_bg)
                        .stroke(Stroke::new(1.0_f32, banner_border))
                        .inner_margin(Margin::symmetric(16.0, 7.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let banner_text = if is_updating {
                            format!(
                                "⏳ 正在自動下載升級至 {} 並無縫重啟，請稍候...",
                                release_tag
                            )
                        } else {
                            format!(
                                "🎉 發現全新版本 {} (目前為 v{})！",
                                release_tag, CURRENT_VERSION
                            )
                        };

                        ui.label(
                            RichText::new(banner_text)
                                .color(text_color)
                                .strong()
                                .size(12.5),
                        )
                        .on_hover_text(&release_notes);

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !is_updating {
                                let dismiss_btn = egui::Button::new(
                                    RichText::new("✕ 稍後")
                                        .size(11.5)
                                        .color(self.theme.text_secondary()),
                                )
                                .fill(btn_dismiss_bg)
                                .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                                .rounding(Rounding::same(5.0));

                                if ui.add(dismiss_btn).clicked() {
                                    dismiss_update = true;
                                }

                                let upgrade_btn = egui::Button::new(
                                    RichText::new(" 🚀 一鍵自動升級 ")
                                        .strong()
                                        .size(12.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(btn_primary_bg)
                                .stroke(Stroke::NONE)
                                .rounding(Rounding::same(5.0));

                                if ui.add(upgrade_btn).clicked() {
                                    do_self_update = true;
                                }
                            } else {
                                ui.label(
                                    RichText::new("⚡ 即時熱替換中...")
                                        .size(11.5)
                                        .strong()
                                        .color(text_color),
                                );
                            }
                        });
                    });
                });
        }

        if dismiss_update {
            self.available_update = None;
        }
        if do_self_update {
            self.trigger_self_update();
        }

        // 頂部現代精緻導航列 (Fluent / macOS 玻璃質感風格，簡報模式下自動隱藏以保持沉浸全螢幕)
        if !self.is_slides_mode {
            egui::TopBottomPanel::top("top_header")
                .frame(
                    Frame::none()
                        .fill(self.theme.card_bg_color())
                    .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                    .inner_margin(Margin::symmetric(14.0, 7.0)),
            )
            .show(ctx, |ui| {
                // 第一階：品牌徽章、檔案切換導航、檔案名稱、檢視模式與檔案屬性資訊
                ui.horizontal(|ui| {
                    // 左側：精緻品牌徽章 (高對比度配色)
                    let (badge_bg, badge_border, badge_fg) = match self.theme {
                        AppTheme::Dark => (
                            Color32::from_rgb(18, 38, 58),
                            Color32::from_rgb(56, 189, 248),
                            Color32::from_rgb(56, 189, 248),
                        ),
                        AppTheme::Light => (
                            Color32::from_rgb(224, 242, 254),
                            Color32::from_rgb(186, 230, 253),
                            Color32::from_rgb(3, 105, 161),
                        ),
                    };

                    Frame::none()
                        .fill(badge_bg)
                        .rounding(Rounding::same(5.0))
                        .stroke(Stroke::new(1.0_f32, badge_border))
                        .inner_margin(Margin::symmetric(7.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("⚡ flash-md")
                                    .size(11.5)
                                    .strong()
                                    .color(badge_fg),
                            );
                        });

                    ui.add_space(4.0);

                    // ◀ 上一個檔案按鈕
                    if self.current_file.is_some() {
                        let prev_resp = ui.add(
                            egui::Button::new(RichText::new("◀").size(11.0).color(self.theme.text_secondary()))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                        );
                        if prev_resp.on_hover_text("上一個檔案 (←)").clicked() {
                            self.navigate_sibling_file(false);
                        }
                    }

                    let file_name = self
                        .current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("未開啟檔案");

                    let title_resp = ui.add(
                        egui::Button::new(
                            RichText::new(file_name)
                                .strong()
                                .size(13.0)
                                .color(self.theme.text_primary()),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(4.0)),
                    );

                    if title_resp.clicked() {
                        if let Some(ref path) = self.current_file {
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(path.to_string_lossy().to_string());
                                self.set_toast("已複製檔案完整路徑 📁".to_string());
                            }
                        }
                    }

                    if title_resp.hovered() {
                        if let Some(ref path) = self.current_file {
                            title_resp.on_hover_text(format!("完整路徑:\n{:?}\n(點擊複製路徑)", path));
                        }
                    }

                    // ▶ 下一個檔案按鈕
                    if self.current_file.is_some() {
                        let next_resp = ui.add(
                            egui::Button::new(RichText::new("▶").size(11.0).color(self.theme.text_secondary()))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                        );
                        if next_resp.on_hover_text("下一個檔案 (→)").clicked() {
                            self.navigate_sibling_file(true);
                        }
                    }

                    // 模式切換膠囊 (支援 Markdown / 語言語法高亮 / 斑馬紋表格 / 純文字 / 圖片向量圖)
                    if !self.content.is_empty() || self.image_uri.is_some() {
                        let (badge_text, badge_tip) = match self.view_mode {
                            ViewMode::Markdown => ("📄 Markdown".to_string(), "目前為 Markdown 模式 (點擊切換 Ctrl+M)".to_string()),
                            ViewMode::Mindmap => ("🧠 心智圖".to_string(), "目前為互動心智圖模式 (點擊切換 Ctrl+M)".to_string()),
                            ViewMode::Table { separator } => {
                                if separator == '\t' {
                                    ("📊 TSV 表格".to_string(), "目前為 TSV 資料表格模式 (點擊切換 Ctrl+M)".to_string())
                                } else {
                                    ("📊 CSV 表格".to_string(), "目前為 CSV 資料表格模式 (點擊切換 Ctrl+M)".to_string())
                                }
                            }
                            ViewMode::Code { ref lang } => {
                                let (name, emoji) = get_language_badge(lang);
                                (format!("{} {}", emoji, name), format!("目前為 {} 語法高亮 (點擊切換 Ctrl+M)", name))
                            }
                            ViewMode::PlainText => ("📝 純文字".to_string(), "目前為純文字模式 (點擊切換 Ctrl+M)".to_string()),
                            ViewMode::Image { ref format } => {
                                let (name, emoji) = get_image_badge(format);
                                (format!("{} {}", emoji, name), format!("目前為 {} 預覽 (點擊切換 Ctrl+M)", name))
                            }
                        };

                        let mode_btn = ui.add(
                            egui::Button::new(
                                RichText::new(badge_text)
                                    .size(11.0)
                                    .color(self.theme.accent_color()),
                            )
                            .fill(self.theme.code_bg_color())
                            .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                            .rounding(Rounding::same(5.0)),
                        );

                        if mode_btn.clicked() {
                            let ext = self
                                .current_file
                                .as_ref()
                                .and_then(|p| p.extension())
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();

                            self.view_mode = match self.view_mode {
                                ViewMode::Markdown => {
                                    if matches!(ext.as_str(), "md" | "markdown" | "mdown" | "mkdn") || (!self.content.is_empty() && self.content.lines().any(|l| l.trim().starts_with('#'))) {
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
                                ViewMode::Table { separator } => {
                                    ViewMode::Code { lang: if separator == '\t' { "tsv".to_string() } else { "csv".to_string() } }
                                }
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
                                        ViewMode::Code { lang: "xml".to_string() }
                                    } else {
                                        ViewMode::PlainText
                                    }
                                }
                            };
                            self.reset_scroll_to_top = true;
                            self.current_scroll_offset = 0.0_f32;
                        }
                        if mode_btn.hovered() {
                            mode_btn.on_hover_text(badge_tip);
                        }
                    }

                    // 第一階右側：檔案屬性標籤 (同目錄序號、行數/尺寸、大小、修改時間)
                    if !self.content.is_empty() || self.image_uri.is_some() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            Frame::none()
                                .fill(self.theme.code_bg_color())
                                .rounding(Rounding::same(4.0))
                                .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                                .inner_margin(Margin::symmetric(6.0, 3.0))
                                .show(ui, |ui| {
                                    let sibling_str = self
                                        .get_sibling_info()
                                        .map(|(cur, total)| format!("[{}/{}]  •  ", cur, total))
                                        .unwrap_or_default();

                                    let (info_text, tooltip_text) = if self.is_editing {
                                        let save_status = if self.is_modified {
                                            "● 未保存 (Ctrl+S)"
                                        } else {
                                            "✓ 已保存"
                                        };
                                        (
                                            format!("{}{}  •  {} 行  •  {}", sibling_str, save_status, self.line_count, self.file_size_str),
                                            format!("✏ 就地編輯模式\n• 儲存狀態: {}\n• 總行數: {} 行\n• 保存模式: {}", if self.is_modified { "已修改未保存" } else { "已保存" }, self.line_count, match self.config.save_mode { SaveMode::Manual => "按 Ctrl+S 手動保存", SaveMode::AutoDebounce => "打字停止自動防抖保存" }),
                                        )
                                    } else if let ViewMode::Image { ref format } = self.view_mode {
                                        (
                                            format!("{}{format_upper}  •  {}  •  {}", sibling_str, self.file_size_str, self.last_modified_str, format_upper = format.to_uppercase()),
                                            format!("🖼 圖片資訊\n• 格式: {}\n• 檔案大小: {}\n• 修改時間: {}", format.to_uppercase(), self.file_size_str, self.last_modified_str),
                                        )
                                    } else if matches!(self.view_mode, ViewMode::Markdown) {
                                        let stats = crate::parsers::cached_stats(ctx, &self.content);
                                        let words_str = if stats.cjk_chars > 0 && stats.words > 0 {
                                            format!("{} 中文 / {} 字", stats.cjk_chars, stats.words)
                                        } else if stats.cjk_chars > 0 {
                                            format!("{} 字", stats.cjk_chars)
                                        } else {
                                            format!("{} 詞", stats.words)
                                        };
                                        (
                                            format!("{}{} 行  •  {}  •  ⏱ {} 分鐘  •  {}  •  {}", sibling_str, self.line_count, words_str, stats.reading_time_mins, self.file_size_str, self.last_modified_str),
                                            format!("📊 文本統計資訊\n• 總行數: {} 行\n• 中文字數 (CJK): {} 字\n• 英文字數 (Words): {} 詞\n• 總字元數 (不含空白): {} 字元\n• 預估閱讀時間: 約 {} 分鐘 (中速 350 字/分)\n• 檔案大小: {}\n• 修改時間: {}", self.line_count, stats.cjk_chars, stats.words, stats.total_chars, stats.reading_time_mins, self.file_size_str, self.last_modified_str),
                                        )
                                    } else {
                                        (
                                            format!("{}{} 行  •  {}  •  {}", sibling_str, self.line_count, self.file_size_str, self.last_modified_str),
                                            format!("📄 檔案資訊\n• 總行數: {} 行\n• 檔案大小: {}\n• 修改時間: {}", self.line_count, self.file_size_str, self.last_modified_str),
                                        )
                                    };

                                    ui.label(
                                        RichText::new(info_text)
                                            .size(10.5)
                                            .color(self.theme.text_secondary()),
                                    ).on_hover_text(tooltip_text);
                                });
                        });
                    }
                });

                ui.add_space(5.0);

                // 第二階：現代精緻功能工具按鈕列
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    // 就地編輯模式切換按鈕
                    let edit_btn_label = if self.is_editing { "👁 預覽" } else { "✏ 編輯" };
                    let edit_btn_tip = if self.is_editing {
                        "切換回美化預覽模式 (E 或 Esc)"
                    } else {
                        "切換為全螢幕就地編輯模式 (E 或 Ctrl+E)"
                    };
                    if render_nav_button(ui, self.theme, edit_btn_label, self.is_editing, edit_btn_tip).clicked() {
                        self.toggle_edit_mode();
                    }

                    // 保存按鈕 (編輯中或有修改時可用)
                    if self.is_editing || self.is_modified {
                        let save_label = if self.is_modified { "💾 保存 *" } else { "💾 保存" };
                        let save_tip = if self.is_modified { "檔案已修改，點擊或按 Ctrl + S 保存" } else { "檔案已保存 (Ctrl + S)" };
                        if render_nav_button(ui, self.theme, save_label, self.is_modified, save_tip).clicked() {
                            self.save_current_file(false);
                        }
                    }

                    // 開啟檔案按鈕
                    if render_nav_button(ui, self.theme, "📂 開啟", false, "開啟本機 Markdown、程式碼或圖片檔案").clicked() {
                        self.open_file_dialog();
                    }

                    // 搜尋按鈕 (僅文字/程式碼模式可用)
                    if !matches!(self.view_mode, ViewMode::Image { .. }) {
                        if render_nav_button(ui, self.theme, "🔍 搜尋", self.search_open, "搜尋關鍵字 (Ctrl + F 或 /)").clicked() {
                            self.search_open = !self.search_open;
                            if self.search_open {
                                self.search_focus_requested = true;
                            }
                        }
                    }

                    // Markdown 大綱側邊欄開關按鈕
                    if matches!(self.view_mode, ViewMode::Markdown) {
                        if render_nav_button(ui, self.theme, "📑 大綱", self.toc_open, "開啟/收起章節目錄大綱 (Ctrl + T)").clicked() {
                            self.toc_open = !self.toc_open;
                        }
                    }

                    // Markdown 簡報投影模式切換按鈕
                    if matches!(self.view_mode, ViewMode::Markdown) && !self.is_editing {
                        if render_nav_button(ui, self.theme, "📽 簡報", self.is_slides_mode, "切換全螢幕簡報投影模式 (F5 或 P)").clicked() {
                            self.is_slides_mode = !self.is_slides_mode;
                            if self.is_slides_mode {
                                self.current_slide_index = 0;
                                self.is_slides_fullscreen = true;
                                self.set_fullscreen_state(ctx, true);
                                self.set_toast("📽 已進入全螢幕簡報投影模式 (F5/Esc 退出，左右鍵翻頁)".to_string());
                            } else {
                                if self.is_slides_fullscreen {
                                    self.is_slides_fullscreen = false;
                                    self.set_fullscreen_state(ctx, false);
                                }
                                self.set_toast("👁 已退出簡報投影模式".to_string());
                            }
                        }
                    }

                    // Markdown 互動心智圖切換按鈕
                    if matches!(self.view_mode, ViewMode::Markdown | ViewMode::Mindmap) && !self.is_editing && !self.content.is_empty() {
                        let is_mindmap = matches!(self.view_mode, ViewMode::Mindmap);
                        if render_nav_button(ui, self.theme, "🧠 心智圖", is_mindmap, "切換 Markdown 互動心智圖模式 (Ctrl + M)").clicked() {
                            if is_mindmap {
                                self.view_mode = ViewMode::Markdown;
                                self.set_toast("已切換回 Markdown 渲染模式 📄".to_string());
                            } else {
                                self.view_mode = ViewMode::Mindmap;
                                self.set_toast("已切換至 🧠 互動心智圖模式".to_string());
                            }
                        }
                    }

                    // JSON 格式化與壓縮按鈕
                    let current_ext = self.current_file.as_ref()
                        .and_then(|p| p.extension())
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();

                    if matches!(current_ext.as_str(), "json" | "jsonc" | "json5" | "jsonl") {
                        if render_nav_button(ui, self.theme, "⚡ 格式化", false, "一鍵排版美化 JSON (縮排對齊)").clicked() {
                            self.format_json_content();
                        }
                        if render_nav_button(ui, self.theme, "📦 壓縮", false, "一鍵壓縮為單行 JSON (去除空白與換行)").clicked() {
                            self.minify_json_content();
                        }
                    }

                    // 在檔案總管中定位按鈕
                    if render_nav_button(ui, self.theme, "📁 定位", false, "在 Windows 檔案總管中高亮選取目前檔案 (Ctrl + Shift + O)").clicked() {
                        self.locate_current_file_in_explorer();
                    }

                    // 複製全文 / 複製路徑按鈕
                    if render_nav_button(ui, self.theme, "📋 複製", false, "複製檔案內容或路徑 (Ctrl + Shift + C)").clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let ViewMode::Image { .. } = self.view_mode {
                                if let Some(ref path) = self.current_file {
                                    let _ = clipboard.set_text(path.to_string_lossy().to_string());
                                    self.set_toast("已複製圖片檔案路徑 📋".to_string());
                                }
                            } else {
                                let _ = clipboard.set_text(self.content.clone());
                                self.set_toast("已複製全文至剪貼簿 📋".to_string());
                            }
                        }
                    }

                    // 外部編輯器開啟
                    if render_nav_button(ui, self.theme, "↗ 編輯器", false, "在系統預設編輯器中開啟 (Ctrl + O)").clicked() {
                        if let Some(ref path) = self.current_file {
                            let _ = open::that(path);
                        }
                    }

                    // 檢查更新按鈕
                    if render_nav_button(ui, self.theme, "🔄 更新", false, "檢查 GitHub 最新版本").clicked() {
                        self.check_update_manually();
                    }

                    // 第二階右側：視窗控制、設定與主題切換
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;

                        // 關閉按鈕
                        if render_nav_button(ui, self.theme, "✕ 關閉", false, "隱藏預覽視窗 (Esc)").clicked() {
                            self.visible = false;
                            hide_app_window();
                            if self.is_standalone {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }

                        // 全螢幕 / 視窗切換按鈕 (F11)
                        let is_fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                        let fs_label = if is_fs { "🗗 視窗" } else { "⛶ 全螢幕" };
                        if render_nav_button(ui, self.theme, fs_label, is_fs, "切換全螢幕模式 (F11)").clicked() {
                            let next_fs = !is_fs;
                            self.set_fullscreen_state(ctx, next_fs);
                            self.set_toast(if next_fs { "⛶ 已進入全螢幕模式 (F11 退出)".to_string() } else { "🗗 已退出全螢幕模式".to_string() });
                        }

                        // 偏好設定按鈕
                        if render_nav_button(ui, self.theme, "⚙ 設定", self.settings_open, "偏好設定 (亮/暗色主題、自動防抖保存、字型縮放)").clicked() {
                            self.settings_open = !self.settings_open;
                        }

                        // 置頂狀態按鈕
                        let pin_btn = render_nav_button(
                            ui,
                            self.theme,
                            if self.always_on_top { "📌 置頂中" } else { "📌 置頂" },
                            self.always_on_top,
                            "切換視窗置頂 (Ctrl + P)",
                        );
                        if pin_btn.clicked() {
                            self.always_on_top = !self.always_on_top;
                            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                if self.always_on_top {
                                    egui::WindowLevel::AlwaysOnTop
                                } else {
                                    egui::WindowLevel::Normal
                                },
                            ));
                            self.config.always_on_top = self.always_on_top;
                            self.config.save();
                        }

                        // 主題切換按鈕 (使用同字元家族的 🔆 與 🌙 保持一致的字圖間距)
                        let (theme_icon, theme_tip) = match self.theme {
                            AppTheme::Dark => ("🔆 淺色", "切換為淺色主題並保存偏好"),
                            AppTheme::Light => ("🌙 深色", "切換為深色主題並保存偏好"),
                        };
                        if render_nav_button(ui, self.theme, theme_icon, false, theme_tip).clicked() {
                            self.theme.toggle();
                            self.theme.apply_to_ctx(ctx);
                            self.config.theme = self.theme;
                            self.config.save();
                        }
                    });
                });

                // 搜尋列展開區 (Ctrl + F / F3)
                if self.search_open && !matches!(self.view_mode, ViewMode::Image { .. }) {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍 尋找內文:").size(12.5).color(self.theme.accent_color()).strong());
                        let search_input_resp = ui.add(
                            TextEdit::singleline(&mut self.search_query)
                                .hint_text("輸入關鍵字 (Enter 下一筆, Shift+Enter 上一筆)...")
                                .desired_width(260.0),
                        );

                        if self.search_focus_requested {
                            search_input_resp.request_focus();
                            self.search_focus_requested = false;
                        }

                        let match_count = self.search_match_count;

                        // 當搜尋字串變更時，自動跳轉至第一筆相符項目
                        if search_input_resp.changed() {
                            self.search_match_index = 0;
                            self.search_jump_requested = true;
                        }

                        // 在搜尋框內按下 Enter 或 Shift + Enter 進行上一筆/下一筆跳轉
                        if search_input_resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let shift = ui.input(|i| i.modifiers.shift);
                            self.navigate_search_match(!shift);
                        }

                        // 搜尋列開啟但焦點不在輸入框時，提供 Vim 風格的 n/N 導航。
                        // 輸入框取得焦點時不攔截，讓使用者仍能正常輸入 n 或 N。
                        if !search_input_resp.has_focus()
                            && ui.input(|i| {
                                i.key_pressed(egui::Key::N)
                                    && !i.modifiers.command
                                    && !i.modifiers.alt
                            })
                        {
                            let shift = ui.input(|i| i.modifiers.shift);
                            self.navigate_search_match(!shift);
                        }

                        let query_clean = self.search_query.trim();
                        if !query_clean.is_empty() {
                            let (count_text, count_color) = if match_count > 0 {
                                (
                                    format!("第 {} / {} 筆", self.search_match_index.min(match_count.saturating_sub(1)) + 1, match_count),
                                    self.theme.accent_color(),
                                )
                            } else {
                                ("無相符項目".to_string(), self.theme.text_secondary())
                            };

                            ui.label(
                                RichText::new(count_text)
                                    .size(11.5)
                                    .color(count_color)
                                    .strong(),
                            );

                            if match_count > 0 {
                                if ui.button(RichText::new("▲ 上一個").size(11.0)).on_hover_text("上一個相符項目 (Shift + Enter 或 Shift + F3)").clicked() {
                                    self.navigate_search_match(false);
                                }
                                if ui.button(RichText::new("▼ 下一個").size(11.0)).on_hover_text("下一個相符項目 (Enter 或 F3)").clicked() {
                                    self.navigate_search_match(true);
                                }
                            }
                        }

                        if ui.button(RichText::new("✕ 清除").size(11.0)).clicked() {
                            self.search_query.clear();
                            self.search_match_index = 0;
                        }
                        if ui.button(RichText::new("關閉 (Esc)").size(11.0)).clicked() {
                            self.search_open = false;
                            self.search_query.clear();
                            self.search_match_index = 0;
                        }
                    });
                }
            });
        }

        // 偏好設定彈出對話框 (委派至 views::settings_modal 模組)
        let modal_out = crate::views::settings_modal::render_settings_modal(
            ctx,
            self.settings_open,
            self.theme,
            self.config.save_mode,
            self.font_scale,
        );

        self.settings_open = modal_out.is_open;
        if let Some(t) = modal_out.new_theme {
            self.theme = t;
            self.theme.apply_to_ctx(ctx);
            self.config.theme = t;
            self.config.save();
        }
        if let Some(sm) = modal_out.new_save_mode {
            self.config.save_mode = sm;
            self.config.save();
        }
        if let Some(fs) = modal_out.new_font_scale {
            self.font_scale = fs;
            self.config.font_scale = fs;
            self.config.save();
        }

        // 底部狀態列 / Toast 提示 (簡報模式下自動隱藏以保持沉浸全螢幕)
        if !self.is_slides_mode {
            egui::TopBottomPanel::bottom("bottom_status")
                .frame(
                    Frame::none()
                        .fill(self.theme.card_bg_color())
                        .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                        .inner_margin(Margin::symmetric(16.0, 6.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if let Some((ref msg, instant)) = self.status_toast {
                            if instant.elapsed().as_secs() < 4 {
                                ui.label(
                                    RichText::new(msg)
                                        .color(self.theme.accent_color())
                                        .strong()
                                        .size(12.0),
                                );
                            } else {
                                self.render_bottom_tips(ui);
                            }
                        } else {
                            self.render_bottom_tips(ui);
                        }

                        // 右側縮放控制 (針對文字或圖片模式各自適配)
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if let ViewMode::Image { .. } = self.view_mode {
                                let zoom_str = if self.image_fit_mode {
                                    "適應視窗".to_string()
                                } else {
                                    format!("{}%", (self.image_zoom * 100.0).round() as u32)
                                };
                                ui.label(
                                    RichText::new(zoom_str)
                                        .color(self.theme.text_secondary())
                                        .size(11.5),
                                );

                                if ui
                                    .small_button(" ↔ ")
                                    .on_hover_text("自適應視窗大小")
                                    .clicked()
                                {
                                    self.image_fit_mode = true;
                                }
                                if ui
                                    .small_button(" 1:1 ")
                                    .on_hover_text("原始尺寸 100% (Ctrl + 0)")
                                    .clicked()
                                {
                                    self.image_zoom = 1.0;
                                    self.image_fit_mode = false;
                                }
                                if ui
                                    .small_button(" + ")
                                    .on_hover_text("放大 (Ctrl + +)")
                                    .clicked()
                                {
                                    self.image_zoom = (self.image_zoom * 1.2).min(10.0);
                                    self.image_fit_mode = false;
                                }
                                if ui
                                    .small_button(" − ")
                                    .on_hover_text("縮小 (Ctrl + -)")
                                    .clicked()
                                {
                                    self.image_zoom = (self.image_zoom / 1.2).max(0.1);
                                    self.image_fit_mode = false;
                                }
                            } else {
                                ui.label(
                                    RichText::new(format!(
                                        "{}%",
                                        (self.font_scale * 100.0).round() as u32
                                    ))
                                    .color(self.theme.text_secondary())
                                    .size(11.5),
                                );

                                if ui
                                    .small_button(" + ")
                                    .on_hover_text("放大字體 (Ctrl + +)")
                                    .clicked()
                                {
                                    self.font_scale = (self.font_scale + 0.1).min(2.5);
                                }
                                if ui
                                    .small_button(" − ")
                                    .on_hover_text("縮小字體 (Ctrl + -)")
                                    .clicked()
                                {
                                    self.font_scale = (self.font_scale - 0.1).max(0.6);
                                }
                                if ui
                                    .small_button(" 1:1 ")
                                    .on_hover_text("重設字體 (Ctrl + 0)")
                                    .clicked()
                                {
                                    self.font_scale = 1.0;
                                }
                            }
                        });
                    });
                });
        }

        let mut should_close_toc = false;
        let mut toc_target_anchor = None;

        // 如果開啟大綱模式且處於 Markdown 檢視，先掛載獨立可調整寬度的左側側邊欄 (SidePanel)
        if self.toc_open && matches!(self.view_mode, ViewMode::Markdown) && !self.content.is_empty()
        {
            egui::SidePanel::left("toc_side_panel")
                .resizable(true)
                .default_width(260.0 * self.font_scale)
                .min_width(180.0 * self.font_scale)
                .max_width(450.0 * self.font_scale)
                .frame(
                    Frame::none()
                        .fill(self.theme.card_bg_color())
                        .inner_margin(Margin::same(12.0 * self.font_scale))
                        .stroke(Stroke::new(1.0_f32, self.theme.border_color())),
                )
                .show(ctx, |ui| {
                    let (close, target_anchor) = self.render_toc_sidebar(ui);
                    if close {
                        should_close_toc = true;
                    }
                    if target_anchor.is_some() {
                        toc_target_anchor = target_anchor;
                    }
                });
        }

        if should_close_toc {
            self.toc_open = false;
        }
        if let Some(anchor) = toc_target_anchor {
            self.target_anchor = Some(anchor);
            ctx.request_repaint();
        }

        let panel_margin = if self.is_slides_mode {
            Margin::symmetric(0.0, 0.0)
        } else {
            Margin::symmetric(24.0, 16.0)
        };

        let previous_match_count = self.search_match_count;
        self.search_match_count = 0;

        // 主預覽渲染檢視區域 (Markdown / 全語言程式碼語法高亮 / 斑馬紋表格 / 純文字 / 圖片向量圖 / 全螢幕就地編輯)
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(self.theme.bg_color())
                    .inner_margin(panel_margin),
            )
            .show(ctx, |ui| {
                if self.is_slides_mode {
                    // 全螢幕簡報投影模式 (支援 --- 分頁、左右鍵翻頁、大字級投影卡片)
                    self.render_slides_mode(ui, ctx);
                } else if self.is_editing {
                    // 全螢幕就地編輯模式 (支援即時打字、行數統計與自動防抖/Ctrl+S保存)
                    self.render_editor(ui);
                } else if self.content.is_empty() && self.image_uri.is_none() {
                    // 極具現代質感的空狀態卡片介面 (Raycast / Linear Style)
                    self.render_empty_state(ui);
                } else {
                    let active_match_idx = if self.search_query.trim().is_empty() {
                        None
                    } else {
                        Some(self.search_match_index)
                    };

                    let mut scroll_target = self.target_scroll_offset;
                    if self.reset_scroll_to_top {
                        scroll_target = Some(0.0_f32);
                        self.current_scroll_offset = 0.0_f32;
                    }

                    match self.view_mode {
                        ViewMode::Markdown => {
                            // Markdown 富文字渲染模式 (支援即時搜尋關鍵字高亮、搜尋項目自動跳轉、滾輪重置回頂部、鍵盤方向鍵上下捲動與動態閱讀進度條)
                            let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
                            if let Some(target) = scroll_target {
                                scroll = scroll.vertical_scroll_offset(target);
                            }

                            let scroll_out = scroll.show(ui, |ui| {
                                let anchor_to_jump = self.target_anchor.clone();
                                let base_dir = self.current_file.as_ref().and_then(|p| p.parent());
                                let mut renderer = MarkdownRenderer::new(
                                    self.theme,
                                    self.font_scale,
                                    &self.search_query,
                                    active_match_idx,
                                    anchor_to_jump.as_deref(),
                                    base_dir,
                                );
                                renderer.search_jump = self.search_jump_requested;
                                let rendered = renderer.render(ui, &self.content);
                                self.search_match_count = rendered.match_count;
                                if let Some(clicked_anchor) = rendered.clicked_anchor {
                                    self.target_anchor = Some(clicked_anchor);
                                    ctx.request_repaint();
                                } else if self.target_anchor.is_some() {
                                    self.target_anchor = None;
                                }
                            });

                            // 即時同步實際滾動偏移量 (支援滑鼠滾輪與鍵盤混合無縫操作)
                            self.current_scroll_offset = scroll_out.state.offset.y;

                            let max_scroll = (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(1.0);
                            self.reading_progress = (self.current_scroll_offset / max_scroll).clamp(0.0, 1.0);

                            // 繪製頂部閱讀進度條 (位於內文區最上方)
                            if self.reading_progress > 0.002 {
                                let rect = ui.clip_rect();
                                let bar_width = rect.width() * self.reading_progress;
                                ui.painter().hline(
                                    rect.min.x..=rect.min.x + bar_width,
                                    rect.min.y,
                                    Stroke::new(2.5_f32, self.theme.accent_color()),
                                );
                            }
                        }
                        ViewMode::Table { separator } => {
                            // 現代斑馬紋資料表格模式 (支援 CSV 與 TSV 欄位解析、搜尋高亮與滾動)
                            let mut scroll = ScrollArea::both().auto_shrink([false, false]);
                            if let Some(target) = scroll_target {
                                scroll = scroll.scroll_offset(Vec2::new(0.0_f32, target));
                            }

                            let scroll_out = scroll.show(ui, |ui| {
                                let table_data = crate::parsers::cached_csv(ctx, &self.content, separator);
                                let mut match_counter = 0;
                                crate::markdown::render_csv_table(
                                    ui,
                                    self.theme,
                                    self.font_scale,
                                    &table_data,
                                    &self.search_query,
                                    active_match_idx,
                                    &mut match_counter,
                                    self.search_jump_requested,
                                );
                                self.search_match_count = match_counter;
                            });
                            self.current_scroll_offset = scroll_out.state.offset.y;
                        }
                        ViewMode::Code { ref lang } => {
                            // 程式碼全語法高亮模式 (支援行號、關鍵字高亮、縮排、即時搜尋高亮與跳轉定位、滾輪重置與鍵盤捲動)
                            let mut scroll = ScrollArea::both().auto_shrink([false, false]);
                            if let Some(target) = scroll_target {
                                scroll = scroll.scroll_offset(Vec2::new(0.0_f32, target));
                            }

                            let scroll_out = scroll.show(ui, |ui| {
                                self.search_match_count = render_code_viewer(ui, self.theme, self.font_scale, &self.content, lang, &self.search_query, active_match_idx, self.search_jump_requested);
                            });
                            self.current_scroll_offset = scroll_out.state.offset.y;
                        }
                        ViewMode::PlainText => {
                            // 純文字檢視模式 (針對 .txt 或其他純文字檔，原汁原味顯示並支援搜尋高亮與跳轉定位、滾輪重置與鍵盤捲動，快取 LayoutJob 零拷貝)
                            let mut scroll = ScrollArea::both().auto_shrink([false, false]);
                            if let Some(target) = scroll_target {
                                scroll = scroll.scroll_offset(Vec2::new(0.0_f32, target));
                            }

                            let scroll_out = scroll.show(ui, |ui| {
                                ui.add_space(4.0);
                                let font_scale = self.font_scale;
                                let font_id = FontId::monospace(14.0 * font_scale);
                                let text_color = self.theme.text_primary();
                                let (hl_bg, hl_fg, act_bg, act_fg) = match self.theme {
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

                                let plaintext_expand_id = egui::Id::new((
                                    "plaintext_expand",
                                    self.content_revision,
                                ));
                                let plaintext_is_expanded = !self.search_query.trim().is_empty() || ctx
                                    .data(|d| d.get_temp::<bool>(plaintext_expand_id).unwrap_or(false));
                                let total_plaintext_lines = self.content.split_inclusive('\n').count();
                                let plaintext_is_truncated = !plaintext_is_expanded
                                    && total_plaintext_lines > 5000;

                                let cache_id = ui.make_persistent_id(format!(
                                    "plaintext_job_{}_{}_{}_{:?}_{:?}_{}",
                                    self.content_revision,
                                    (font_scale * 100.0) as u32,
                                    self.search_query,
                                    active_match_idx,
                                    self.theme,
                                    plaintext_is_expanded,
                                ));

                                let text_job = ui.ctx().data_mut(|d| {
                                    if let Some(cached) = d.get_temp::<egui::text::LayoutJob>(cache_id) {
                                        cached.clone()
                                    } else {
                                        let mut job = egui::text::LayoutJob::default();
                                        let base_fmt = egui::TextFormat {
                                             font_id: font_id.clone(),
                                            color: text_color,
                                            line_height: Some(22.0 * font_scale),
                                            ..Default::default()
                                        };
                                        let mut match_counter = 0;
                                        const MAX_PLAINTEXT_LINES: usize = 5000;
                                        let mut lines_count = 0;
                                        let mut displayed_text = String::new();
                                        let mut is_truncated = false;

                                        for line in self.content.split_inclusive('\n') {
                                            lines_count += 1;
                                            if plaintext_is_expanded || lines_count <= MAX_PLAINTEXT_LINES {
                                                displayed_text.push_str(line);
                                            } else {
                                                is_truncated = true;
                                            }
                                        }

                                        if is_truncated {
                                            displayed_text.push_str(&format!("\n\n⚡ ... [檔案過大（共 {} 行），已安全預覽前 {} 行] ...\n", lines_count, MAX_PLAINTEXT_LINES));
                                        }

                                        crate::markdown::append_highlighted_text(
                                            &mut job,
                                            &displayed_text,
                                            &self.search_query,
                                            base_fmt,
                                            hl_bg,
                                            hl_fg,
                                            act_bg,
                                            act_fg,
                                            active_match_idx,
                                            &mut match_counter,
                                        );
                                        d.insert_temp(cache_id, job.clone());
                                        job
                                    }
                                });

                                self.search_match_count = crate::search::find_matches(&text_job.text, &self.search_query).len();
                                crate::search::searchable_label(ui, text_job, &self.search_query, active_match_idx, self.search_jump_requested, egui::Sense::hover());
                                if plaintext_is_truncated {
                                    ui.add_space(8.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "檔案共 {} 行，目前為安全預覽前 5000 行。",
                                                total_plaintext_lines
                                            ))
                                            .color(self.theme.accent_color())
                                            .size(11.5 * self.font_scale),
                                        );
                                        if ui.button("載入完整內容").clicked() {
                                            ctx.data_mut(|d| {
                                                d.insert_temp(plaintext_expand_id, true);
                                            });
                                            ctx.request_repaint();
                                        }
                                    });
                                }
                            });
                            self.current_scroll_offset = scroll_out.state.offset.y;
                        }
                        ViewMode::Mindmap => {
                            // Markdown 互動式心智圖渲染模式 (支援樹狀水平佈局、貝茲曲線連線、縮放平移、節點收折與點擊跳轉回正文錨點)
                            let fname = self
                                .current_file
                                .as_ref()
                                .and_then(|p| p.file_stem())
                                .and_then(|s| s.to_str())
                                .unwrap_or("Markdown Document");

                            if self.mindmap_root.is_none() {
                                self.mindmap_root = Some(crate::views::mindmap::parse_markdown_to_mindmap(
                                    &self.content,
                                    fname,
                                ));
                            }

                            if let Some(ref mut root) = self.mindmap_root {
                                let output = crate::views::mindmap::render_mindmap_view(
                                    ui,
                                    self.theme,
                                    self.font_scale,
                                    root,
                                    &mut self.mindmap_state,
                                );

                                if let Some(anchor) = output.jump_to_anchor {
                                    self.target_anchor = Some(anchor);
                                    self.view_mode = ViewMode::Markdown;
                                    ctx.request_repaint();
                                } else if output.switch_to_markdown {
                                    self.view_mode = ViewMode::Markdown;
                                    ctx.request_repaint();
                                }
                            }
                        }
                        ViewMode::Image { .. } => {
                            // 圖片與 SVG 向量圖檢視模式 (支援縮放、滾輪、適應視窗)
                            self.render_image_viewer(ui);
                        }
                    }
                }
            });

        // 渲染完成後清除滾輪回到頂部、目標搜尋偏移與鍵盤捲動旗標，允許使用者後續正常捲動
        self.reset_scroll_to_top = false;
        self.target_scroll_offset = None;
        self.keyboard_scroll_delta = 0.0;
        if previous_match_count != self.search_match_count {
            self.search_match_index = self
                .search_match_index
                .min(self.search_match_count.saturating_sub(1));
            ctx.request_repaint();
        }
        self.search_jump_requested = false;
    }
}

impl MdPreviewApp {
    fn render_bottom_tips(&self, ui: &mut egui::Ui) {
        crate::views::status_bar::render_bottom_tips(ui, self.theme, self.is_editing);
    }

    fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        let mut do_browse = false;
        crate::views::empty_state::render_empty_state(ui, self.theme, || {
            do_browse = true;
        });
        if do_browse {
            self.open_file_dialog();
        }
    }

    /// 繪製圖片與 SVG 向量圖檢視畫布 (委派至 views::image_viewer 模組)
    fn render_image_viewer(&mut self, ui: &mut egui::Ui) {
        let format_ext = if let ViewMode::Image { ref format } = self.view_mode {
            format.as_str()
        } else {
            ""
        };

        crate::views::image_viewer::render_image_viewer(
            ui,
            self.image_bytes.as_deref(),
            self.image_uri.as_deref(),
            format_ext,
            &mut self.image_zoom,
            &mut self.image_fit_mode,
            self.reset_scroll_to_top,
            self.keyboard_scroll_delta,
        );
    }
}

fn rfd_open_file() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                r#"[System.Reflection.Assembly]::LoadWithPartialName("System.windows.forms") | Out-Null; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = "Markdown & Code Files (*.md;*.rs;*.py;*.js;*.ts;*.json;*.toml;*.yaml;*.cpp;*.go;*.txt)|*.md;*.rs;*.py;*.js;*.ts;*.json;*.toml;*.yaml;*.cpp;*.go;*.txt|All files (*.*)|*.*"; if($d.ShowDialog() -eq "OK"){ Write-Output $d.FileName }"#,
            ])
            .output();

        if let Ok(out) = output {
            let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> MdPreviewApp {
        let (_, hotkey_rx) = unbounded();
        let (watcher_tx, watcher_rx) = unbounded();
        let (_, tray_rx) = unbounded();
        let holder = Arc::new(Mutex::new(Some(Context::default())));
        let watcher = FileWatcher::new(watcher_tx, holder.clone());
        MdPreviewApp::empty(
            AppConfig::default(),
            true,
            true,
            watcher,
            hotkey_rx,
            watcher_rx,
            tray_rx,
            holder,
        )
    }

    #[test]
    fn opening_or_reloading_a_failed_file_preserves_the_current_document() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.md");
        fs::write(&path, "original").unwrap();
        let mut app = app();
        app.open_document(&path);
        app.open_document(&dir.path().join("missing.md"));
        assert_eq!(app.current_file.as_deref(), Some(path.as_path()));
        assert_eq!(app.content, "original");
        app.content = "unsaved draft".to_string();
        app.is_modified = true;
        fs::write(&path, "external change").unwrap();
        app.reload_current_file();
        assert_eq!(app.content, "unsaved draft");
        assert!(app.is_modified);
    }

    #[test]
    fn cancel_keeps_draft_and_failed_save_keeps_pending_action() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.md");
        let next = dir.path().join("next.md");
        fs::write(&path, "original").unwrap();
        fs::write(&next, "next").unwrap();
        let mut app = app();
        app.open_document(&path);
        app.content = "draft".to_string();
        app.is_modified = true;
        app.load_file(&next);
        assert!(app.pending_action.is_some());
        assert_eq!(app.content, "draft");
        app.resolve_pending_action(UnsavedChoice::Cancel);
        assert!(app.pending_action.is_none());
        assert!(app.is_modified);
        app.load_file(&next);
        fs::write(&path, "external").unwrap();
        app.resolve_pending_action(UnsavedChoice::Save);
        assert!(app.pending_action.is_some());
        assert_eq!(app.content, "draft");
        assert!(app.is_modified);
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
        app.resolve_pending_action(UnsavedChoice::Discard);
        assert_eq!(app.current_file, Some(next));
        assert_eq!(app.content, "next");
        assert!(!app.is_modified);
    }

    #[test]
    fn save_and_continue_writes_the_draft_before_switching() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.md");
        let next = dir.path().join("next.md");
        fs::write(&path, "original").unwrap();
        fs::write(&next, "next").unwrap();
        let mut app = app();
        app.open_document(&path);
        app.content = "draft".to_string();
        app.is_modified = true;
        app.load_file(&next);
        app.resolve_pending_action(UnsavedChoice::Save);
        assert_eq!(fs::read_to_string(&path).unwrap(), "draft");
        assert_eq!(app.current_file, Some(next));
        assert!(app.pending_action.is_none());
        assert!(!app.is_modified);
    }

    #[test]
    fn image_reload_changes_bytes_and_derived_caches_are_invalidated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        fs::write(&path, b"old image").unwrap();
        let mut app = app();
        app.open_document(&path);
        let uri = app.image_uri.clone();
        let revision = app.content_revision;
        fs::write(&path, b"new image").unwrap();
        app.reload_current_file();
        assert_ne!(app.image_uri, uri);
        assert_eq!(app.image_bytes.as_deref(), Some(b"new image".as_slice()));
        assert!(app.content_revision > revision);
        app.mindmap_root = Some(crate::views::mindmap::parse_markdown_to_mindmap(
            "# old", "old",
        ));
        app.invalidate_content();
        assert!(app.mindmap_root.is_none());
    }

    #[test]
    fn updater_failure_unlocks_the_ui_and_allows_retry() {
        let mut app = app();
        app.is_updating = true;
        app.handle_update_event(UpdateEvent::Installed(Err("download failed".to_string())));
        assert!(!app.is_updating);
        assert!(app
            .status_toast
            .as_ref()
            .unwrap()
            .0
            .contains("download failed"));
    }
}
