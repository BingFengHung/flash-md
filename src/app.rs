use crate::explorer::get_selected_file_from_explorer;
use crate::hotkey::HotkeyEvent;
use crate::markdown::MarkdownRenderer;
use crate::theme::AppTheme;
use crate::tray::TrayMenuAction;
use crate::watcher::{FileWatcher, WatcherEvent};
use crossbeam_channel::Receiver;
use egui::{
    Align, Frame, Layout, Margin, RichText, ScrollArea, Stroke, TextEdit,
};
use log::{error, info};
use std::fs;
use std::path::{Path, PathBuf};

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

    pub file_watcher: FileWatcher,
    pub hotkey_rx: Receiver<HotkeyEvent>,
    pub watcher_rx: Receiver<WatcherEvent>,
    pub tray_rx: Receiver<TrayMenuAction>,

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
    ) -> Self {
        let theme = AppTheme::Dark;
        theme.apply_to_ctx(&cc.egui_ctx);

        let mut app = Self {
            current_file: None,
            content: String::new(),
            file_size_str: String::new(),
            line_count: 0,
            last_modified_str: String::new(),
            theme,
            font_scale: 1.0,
            always_on_top: false,
            visible: initial_file.is_some() || !is_standalone,
            is_standalone,
            search_open: false,
            search_query: String::new(),
            file_watcher,
            hotkey_rx,
            watcher_rx,
            tray_rx,
            status_toast: None,
        };

        if let Some(file) = initial_file {
            app.load_file(&file);
        }

        app
    }

    pub fn load_file(&mut self, path: &Path) {
        info!("載入檔案: {:?}", path);
        match fs::read_to_string(path) {
            Ok(text) => {
                self.line_count = text.lines().count();
                self.content = text;
                self.current_file = Some(path.to_path_buf());

                // 檔案資訊
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
                self.set_toast(format!("已載入: {}", path.file_name().and_then(|f| f.to_str()).unwrap_or("")));
            }
            Err(e) => {
                error!("讀取檔案失敗 {:?}: {:?}", path, e);
                self.set_toast(format!("無法讀取檔案: {}", e));
            }
        }
    }

    pub fn reload_current_file(&mut self) {
        if let Some(path) = self.current_file.clone() {
            if let Ok(text) = fs::read_to_string(&path) {
                self.line_count = text.lines().count();
                self.content = text;
                self.set_toast("檔案已自動即時重新整理 ⚡".to_string());
            }
        }
    }

    pub fn trigger_hotkey_preview(&mut self) {
        // 從檔案總管或桌面取得選取檔案
        if let Some(selected_path) = get_selected_file_from_explorer() {
            let is_supported = selected_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    let ext_lower = ext.to_lowercase();
                    matches!(
                        ext_lower.as_str(),
                        "md" | "markdown" | "mdown" | "mkd" | "txt" | "rs" | "toml" | "json" | "yaml" | "yml"
                    )
                })
                .unwrap_or(false);

            if is_supported {
                // 若當前已經在預覽同一檔案且視窗可見，則關閉（macOS QuickLook 體驗）
                if self.visible && self.current_file.as_deref() == Some(&selected_path) {
                    self.visible = false;
                } else {
                    self.load_file(&selected_path);
                    self.visible = true;
                }
            } else {
                // 若為其他檔案或無副檔名，依舊嘗試以文字/Markdown 預覽
                if self.visible && self.current_file.as_deref() == Some(&selected_path) {
                    self.visible = false;
                } else {
                    self.load_file(&selected_path);
                    self.visible = true;
                }
            }
        } else if self.visible {
            // 沒有選取檔案但視窗開啟中，按下快捷鍵則隱藏視窗
            self.visible = false;
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

        // Ctrl + C: 複製全文
        if input.modifiers.command && input.modifiers.shift && input.key_pressed(egui::Key::C) {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(self.content.clone());
                self.set_toast("已複製全文到剪貼簿 📋".to_string());
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
                "視窗置頂: 開啟 📌".to_string()
            } else {
                "視窗置頂: 關閉".to_string()
            });
        }
    }
}

