use crate::explorer::{hide_app_window, show_and_focus_app_window};
use crate::hotkey::HotkeyEvent;
use crate::markdown::MarkdownRenderer;
use crate::theme::{setup_system_cjk_fonts, AppTheme};
use crate::tray::TrayMenuAction;
use crate::updater::{check_latest_release, perform_self_update, ReleaseInfo, CURRENT_VERSION};
use crate::watcher::{FileWatcher, WatcherEvent};
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{
    Align, Color32, Context, FontId, Frame, Layout, Margin, RichText, Rounding, ScrollArea, Stroke,
    TextEdit, Vec2,
};
use log::{error, info};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct MdPreviewApp {
    pub current_file: Option<PathBuf>,
    pub content: String,
    pub file_size_str: String,
    pub line_count: usize,
    pub last_modified_str: String,

    pub theme: AppTheme,
    pub font_scale: f32,
    pub always_on_top: bool,
    pub visible: bool,
    pub is_standalone: bool,

    pub search_open: bool,
    pub search_query: String,

    pub available_update: Option<ReleaseInfo>,
    pub is_updating: bool,
    pub update_tx: Sender<Option<ReleaseInfo>>,
    pub update_rx: Receiver<Option<ReleaseInfo>>,

    pub file_watcher: FileWatcher,
    pub hotkey_rx: Receiver<HotkeyEvent>,
    pub watcher_rx: Receiver<WatcherEvent>,
    pub tray_rx: Receiver<TrayMenuAction>,
    pub ctx_holder: Arc<Mutex<Option<Context>>>,

    pub status_toast: Option<(String, std::time::Instant)>,
}

impl MdPreviewApp {
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

        // 載入 Windows 繁體中文與 Emoji 系統字型 (徹底解決方塊字問題)
        setup_system_cjk_fonts(&cc.egui_ctx);

        let theme = AppTheme::Dark;
        theme.apply_to_ctx(&cc.egui_ctx);

        let (update_tx, update_rx) = unbounded();

        let is_visible = initial_file.is_some() || is_standalone;

        let mut app = Self {
            current_file: None,
            content: String::new(),
            file_size_str: String::new(),
            line_count: 0,
            last_modified_str: String::new(),
            theme,
            font_scale: 1.0,
            always_on_top: false,
            visible: is_visible,
            is_standalone,
            search_open: false,
            search_query: String::new(),
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
        };