impl eframe::App for MdPreviewApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 處理全域快捷鍵事件
        while let Ok(event) = self.hotkey_rx.try_recv() {
            if event == HotkeyEvent::TriggerPreview {
                self.trigger_hotkey_preview();
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
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
                TrayMenuAction::About => {
                    self.set_toast("flash-md v0.2.4 - 快捷鍵 Alt+Space 閃電預覽 Markdown ⚡".to_string());
                    self.visible = true;
                }
                TrayMenuAction::Exit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // 快捷鍵監聽
        self.handle_shortcuts(ctx);

        // 如果視窗隱藏且非單獨預覽模式，最小化繪製負擔
        if !self.visible && !self.is_standalone {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            return;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }

        // 頂部導航與工具列
        egui::TopBottomPanel::top("top_header")
            .frame(
                Frame::none()
                    .fill(self.theme.card_bg_color())
                    .stroke(Stroke::new(1.0_f32, self.theme.border_color()))
                    .inner_margin(Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // 左側：檔案標題與元資訊
                    let file_name = self
                        .current_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("未選擇檔案 (按 Alt + Space 或開啟檔案)");

                    ui.label(
                        RichText::new("⚡")
                            .size(16.0)
                            .color(self.theme.accent_color()),
                    );

                    let title_resp = ui.button(
                        RichText::new(file_name)
                            .strong()
                            .size(14.5)
                            .color(self.theme.text_primary()),
                    );

                    if title_resp.clicked() {
                        if let Some(ref path) = self.current_file {
                            // 點擊複製完整路徑
                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                let _ = cb.set_text(path.to_string_lossy().to_string());
                                self.set_toast("已複製檔案路徑 📁".to_string());
                            }
                        }
                    }

                    if title_resp.hovered() {
                        if let Some(ref path) = self.current_file {
                            title_resp.on_hover_text(format!("完整路徑: {:?}\n(點擊複製路徑)", path));
                        }
                    }

                    // 檔案徽章 (行數, 大小, 時間)
                    if !self.content.is_empty() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!(
                                "{} 行 • {} • {}",
                                self.line_count, self.file_size_str, self.last_modified_str
                            ))
                            .size(11.5)
                            .color(self.theme.text_secondary()),
                        );
                    }

                    // 右側功能按鈕列
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // 關閉按鈕
                        if ui
                            .button(RichText::new("✕").size(14.0).color(self.theme.text_secondary()))
                            .on_hover_text("隱藏視窗 (Esc)")
                            .clicked()
                        {
                            self.visible = false;
                            if self.is_standalone {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }

                        // 置頂按鈕
                        let pin_text = if self.always_on_top { "📌 置頂中" } else { "📌 置頂" };
                        let pin_btn = ui.button(
                            RichText::new(pin_text)
                                .size(12.5)
                                .color(if self.always_on_top {
                                    self.theme.accent_color()
                                } else {
                                    self.theme.text_secondary()
                                }),
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

                        // 主題切換
                        let theme_icon = match self.theme {
                            AppTheme::Dark => "☀️ 淺色",
                            AppTheme::Light => "🌙 深色",
                        };
                        if ui
                            .button(RichText::new(theme_icon).size(12.5))
                            .clicked()
                        {
                            self.theme.toggle();
                            self.theme.apply_to_ctx(ctx);
                        }

                        // 外部編輯器開啟
                        if ui
                            .button(RichText::new("🚀 編輯器開啟").size(12.5))
                            .on_hover_text("在系統預設編輯器中開啟 (Ctrl + O)")
                            .clicked()
                        {
                            if let Some(ref path) = self.current_file {
                                let _ = open::that(path);
                            }
                        }

                        // 複製內容
                        if ui
                            .button(RichText::new("📋 複製全文").size(12.5))
                            .on_hover_text("複製全部 Markdown 內文 (Ctrl + Shift + C)")
                            .clicked()
                        {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                let _ = clipboard.set_text(self.content.clone());
                                self.set_toast("已複製全文到剪貼簿 📋".to_string());
                            }
                        }

                        // 開啟檔案
                        if ui
                            .button(RichText::new("📂 開啟...").size(12.5))
                            .clicked()
                        {
                            self.open_file_dialog();
                        }
                    });
                });

                // 搜尋條 (Ctrl + F)
                if self.search_open {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍 尋找:").size(13.0));
                        let response = ui.add(
                            TextEdit::singleline(&mut self.search_query)
                                .hint_text("輸入搜尋關鍵字...")
                                .desired_width(260.0),
                        );
                        if response.changed() {
                            // 可擴展標亮功能
                        }
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
                    .inner_margin(Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some((ref msg, instant)) = self.status_toast {
                        if instant.elapsed().as_secs() < 3 {
                            ui.label(
                                RichText::new(msg)
                                    .color(self.theme.accent_color())
                                    .size(12.0),
                            );
                        }
                    } else {
                        ui.label(
                            RichText::new("快捷鍵: Alt + Space (預覽選取檔案) | Esc (隱藏) | Ctrl+O (開啟編輯器)")
                                .color(self.theme.text_secondary())
                                .size(11.5),
                        );
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let zoom_str = format!("{}%", (self.font_scale * 100.0).round() as u32);
                        ui.label(
                            RichText::new(zoom_str)
                                .color(self.theme.text_secondary())
                                .size(11.5),
                        );

                        if ui.small_button("+").clicked() {
                            self.font_scale = (self.font_scale + 0.1).min(2.0);
                        }
                        if ui.small_button("-").clicked() {
                            self.font_scale = (self.font_scale - 0.1).max(0.6);
                        }
                    });
                });
            });

        // 主 Markdown 渲染檢視區域
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(self.theme.bg_color())
                    .inner_margin(Margin::symmetric(24.0, 18.0)),
            )
            .show(ctx, |ui| {
                if self.content.is_empty() {
                    // 空狀態導引
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(80.0);
                            ui.label(
                                RichText::new("⚡ flash-md")
                                    .size(28.0)
                                    .strong()
                                    .color(self.theme.accent_color()),
                            );
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new("在 Windows 檔案總管或桌面選取 .md 檔案，按下 Alt + Space 即可閃電預覽！")
                                    .size(15.0)
                                    .color(self.theme.text_secondary()),
                            );
                            ui.add_space(20.0);
                            if ui
                                .button(RichText::new("📂 選擇並開啟 Markdown 檔案").size(14.0))
                                .clicked()
                            {
                                self.open_file_dialog();
                            }
                        });
                    });
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