        // 啟動時在背景默默檢查是否有新版本發布
        let bg_tx = update_tx.clone();
        let bg_ctx_holder = ctx_holder.clone();
        thread::spawn(move || {
            let rel = check_latest_release();
            let has_update = rel.is_some();
            let _ = bg_tx.send(rel);
            if has_update {
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

    pub fn check_update_manually(&mut self) {
        self.set_toast("正在檢查 GitHub 最新版本... ⏳".to_string());
        let tx = self.update_tx.clone();
        let ctx_holder = self.ctx_holder.clone();
        thread::spawn(move || {
            let rel = check_latest_release();
            let _ = tx.send(rel);
            if let Ok(guard) = ctx_holder.lock() {
                if let Some(ref ctx) = *guard {
                    ctx.request_repaint();
                }
            }
        });
    }

    pub fn trigger_self_update(&mut self) {
        if let Some(release) = self.available_update.clone() {
            self.is_updating = true;
            self.set_toast(format!("正在下載並自動更新至 {}... 請稍候 ⏳", release.tag_name));
            let ctx_holder = self.ctx_holder.clone();

            thread::spawn(move || {
                match perform_self_update(&release) {
                    Ok(_) => {
                        info!("更新完成！");
                    }
                    Err(e) => {
                        error!("更新失敗: {}", e);
                    }
                }
                if let Ok(guard) = ctx_holder.lock() {
                    if let Some(ref ctx) = *guard {
                        ctx.request_repaint();
                    }
                }
            });
        }
    }

    pub fn load_file(&mut self, path: &Path) {
        info!("嘗試載入檔案: {:?}", path);

        if path.is_dir() {
            self.set_toast("已選取資料夾，請在資料夾內選取 Markdown 或文字檔案預覽 📁".to_string());
            self.visible = true;
            show_and_focus_app_window();
            return;
        }

        let path_str = path.to_string_lossy().to_string();
        if !path.exists() {
            if path_str.to_lowercase().contains(".zip\\") || path_str.to_lowercase().contains(".zip/") {
                self.set_toast("⚠️ 提示：請先將 ZIP 壓縮檔解壓縮後再進行檔案預覽 📦".to_string());
            } else {
                self.set_toast(format!("找不到檔案: {:?}", path));
            }
            self.visible = true;
            show_and_focus_app_window();
            return;
        }

        match fs::read_to_string(path) {
            Ok(text) => {
                self.line_count = text.lines().count();
                self.content = text;
                self.current_file = Some(path.to_path_buf());

                // 檔案元資訊計算
                if let Ok(metadata) = fs::metadata(path) {
                    let len = metadata.len();
                    self.file_size_str = if len < 1024 {
                        format!("{} B", len)
                    } else if len < 1024 * 1024 {
                        format!("{:.1} KB", len as f64 / 1024.0)
                    } else {
                        format!("{:.2} MB", len as f64 / (1024.0 * 1024.0))
                    };

                    if let Ok(mod_time) = metadata.modified() {
                        let datetime: chrono::DateTime<chrono::Local> = mod_time.into();
                        self.last_modified_str = datetime.format("%Y-%m-%d %H:%M").to_string();
                    }
                }

                // 啟動變更監視
                self.file_watcher.watch_file(path);
                self.visible = true;
                show_and_focus_app_window();
                let fname = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                self.set_toast(format!("⚡ 已開啟預覽: {}", fname));
            }
            Err(e) => {
                error!("讀取檔案失敗 {:?}: {:?}", path, e);
                self.set_toast(format!("無法讀取檔案: {}", e));
                self.visible = true;
                show_and_focus_app_window();
            }
        }
    }

    pub fn reload_current_file(&mut self) {
        if let Some(path) = self.current_file.clone() {
            if let Ok(text) = fs::read_to_string(&path) {
                self.line_count = text.lines().count();
                self.content = text;
                self.set_toast("檔案已自動即時同步更新 ⚡".to_string());
            }
        }
    }

    pub fn handle_hotkey_preview(&mut self, maybe_path: Option<PathBuf>) {
        if let Some(selected_path) = maybe_path {
            info!("快捷鍵觸發，處理選取檔案: {:?}", selected_path);
            if self.visible && self.current_file.as_deref() == Some(&selected_path) {
                // 如果已經在預覽同一檔案且視窗開啟中，則隱藏 (Quick Look 體驗)
                self.visible = false;
                hide_app_window();
            } else {
                self.load_file(&selected_path);
            }
        } else {
            // 沒有在檔案總管選取特定檔案
            if self.visible {
                self.visible = false;
                hide_app_window();
            } else {
                self.visible = true;
                show_and_focus_app_window();
                self.set_toast("⚡ 已開啟 flash-md！(在檔案總管點選 .md 或 .txt 檔案後按 Alt+Space 可直接預覽)".to_string());
            }
        }
    }

    pub fn set_toast(&mut self, msg: String) {
        self.status_toast = Some((msg, std::time::Instant::now()));
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd_open_file() {
            self.load_file(&path);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        // ESC: 隱藏或關閉視窗
        if input.key_pressed(egui::Key::Escape) {
            if self.search_open {
                self.search_open = false;
                self.search_query.clear();
            } else {
                self.visible = false;
                hide_app_window();
                if self.is_standalone {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // Ctrl + F: 搜尋
        if input.modifiers.command && input.key_pressed(egui::Key::F) {
            self.search_open = !self.search_open;
        }

        // Ctrl + O: 在外部預設編輯器開啟
        if input.modifiers.command && input.key_pressed(egui::Key::O) {
            if let Some(ref path) = self.current_file {
                let _ = open::that(path);
            }
        }

        // Ctrl + Shift + C: 複製全文
        if input.modifiers.command && input.modifiers.shift && input.key_pressed(egui::Key::C) {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(self.content.clone());
                self.set_toast("已複製完整 Markdown 內文至剪貼簿 📋".to_string());
            }
        }

        // Ctrl + + / Ctrl + - : 縮放字體
        if input.modifiers.command && (input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)) {
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
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                if self.always_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                },
            ));
            self.set_toast(if self.always_on_top {
                "視窗置頂: 已開啟 📌".to_string()
            } else {
                "視窗置頂: 已關閉".to_string()
            });
        }
    }
}

impl eframe::App for MdPreviewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 定期自動喚醒保持背景訊息接收敏捷 (每 100ms 檢查一次)
        ctx.request_repaint_after(Duration::from_millis(100));

        // 確保 context holder 隨時保持最新
        if let Ok(mut guard) = self.ctx_holder.lock() {
            if guard.is_none() {
                *guard = Some(ctx.clone());
            }
        }

        // 處理非同步更新檢查結果
        while let Ok(res) = self.update_rx.try_recv() {
            if let Some(rel) = res {
                self.set_toast(format!("🎉 發現新版本 {}！可在頂部點擊升級", rel.tag_name));
                self.available_update = Some(rel);
            } else {
                self.set_toast(format!("✅ 目前已是最新版本 (v{})", CURRENT_VERSION));
            }
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
                WatcherEvent::FileChanged(path) => {
                    if self.current_file.as_deref() == Some(&path) {
                        self.reload_current_file();
                        ctx.request_repaint();
                    }
                }
            }
        }

        // 攔截原生視窗關閉 (X) 事件：常駐模式下取消退出，改為隱藏回系統匣
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.is_standalone {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.visible = false;
                hide_app_window();
            }
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
                    self.set_toast(format!("flash-md v{} - 快捷鍵 Alt+Space 閃電預覽 ⚡", CURRENT_VERSION));
                    self.visible = true;
                    show_and_focus_app_window();
                }
                TrayMenuAction::Exit => {
                    info!("使用者自系統匣退出 flash-md");
                    std::process::exit(0);
                }
            }
        }

        // 快捷鍵監聽
        self.handle_shortcuts(ctx);

        // 如果視窗處於隱藏狀態，則不進行後續完整 Panel 渲染以節省資源
        if !self.visible && !self.is_standalone {
            return;
        }

        // 頂部新版本升級橫幅 (若有新版本)
        let mut dismiss_update = false;
        let mut do_self_update = false;
        if let Some(ref release) = self.available_update {
            let release_tag = release.tag_name.clone();
            egui::TopBottomPanel::top("update_banner")
                .frame(
                    Frame::none()
                        .fill(self.theme.accent_bg())
                        .stroke(Stroke::new(1.0_f32, self.theme.accent_color()))
                        .inner_margin(Margin::symmetric(16.0, 8.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("🎉 發現全新版本 {} (目前為 v{})！", release_tag, CURRENT_VERSION))
                                .color(self.theme.accent_color())
                                .strong()
                                .size(12.5),
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(RichText::new("✕ 稍後").size(11.0)).clicked() {
                                dismiss_update = true;
                            }

                            if ui
                                .button(RichText::new(" 🚀 一鍵自動升級 ").strong().size(12.0).color(Color32::WHITE))
                                .clicked()
                            {
                                do_self_update = true;
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

        // 頂部現代精緻導航列 (Fluent / macOS 玻璃質感風格)
        egui::TopBottomPanel::top("top_header")
            .frame(
                Frame::none()
                    .fill(self.theme.card_bg_color())
                    .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                    .inner_margin(Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // 左側：閃電標誌與檔案資訊
                    ui.label(
                        RichText::new("⚡")
                            .size(16.0)
                            .color(self.theme.accent_color())
                            .strong(),
                    );

                    let file_name = self
                        .current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("flash-md 預覽器");

                    let title_resp = ui.button(
                        RichText::new(file_name)
                            .strong()
                            .size(14.0)
                            .color(self.theme.text_primary()),
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

                    // 檔案屬性標籤 (行數、大小、修改時間)
                    if !self.content.is_empty() {
                        ui.add_space(6.0);
                        Frame::none()
                            .fill(self.theme.code_bg_color())
                            .rounding(Rounding::same(4.0))
                            .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                            .inner_margin(Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} 行  •  {}  •  {}",
                                        self.line_count, self.file_size_str, self.last_modified_str
                                    ))
                                    .size(11.0)
                                    .color(self.theme.text_secondary()),
                                );
                            });
                    }

                    // 右側功能按鈕列 (簡約現代圖示按鈕)
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // 關閉按鈕
                        if ui
                            .button(RichText::new(" ✕ ").size(13.0).color(self.theme.text_secondary()))
                            .on_hover_text("關閉 / 隱藏預覽 (Esc)")
                            .clicked()
                        {
                            self.visible = false;
                            hide_app_window();
                            if self.is_standalone {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }

                        // 置頂狀態按鈕
                        let pin_color = if self.always_on_top {
                            self.theme.accent_color()
                        } else {
                            self.theme.text_secondary()
                        };
                        let pin_btn = ui.button(
                            RichText::new(if self.always_on_top { " 📌 置頂中 " } else { " 📌 置頂 " })
                                .size(12.0)
                                .color(pin_color),
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
                        }
                        if pin_btn.hovered() {
                            pin_btn.on_hover_text("切換視窗置頂 (Ctrl + P)");
                        }

                        // 主題切換按鈕
                        let (theme_icon, theme_tip) = match self.theme {
                            AppTheme::Dark => (" ☀️ 淺色 ", "切換為淺色主題"),
                            AppTheme::Light => (" 🌙 深色 ", "切換為深色主題"),
                        };
                        if ui
                            .button(RichText::new(theme_icon).size(12.0).color(self.theme.text_secondary()))
                            .on_hover_text(theme_tip)
                            .clicked()
                        {
                            self.theme.toggle();
                            self.theme.apply_to_ctx(ctx);
                        }

                        // 檢查更新按鈕
                        if ui
                            .button(RichText::new(" 🔄 更新 ").size(12.0).color(self.theme.text_secondary()))
                            .on_hover_text("檢查 GitHub 最新版本")
                            .clicked()
                        {
                            self.check_update_manually();
                        }

                        // 外部編輯器開啟
                        if ui
                            .button(RichText::new(" 🚀 編輯器 ").size(12.0).color(self.theme.text_secondary()))
                            .on_hover_text("在系統預設編輯器中開啟 (Ctrl + O)")
                            .clicked()
                        {
                            if let Some(ref path) = self.current_file {
                                let _ = open::that(path);
                            }
                        }

                        // 複製全文按鈕
                        if ui
                            .button(RichText::new(" 📋 複製 ").size(12.0).color(self.theme.text_secondary()))
                            .on_hover_text("複製全部 Markdown 內文 (Ctrl + Shift + C)")
                            .clicked()
                        {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(self.content.clone());
                                self.set_toast("已複製全文至剪貼簿 📋".to_string());
                            }
                        }

                        // 搜尋按鈕
                        if ui
                            .button(RichText::new(" 🔍 尋找 ").size(12.0).color(self.theme.text_secondary()))
                            .on_hover_text("搜尋關鍵字 (Ctrl + F)")
                            .clicked()
                        {
                            self.search_open = !self.search_open;
                        }

                        // 開啟檔案按鈕
                        if ui
                            .button(RichText::new(" 📂 開啟... ").size(12.0).color(self.theme.text_secondary()))
                            .on_hover_text("開啟本機 Markdown 檔案")
                            .clicked()
                        {
                            self.open_file_dialog();
                        }
                    });
                });

                // 搜尋列展開區 (Ctrl + F)
                if self.search_open {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍 尋找內文:").size(12.5).color(self.theme.accent_color()));
                        ui.add(
                            TextEdit::singleline(&mut self.search_query)
                                .hint_text("輸入關鍵字...")
                                .desired_width(280.0),
                        );
                        if ui.button(RichText::new("✕").size(11.0)).clicked() {
                            self.search_open = false;
                            self.search_query.clear();
                        }
                    });
                }
            });

        // 底部狀態列 / Toast 提示
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

                    // 右側縮放控制
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let zoom_str = format!("{}%", (self.font_scale * 100.0).round() as u32);
                        ui.label(
                            RichText::new(zoom_str)
                                .color(self.theme.text_secondary())
                                .size(11.5),
                        );

                        if ui.small_button(" + ").on_hover_text("放大字體 (Ctrl + +)").clicked() {
                            self.font_scale = (self.font_scale + 0.1).min(2.0);
                        }
                        if ui.small_button(" − ").on_hover_text("縮小字體 (Ctrl + -)").clicked() {
                            self.font_scale = (self.font_scale - 0.1).max(0.6);
                        }
                        if ui.small_button(" 1:1 ").on_hover_text("重設字體 (Ctrl + 0)").clicked() {
                            self.font_scale = 1.0;
                        }
                    });
                });
            });

        // 主 Markdown 渲染檢視區域
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(self.theme.bg_color())
                    .inner_margin(Margin::symmetric(32.0, 20.0)),
            )
            .show(ctx, |ui| {
                if self.content.is_empty() {
                    // 極具現代質感的空狀態卡片介面 (Raycast / Linear Style)
                    self.render_empty_state(ui);
                } else {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let renderer = MarkdownRenderer::new(self.theme, self.font_scale);
                            renderer.render(ui, &self.content);
                        });
                }
            });
    }
}

impl MdPreviewApp {
    fn render_bottom_tips(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!(
                "flash-md v{}  •  快捷鍵: Alt + Space (快速預覽)  •  Esc (隱藏)  •  Ctrl + O (外部開啟)",
                CURRENT_VERSION
            ))
            .color(self.theme.text_secondary())
            .size(11.5),
        );
    }

    fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            Frame::none()
                .fill(self.theme.card_bg_color())
                .rounding(Rounding::same(12.0))
                .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                .inner_margin(Margin::symmetric(40.0, 36.0))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        // 閃電發光徽章
                        Frame::none()
                            .fill(self.theme.accent_bg())
                            .rounding(Rounding::same(24.0))
                            .stroke(Stroke::new(1.5_f32, self.theme.accent_color()))
                            .inner_margin(Margin::symmetric(16.0, 10.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("⚡ flash-md v{}", CURRENT_VERSION))
                                        .size(24.0)
                                        .strong()
                                        .color(self.theme.accent_color()),
                                );
                            });

                        ui.add_space(16.0);

                        ui.label(
                            RichText::new("Windows 快捷鍵 Markdown 閃電預覽")
                                .size(17.0)
                                .strong()
                                .color(self.theme.text_primary()),
                        );

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("在檔案總管或桌面選取任何 Markdown 檔案，按下快捷鍵即可秒開預覽")
                                .size(13.5)
                                .color(self.theme.text_secondary()),
                        );

                        ui.add_space(20.0);

                        // 擬真實體鍵盤按鍵 UI
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            render_keycap(ui, self.theme, "Alt");
                            ui.label(RichText::new("+").size(15.0).color(self.theme.text_secondary()));
                            render_keycap(ui, self.theme, "Space");
                        });

                        ui.add_space(24.0);

                        // 選擇檔案按鈕
                        let browse_btn = ui.add_sized(
                            Vec2::new(200.0, 36.0),
                            egui::Button::new(
                                RichText::new("📂 瀏覽並開啟 Markdown 檔案")
                                    .size(13.5)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(self.theme.accent_color())
                            .rounding(Rounding::same(8.0)),
                        );

                        if browse_btn.clicked() {
                            self.open_file_dialog();
                        }

                        ui.add_space(18.0);
                        ui.separator();
                        ui.add_space(10.0);

                        // 特色小標
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("⚡ 純 Rust 毫秒級渲染  •  🔄 即時熱重載  •  🚀 一鍵在線升級")
                                    .size(11.5)
                                    .color(self.theme.text_secondary()),
                            );
                        });
                    });
                });
        });
    }
}

/// 繪製擬真鍵盤按鍵 (Keycap) 元件
fn render_keycap(ui: &mut egui::Ui, theme: AppTheme, key_text: &str) {
    Frame::none()
        .fill(theme.code_bg_color())
        .rounding(Rounding::same(6.0))
        .stroke(Stroke::new(1.0_f32, theme.border_color()))
        .inner_margin(Margin::symmetric(12.0, 6.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(key_text)
                    .font(FontId::monospace(13.0))
                    .strong()
                    .color(theme.accent_color()),
            );
        });
}

fn rfd_open_file() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                r#"[System.Reflection.Assembly]::LoadWithPartialName("System.windows.forms") | Out-Null; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = "Markdown (*.md;*.markdown;*.txt)|*.md;*.markdown;*.txt|All files (*.*)|*.*"; if($d.ShowDialog() -eq "OK"){ Write-Output $d.FileName }"#,
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
