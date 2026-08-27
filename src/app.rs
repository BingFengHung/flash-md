use crate::config::{AppConfig, SaveMode};
use crate::explorer::{get_selected_file_from_explorer, hide_app_window, show_and_focus_app_window};
use crate::hotkey::HotkeyEvent;
use crate::markdown::{
    calculate_text_stats, get_image_badge, get_language_badge, is_code_extension,
    is_image_extension, is_pdf_extension, render_code_viewer, MarkdownRenderer,
};
use crate::theme::{setup_system_cjk_fonts, AppTheme};
use crate::tray::TrayMenuAction;
use crate::updater::{
    check_latest_release, perform_self_update, restart_with_new_version,
    CURRENT_VERSION, ReleaseInfo,
};
use crate::views::status_bar::render_nav_button;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Markdown,
    Code { lang: String },
    PlainText,
    Table { separator: char },
    Image { format: String },
}

pub struct MdPreviewApp {
    pub current_file: Option<PathBuf>,
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
    pub update_tx: Sender<Option<ReleaseInfo>>,
    pub update_rx: Receiver<Option<ReleaseInfo>>,

    pub file_watcher: FileWatcher,
    pub hotkey_rx: Receiver<HotkeyEvent>,
    pub watcher_rx: Receiver<WatcherEvent>,
    pub tray_rx: Receiver<TrayMenuAction>,
    pub ctx_holder: Arc<Mutex<Option<Context>>>,

    pub status_toast: Option<(String, std::time::Instant)>,
    pub reset_scroll_to_top: bool,
    pub keyboard_scroll_delta: f32,
    pub reading_progress: f32,
    pub is_ime_composing: bool,
    pub last_ime_activity: Option<std::time::Instant>,
    pub is_slides_mode: bool,
    pub current_slide_index: usize,
    pub is_slides_fullscreen: bool,
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

        // 安裝 egui_extras 內建的所有圖片與 SVG 向量圖載入器
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // 載入 Windows 繁體中文與 Emoji 系統字型 (徹底解決方塊字問題)
        setup_system_cjk_fonts(&cc.egui_ctx);

        let config = AppConfig::load();
        let theme = config.theme;
        theme.apply_to_ctx(&cc.egui_ctx);
        let font_scale = config.font_scale;
        let always_on_top = config.always_on_top;

        let (update_tx, update_rx) = unbounded();

        let is_visible = initial_file.is_some() || is_standalone;

        let mut app = Self {
            current_file: None,
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
            visible: is_visible,
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
            reading_progress: 0.0,
            is_ime_composing: false,
            last_ime_activity: None,
            is_slides_mode: false,
            current_slide_index: 0,
            is_slides_fullscreen: false,
        };

        if !is_visible {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            hide_app_window();
        }

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
            if self.is_updating {
                return;
            }
            self.is_updating = true;
            self.set_toast(format!("正在下載並自動升級至 {}... ⏳", release.tag_name));
            let ctx_holder = self.ctx_holder.clone();
            let current_file_path = self.current_file.clone();
            let is_standalone = self.is_standalone;

            thread::spawn(move || {
                match perform_self_update(&release) {
                    Ok(_) => {
                        info!("更新完成！即將自動無縫重啟新版本...");
                        if let Ok(guard) = ctx_holder.lock() {
                            if let Some(ref ctx) = *guard {
                                ctx.request_repaint();
                            }
                        }
                        // 稍作緩衝讓介面完成最後繪製
                        thread::sleep(Duration::from_millis(600));

                        let mut args = Vec::new();
                        if is_standalone {
                            if let Some(ref p) = current_file_path {
                                args.push(p.to_string_lossy().to_string());
                            }
                        }
                        restart_with_new_version(&args);
                        std::process::exit(0);
                    }
                    Err(e) => {
                        error!("更新失敗: {}", e);
                        if let Ok(guard) = ctx_holder.lock() {
                            if let Some(ref ctx) = *guard {
                                ctx.request_repaint();
                            }
                        }
                    }
                }
            });
        }
    }

    pub fn load_file(&mut self, path: &Path) {
        info!("嘗試載入檔案: {:?}", path);
        self.reset_scroll_to_top = true;
        self.search_match_index = 0;
        self.target_scroll_offset = None;
        self.target_anchor = None;
        self.is_editing = false;
        self.is_slides_mode = false;
        self.current_slide_index = 0;
        self.is_modified = false;
        self.last_edit_instant = None;

        if path.is_dir() {
            self.set_toast(format!("已選取資料夾: {:?}", path.file_name().unwrap_or_default()));
            self.visible = true;
            show_and_focus_app_window();
            return;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if path.exists() {
            if is_image_extension(&ext) {
                if let Ok(bytes) = fs::read(path) {
                    let path_str = path.to_string_lossy().to_string();
                    let uri = format!("file:///{}", path_str.replace('\\', "/"));
                    self.image_uri = Some(uri);
                    self.image_bytes = Some(bytes.clone());
                    self.image_zoom = 1.0;
                    self.image_fit_mode = true;
                    self.view_mode = ViewMode::Image { format: ext.clone() };

                    if ext == "svg" {
                        self.content = String::from_utf8_lossy(&bytes).to_string();
                        self.original_content = self.content.clone();
                        self.line_count = self.content.lines().count();
                    } else {
                        self.content.clear();
                        self.original_content.clear();
                        self.line_count = 0;
                    }

                    self.current_file = Some(path.to_path_buf());

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

                    self.file_watcher.watch_file(path);
                    self.visible = true;
                    show_and_focus_app_window();
                    let fname = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                    let (name, emoji) = get_image_badge(&ext);
                    self.set_toast(format!("⚡ 已開啟: {} ({} {})", fname, emoji, name));
                    return;
                }
            } else if is_pdf_extension(&ext) {
                if let Ok(bytes) = fs::read(path) {
                    if let Ok((pdf_md, page_count)) = crate::markdown::extract_text_from_pdf_bytes(&bytes) {
                        self.image_uri = None;
                        self.image_bytes = None;
                        self.view_mode = ViewMode::Markdown;
                        self.line_count = pdf_md.lines().count();
                        self.content = pdf_md.clone();
                        self.original_content = pdf_md;
                        self.current_file = Some(path.to_path_buf());

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

                        self.file_watcher.watch_file(path);
                        self.visible = true;
                        show_and_focus_app_window();
                        let fname = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                        self.set_toast(format!("⚡ 已開啟 PDF 快速文字預覽: {} (共 {} 頁)", fname, page_count));
                        return;
                    }
                }
            } else if let Ok(text) = fs::read_to_string(path) {
                self.image_uri = None;
                self.image_bytes = None;
                if matches!(ext.as_str(), "md" | "markdown" | "mdown" | "mkd") {
                    self.view_mode = ViewMode::Markdown;
                } else if ext == "csv" {
                    self.view_mode = ViewMode::Table { separator: ',' };
                } else if ext == "tsv" {
                    self.view_mode = ViewMode::Table { separator: '\t' };
                } else if is_code_extension(&ext) {
                    self.view_mode = ViewMode::Code { lang: ext.clone() };
                } else {
                    self.view_mode = ViewMode::PlainText;
                }

                self.line_count = text.lines().count();
                self.content = text.clone();
                self.original_content = text;
                self.current_file = Some(path.to_path_buf());

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

                self.file_watcher.watch_file(path);
                self.visible = true;
                show_and_focus_app_window();
                let fname = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                let mode_desc = match self.view_mode {
                    ViewMode::Markdown => "Markdown 渲染".to_string(),
                    ViewMode::Table { separator } => {
                        if separator == '\t' {
                            "TSV 資料表格 📊".to_string()
                        } else {
                            "CSV 資料表格 📊".to_string()
                        }
                    }
                    ViewMode::Code { ref lang } => {
                        let (name, emoji) = get_language_badge(lang);
                        format!("{} {} 語法高亮", emoji, name)
                    }
                    ViewMode::PlainText => "純文字模式".to_string(),
                    ViewMode::Image { ref format } => format!("{} 圖片預覽", format),
                };
                self.set_toast(format!("⚡ 已開啟: {} ({})", fname, mode_desc));
                return;
            }
        }

        let path_str = path.to_string_lossy().to_string();
        if let Some(zip_idx) = path_str.to_lowercase().find(".zip\\") {
            let zip_file_part = &path_str[..zip_idx + 4];
            let inner_entry = &path_str[zip_idx + 5..];
            let zip_path = PathBuf::from(zip_file_part);

            if zip_path.is_file() {
                let inner_entry_str = inner_entry.replace('\\', "/");
                let inner_ext = Path::new(&inner_entry_str)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if is_image_extension(&inner_ext) {
                    if let Ok(bytes) = read_bytes_from_zip(&zip_path, &inner_entry_str) {
                        let uri = format!("bytes://{}", inner_entry_str);
                        self.image_uri = Some(uri);
                        self.image_bytes = Some(bytes.clone());
                        self.image_zoom = 1.0;
                        self.image_fit_mode = true;
                        self.view_mode = ViewMode::Image { format: inner_ext.clone() };

                        if inner_ext == "svg" {
                            self.content = String::from_utf8_lossy(&bytes).to_string();
                            self.original_content = self.content.clone();
                            self.line_count = self.content.lines().count();
                        } else {
                            self.content.clear();
                            self.original_content.clear();
                            self.line_count = 0;
                        }

                        let len = bytes.len();
                        self.file_size_str = if len < 1024 {
                            format!("{} B", len)
                        } else if len < 1024 * 1024 {
                            format!("{:.1} KB", len as f64 / 1024.0)
                        } else {
                            format!("{:.2} MB", len as f64 / (1024.0 * 1024.0))
                        };
                        self.last_modified_str = "ZIP 壓縮檔".to_string();
                        self.current_file = Some(path.to_path_buf());
                        self.visible = true;
                        show_and_focus_app_window();

                        let fname = Path::new(&inner_entry_str).file_name().and_then(|f| f.to_str()).unwrap_or(&inner_entry_str);
                        let (name, emoji) = get_image_badge(&inner_ext);
                        self.set_toast(format!("⚡ 已自 ZIP 即時預覽: {} ({} {}) 📦", fname, emoji, name));
                        return;
                    }
                } else if is_pdf_extension(&inner_ext) {
                    if let Ok(bytes) = read_bytes_from_zip(&zip_path, &inner_entry_str) {
                        if let Ok((pdf_md, page_count)) = crate::markdown::extract_text_from_pdf_bytes(&bytes) {
                            self.image_uri = None;
                            self.image_bytes = None;
                            self.view_mode = ViewMode::Markdown;
                            self.line_count = pdf_md.lines().count();
                            self.content = pdf_md.clone();
                            self.original_content = pdf_md;
                            let len = bytes.len();
                            self.file_size_str = if len < 1024 {
                                format!("{} B", len)
                            } else if len < 1024 * 1024 {
                                format!("{:.1} KB", len as f64 / 1024.0)
                            } else {
                                format!("{:.2} MB", len as f64 / (1024.0 * 1024.0))
                            };
                            self.last_modified_str = "ZIP 壓縮檔".to_string();
                            self.current_file = Some(path.to_path_buf());
                            self.visible = true;
                            show_and_focus_app_window();

                            let fname = Path::new(&inner_entry_str).file_name().and_then(|f| f.to_str()).unwrap_or(&inner_entry_str);
                            self.set_toast(format!("⚡ 已自 ZIP 預覽 PDF: {} (共 {} 頁) 📦", fname, page_count));
                            return;
                        }
                    }
                } else {
                    if let Ok(bytes) = read_bytes_from_zip(&zip_path, &inner_entry_str) {
                        self.image_uri = None;
                        self.image_bytes = None;
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        if matches!(inner_ext.as_str(), "md" | "markdown" | "mdown" | "mkd") {
                            self.view_mode = ViewMode::Markdown;
                        } else if inner_ext == "csv" {
                            self.view_mode = ViewMode::Table { separator: ',' };
                        } else if inner_ext == "tsv" {
                            self.view_mode = ViewMode::Table { separator: '\t' };
                        } else if is_code_extension(&inner_ext) {
                            self.view_mode = ViewMode::Code { lang: inner_ext.clone() };
                        } else {
                            self.view_mode = ViewMode::PlainText;
                        }

                        self.line_count = text.lines().count();
                        let len = text.len();
                        self.file_size_str = if len < 1024 {
                            format!("{} B", len)
                        } else if len < 1024 * 1024 {
                            format!("{:.1} KB", len as f64 / 1024.0)
                        } else {
                            format!("{:.2} MB", len as f64 / (1024.0 * 1024.0))
                        };
                        self.last_modified_str = "ZIP 壓縮檔".to_string();
                        self.content = text.clone();
                        self.original_content = text;
                        self.current_file = Some(path.to_path_buf());
                        self.visible = true;
                        show_and_focus_app_window();

                        let fname = Path::new(&inner_entry_str).file_name().and_then(|f| f.to_str()).unwrap_or(&inner_entry_str);
                        self.set_toast(format!("⚡ 已自 ZIP 即時預覽: {} 📦", fname));
                        return;
                    }
                }
            }
        }

        error!("無法開啟檔案 {:?}", path);
        self.set_toast(format!("無法開啟檔案: {:?}", path.file_name().unwrap_or_default()));
        self.visible = true;
        show_and_focus_app_window();
    }

    pub fn save_current_file(&mut self, is_auto: bool) {
        if let Some(ref path) = self.current_file {
            if path.exists() {
                match fs::write(path, &self.content) {
                    Ok(_) => {
                        self.original_content = self.content.clone();
                        self.is_modified = false;
                        self.line_count = self.content.lines().count();
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
                        if is_auto {
                            self.set_toast("💾 已自動防抖保存".to_string());
                        } else {
                            self.set_toast("💾 檔案已成功保存！".to_string());
                        }
                    }
                    Err(e) => {
                        self.set_toast(format!("❌ 保存檔案失敗: {}", e));
                    }
                }
            }
        }
    }

    pub fn toggle_edit_mode(&mut self) {
        if matches!(self.view_mode, ViewMode::Image { .. }) && self.content.is_empty() {
            self.set_toast("ℹ️ 二進制圖片不支援文字編輯".to_string());
            return;
        }
        self.is_editing = !self.is_editing;
        if self.is_editing {
            self.set_toast("✏️ 已進入全螢幕就地編輯模式 (Ctrl+S 保存，E 退出)".to_string());
        } else {
            self.set_toast("👁️ 已切換至美化預覽模式".to_string());
        }
    }

    fn render_editor(&mut self, ui: &mut egui::Ui) {
        let out = crate::views::editor::render_editor(ui, self.theme, self.font_scale, &mut self.content);
        if out.changed {
            self.line_count = out.new_line_count;
            self.is_modified = self.content != self.original_content;
            self.last_edit_instant = Some(std::time::Instant::now());
        }
    }

    fn render_slides_mode(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
            self.set_toast("👁️ 已退出簡報投影模式".to_string());
        }
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

    pub fn reload_current_file(&mut self) {
        if self.is_modified {
            return;
        }
        if let Some(path) = self.current_file.clone() {
            if let Ok(text) = fs::read_to_string(&path) {
                self.line_count = text.lines().count();
                self.content = text.clone();
                self.original_content = text;
                self.is_modified = false;
                self.set_toast("檔案已自動即時同步更新 ⚡".to_string());
            }
        }
    }

    pub fn handle_hotkey_preview(&mut self, maybe_path: Option<PathBuf>) {
        // 若事件未帶路徑，嘗試二次即時查詢檔案總管
        let target_path = maybe_path.or_else(get_selected_file_from_explorer);

        if let Some(selected_path) = target_path {
            info!("快捷鍵觸發，載入檔案: {:?}", selected_path);
            if self.visible && self.current_file.as_deref() == Some(&selected_path) {
                // 如果已經在預覽同一檔案且視窗開啟中，則隱藏 (Quick Look 體驗)
                self.visible = false;
                hide_app_window();
            } else {
                self.load_file(&selected_path);
                self.visible = true;
                show_and_focus_app_window();
            }
        } else {
            // 沒有在檔案總管選取特定檔案
            if self.visible {
                // 若當前已經開啟，再次按下快捷鍵則隱藏收起視窗
                self.visible = false;
                hide_app_window();
            } else {
                // 若為隱藏狀態，且沒有選取任何檔案，重置為純淨的空狀態 (絕不殘留舊檔案！)
                self.current_file = None;
                self.content.clear();
                self.image_uri = None;
                self.image_bytes = None;
                self.line_count = 0;
                self.file_size_str.clear();
                self.last_modified_str.clear();
                self.visible = true;
                show_and_focus_app_window();
                self.set_toast("⚡ 已開啟 flash-md！(在檔案總管點選檔案後按 Alt+Space 可直接預覽)".to_string());
            }
        }
    }

    pub fn set_toast(&mut self, msg: String) {
        self.status_toast = Some((msg, std::time::Instant::now()));
    }

    /// 切換至同目錄下的上一個 / 下一個檔案 (依檔名自然排序)
    pub fn navigate_sibling_file(&mut self, forward: bool) {
        let current_path = match self.current_file.clone() {
            Some(p) => p,
            None => return,
        };

        let parent_dir = match current_path.parent() {
            Some(p) if p.exists() => p,
            _ => return,
        };

        // 讀取同目錄下的所有實體檔案
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        // 排除隱藏檔與暫存檔
                        if !name.starts_with('.') && !name.starts_with("~$") {
                            files.push(path);
                        }
                    }
                }
            }
        }

        if files.is_empty() {
            return;
        }

        // 依檔名不分大小寫排序
        files.sort_by(|a, b| {
            let name_a = a.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let name_b = b.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name_a.cmp(&name_b)
        });

        let current_idx = files.iter().position(|p| p == &current_path);

        let target_idx = match current_idx {
            Some(idx) => {
                if forward {
                    if idx + 1 < files.len() {
                        idx + 1
                    } else {
                        0 // 循環至第一筆
                    }
                } else {
                    if idx > 0 {
                        idx - 1
                    } else {
                        files.len() - 1 // 循環至最後一筆
                    }
                }
            }
            None => 0,
        };

        if let Some(target_path) = files.get(target_idx) {
            let target_path_clone = target_path.clone();
            let total_count = files.len();
            let display_idx = target_idx + 1;

            self.load_file(&target_path_clone);

            let file_name = target_path_clone
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            self.set_toast(format!("[{}/{}] ⚡ 已切換: {}", display_idx, total_count, file_name));
        }
    }

    /// 取得當前檔案在同目錄下的序號資訊，例如 (3, 18) 代表第 3 個，共 18 個檔案
    pub fn get_sibling_info(&self) -> Option<(usize, usize)> {
        let current_path = self.current_file.as_ref()?;
        let parent_dir = current_path.parent()?;
        if !parent_dir.exists() {
            return None;
        }

        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !name.starts_with('.') && !name.starts_with("~$") {
                            files.push(path);
                        }
                    }
                }
            }
        }

        if files.is_empty() {
            return None;
        }

        files.sort_by(|a, b| {
            let name_a = a.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let name_b = b.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name_a.cmp(&name_b)
        });

        let idx = files.iter().position(|p| p == current_path)?;
        Some((idx + 1, files.len()))
    }

    /// 取得目前文字中符合搜尋關鍵字的所有出現行號清單
    pub fn get_search_matches(&self) -> Vec<usize> {
        let q = self.search_query.trim();
        if q.is_empty() || self.content.is_empty() {
            return Vec::new();
        }
        let q_lower = q.to_lowercase();
        let mut matches = Vec::new();
        for (line_idx, line) in self.content.lines().enumerate() {
            let line_lower = line.to_lowercase();
            let mut start = 0;
            while let Some(pos) = line_lower[start..].find(&q_lower) {
                matches.push(line_idx);
                start += pos + q_lower.len();
            }
        }
        matches
    }

    /// 依方向導航搜尋結果 (next: true 為下一筆，false 為上一筆)
    pub fn navigate_search_match(&mut self, next: bool) {
        let matches = self.get_search_matches();
        if matches.is_empty() {
            return;
        }

        if next {
            self.search_match_index = (self.search_match_index + 1) % matches.len();
        } else {
            self.search_match_index = if self.search_match_index == 0 {
                matches.len() - 1
            } else {
                self.search_match_index - 1
            };
        }

        let target_line = matches[self.search_match_index];
        self.scroll_to_line(target_line);
    }

    /// 將視圖滾動至指定行號並居中
    pub fn scroll_to_line(&mut self, line_idx: usize) {
        let line_height = match self.view_mode {
            ViewMode::Markdown => 24.0 * self.font_scale,
            ViewMode::Table { .. } => 26.0 * self.font_scale,
            ViewMode::Code { .. } => 21.0 * self.font_scale,
            ViewMode::PlainText => 22.0 * self.font_scale,
            ViewMode::Image { .. } => return,
        };

        let target_y = (line_idx as f32) * line_height;
        // 預留頂部 180px 空間，使搜尋結果落在視窗上半部黃金視覺區域
        let target_offset = (target_y - 180.0).max(0.0);
        self.target_scroll_offset = Some(target_offset);
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd_open_file() {
            self.load_file(&path);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        // ESC: 隱藏或關閉視窗 (若簡報模式/設定/搜尋列/編輯模式開啟則優先關閉或退出)
        if input.key_pressed(egui::Key::Escape) {
            if self.is_slides_mode {
                self.is_slides_mode = false;
                if self.is_slides_fullscreen {
                    self.is_slides_fullscreen = false;
                    self.set_fullscreen_state(ctx, false);
                }
                self.set_toast("👁️ 已退出簡報投影模式".to_string());
            } else if self.settings_open {
                self.settings_open = false;
            } else if self.is_editing {
                self.is_editing = false;
                self.set_toast("👁️ 已退出編輯模式，回到預覽".to_string());
            } else if self.search_open {
                self.search_open = false;
                self.search_query.clear();
                self.search_match_index = 0;
            } else if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                self.set_fullscreen_state(ctx, false);
                self.set_toast("🗗 已退出全螢幕模式".to_string());
            } else {
                self.visible = false;
                hide_app_window();
                if self.is_standalone {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
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
                    || (input.key_pressed(egui::Key::H) && !input.modifiers.command && !input.modifiers.alt)
                {
                    self.navigate_sibling_file(false);
                } else if input.key_pressed(egui::Key::ArrowRight)
                    || (input.key_pressed(egui::Key::L) && !input.modifiers.command && !input.modifiers.alt)
                {
                    self.navigate_sibling_file(true);
                }
            }

            // ↑ / ↓ 或 j / k (Vim): 捲動瀏覽當前文件內容 (支援單擊與長按連續平滑捲動)
            let mut scroll_y = 0.0_f32;
            if input.key_pressed(egui::Key::ArrowDown) || input.key_down(egui::Key::ArrowDown)
                || input.key_pressed(egui::Key::J) || input.key_down(egui::Key::J)
            {
                scroll_y -= 36.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::ArrowUp) || input.key_down(egui::Key::ArrowUp)
                || input.key_pressed(egui::Key::K) || input.key_down(egui::Key::K)
            {
                scroll_y += 36.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::PageDown) {
                scroll_y -= 360.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::PageUp) {
                scroll_y += 360.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::Space) && !input.modifiers.alt && !input.modifiers.command {
                if input.modifiers.shift {
                    scroll_y += 360.0 * self.font_scale;
                } else {
                    scroll_y -= 360.0 * self.font_scale;
                }
            }
            // Home 或 g (Vim): 置頂；End 或 G / Shift+g (Vim): 置底
            if input.key_pressed(egui::Key::Home)
                || (input.key_pressed(egui::Key::G) && !input.modifiers.shift && !input.modifiers.command)
            {
                self.reset_scroll_to_top = true;
            }
            if input.key_pressed(egui::Key::End)
                || (input.key_pressed(egui::Key::G) && input.modifiers.shift && !input.modifiers.command)
            {
                scroll_y -= 100000.0;
            }

            if scroll_y != 0.0 {
                self.keyboard_scroll_delta += scroll_y;
                ctx.request_repaint();
            }
        }

        // F5 或 P: 切換全螢幕簡報投影模式 (非編輯/搜尋輸入狀態下)
        if !self.is_editing && !self.search_open && matches!(self.view_mode, ViewMode::Markdown) {
            if input.key_pressed(egui::Key::F5)
                || (input.key_pressed(egui::Key::P) && !input.modifiers.command && !input.modifiers.alt && !ctx.wants_keyboard_input())
                || (input.modifiers.command && input.key_pressed(egui::Key::P))
            {
                self.is_slides_mode = !self.is_slides_mode;
                if self.is_slides_mode {
                    self.current_slide_index = 0;
                    self.is_slides_fullscreen = true;
                    self.set_fullscreen_state(ctx, true);
                    self.set_toast("📽️ 已進入全螢幕簡報投影模式 (F5/Esc 退出，左右鍵翻頁)".to_string());
                } else {
                    if self.is_slides_fullscreen {
                        self.is_slides_fullscreen = false;
                        self.set_fullscreen_state(ctx, false);
                    }
                    self.set_toast("👁️ 已退出簡報投影模式".to_string());
                }
            }
        }

        // 簡報投影模式專屬鍵盤導航 (左右/Page/Space/Enter/翻頁/全螢幕)
        if self.is_slides_mode {
            let total_slides = crate::markdown::extract_slides(&self.content).len();
            if input.key_pressed(egui::Key::ArrowRight)
                || input.key_pressed(egui::Key::PageDown)
                || input.key_pressed(egui::Key::Space)
                || input.key_pressed(egui::Key::Enter)
                || (input.key_pressed(egui::Key::L) && !input.modifiers.command && !input.modifiers.alt)
            {
                if self.current_slide_index + 1 < total_slides {
                    self.current_slide_index += 1;
                    ctx.request_repaint();
                }
            }
            if input.key_pressed(egui::Key::ArrowLeft)
                || input.key_pressed(egui::Key::PageUp)
                || input.key_pressed(egui::Key::Backspace)
                || (input.key_pressed(egui::Key::H) && !input.modifiers.command && !input.modifiers.alt)
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
                self.set_toast(if next_fs { "⛶ 已進入全螢幕模式 (F11 退出)".to_string() } else { "🗗 已退出全螢幕模式".to_string() });
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
                self.set_toast(if self.toc_open { "已開啟目錄大綱 📑".to_string() } else { "已收起目錄大綱".to_string() });
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

            self.set_toast(match self.view_mode {
                ViewMode::Markdown => "已切換至 Markdown 渲染模式 📄".to_string(),
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

    /// 在 Windows 檔案總管中高亮定位目前檔案
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
        if let Ok(formatted) = crate::markdown::format_json(&self.content) {
            self.content = formatted;
            self.line_count = self.content.lines().count();
            self.set_toast("已完成 JSON 排版美化 ⚡".to_string());
        } else {
            self.set_toast("JSON 格式無效或解析失敗 ⚠️".to_string());
        }
    }

    /// 一鍵壓縮 JSON 為單行
    pub fn minify_json_content(&mut self) {
        self.content = crate::markdown::minify_json(&self.content);
        self.line_count = self.content.lines().count();
        self.set_toast("已壓縮為單行 JSON 📦".to_string());
    }

    /// 渲染 Markdown TOC 目錄大綱側邊欄，回傳 (是否收起大綱, 選取的目標標題錨點)
    pub fn render_toc_sidebar(&self, ui: &mut egui::Ui) -> (bool, Option<String>) {
        crate::views::toc_sidebar::render_toc_sidebar(ui, self.theme, self.font_scale, &self.content)
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

            let should_filter_enter = self.is_ime_composing || was_recent_ime || ime_event_this_frame;

            if should_filter_enter {
                let mut found_enter = false;
                i.events.retain(|ev| {
                    match ev {
                        egui::Event::Key { key: egui::Key::Enter, .. } => {
                            found_enter = true;
                            false
                        }
                        egui::Event::Text(s) if s == "\n" || s == "\r" || s == "\r\n" => {
                            found_enter = true;
                            false
                        }
                        _ => true,
                    }
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
            let is_updating = self.is_updating;

            let banner_bg = match self.theme {
                AppTheme::Dark => Color32::from_rgb(20, 30, 48),    // 質感暗夜深藍底
                AppTheme::Light => Color32::from_rgb(238, 246, 255), // 清爽透亮淺藍底
            };
            let banner_border = match self.theme {
                AppTheme::Dark => Color32::from_rgb(56, 189, 248),   // 科技青藍
                AppTheme::Light => Color32::from_rgb(186, 230, 253), // 柔和淺天藍
            };
            let text_color = match self.theme {
                AppTheme::Dark => Color32::from_rgb(224, 242, 254),  // 明亮淺白藍
                AppTheme::Light => Color32::from_rgb(12, 74, 110),   // 高對比深海軍藍 (極度清晰可讀)
            };
            let btn_primary_bg = match self.theme {
                AppTheme::Dark => Color32::from_rgb(14, 165, 233),   // 亮天藍
                AppTheme::Light => Color32::from_rgb(2, 132, 199),   // 深天藍
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
                            format!("⏳ 正在自動下載升級至 {} 並無縫重啟，請稍候...", release_tag)
                        } else {
                            format!("🎉 發現全新版本 {} (目前為 v{})！", release_tag, CURRENT_VERSION)
                        };

                        ui.label(
                            RichText::new(banner_text)
                                .color(text_color)
                                .strong()
                                .size(12.5),
                        );

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
                                            format!("✏️ 就地編輯模式\n• 儲存狀態: {}\n• 總行數: {} 行\n• 保存模式: {}", if self.is_modified { "已修改未保存" } else { "已保存" }, self.line_count, match self.config.save_mode { SaveMode::Manual => "按 Ctrl+S 手動保存", SaveMode::AutoDebounce => "打字停止自動防抖保存" }),
                                        )
                                    } else if let ViewMode::Image { ref format } = self.view_mode {
                                        (
                                            format!("{}{format_upper}  •  {}  •  {}", sibling_str, self.file_size_str, self.last_modified_str, format_upper = format.to_uppercase()),
                                            format!("🖼️ 圖片資訊\n• 格式: {}\n• 檔案大小: {}\n• 修改時間: {}", format.to_uppercase(), self.file_size_str, self.last_modified_str),
                                        )
                                    } else if matches!(self.view_mode, ViewMode::Markdown) {
                                        let stats = calculate_text_stats(&self.content);
                                        let words_str = if stats.cjk_chars > 0 && stats.words > 0 {
                                            format!("{} 中文 / {} 字", stats.cjk_chars, stats.words)
                                        } else if stats.cjk_chars > 0 {
                                            format!("{} 字", stats.cjk_chars)
                                        } else {
                                            format!("{} 詞", stats.words)
                                        };
                                        (
                                            format!("{}{} 行  •  {}  •  ⏱️ {} 分鐘  •  {}  •  {}", sibling_str, self.line_count, words_str, stats.reading_time_mins, self.file_size_str, self.last_modified_str),
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
                    let edit_btn_label = if self.is_editing { "👁️ 預覽" } else { "✏️ 編輯" };
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
                        if render_nav_button(ui, self.theme, "📽️ 簡報", self.is_slides_mode, "切換全螢幕簡報投影模式 (F5 或 P)").clicked() {
                            self.is_slides_mode = !self.is_slides_mode;
                            if self.is_slides_mode {
                                self.current_slide_index = 0;
                                self.is_slides_fullscreen = true;
                                self.set_fullscreen_state(ctx, true);
                                self.set_toast("📽️ 已進入全螢幕簡報投影模式 (F5/Esc 退出，左右鍵翻頁)".to_string());
                            } else {
                                if self.is_slides_fullscreen {
                                    self.is_slides_fullscreen = false;
                                    self.set_fullscreen_state(ctx, false);
                                }
                                self.set_toast("👁️ 已退出簡報投影模式".to_string());
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
                        if render_nav_button(ui, self.theme, "⚙️ 設定", self.settings_open, "偏好設定 (亮/暗色主題、自動防抖保存、字型縮放)").clicked() {
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

                        let matches = self.get_search_matches();
                        let match_count = matches.len();

                        // 當搜尋字串變更時，自動跳轉至第一筆相符項目
                        if search_input_resp.changed() {
                            self.search_match_index = 0;
                            if let Some(&first_line) = matches.first() {
                                self.scroll_to_line(first_line);
                            }
                        }

                        // 在搜尋框內按下 Enter 或 Shift + Enter 進行上一筆/下一筆跳轉
                        if search_input_resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
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

                                if ui.small_button(" ↔ ").on_hover_text("自適應視窗大小").clicked() {
                                    self.image_fit_mode = true;
                                }
                                if ui.small_button(" 1:1 ").on_hover_text("原始尺寸 100% (Ctrl + 0)").clicked() {
                                    self.image_zoom = 1.0;
                                    self.image_fit_mode = false;
                                }
                                if ui.small_button(" + ").on_hover_text("放大 (Ctrl + +)").clicked() {
                                    self.image_zoom = (self.image_zoom * 1.2).min(10.0);
                                    self.image_fit_mode = false;
                                }
                                if ui.small_button(" − ").on_hover_text("縮小 (Ctrl + -)").clicked() {
                                    self.image_zoom = (self.image_zoom / 1.2).max(0.1);
                                    self.image_fit_mode = false;
                                }
                            } else {
                                ui.label(
                                    RichText::new(format!("{}%", (self.font_scale * 100.0).round() as u32))
                                        .color(self.theme.text_secondary())
                                        .size(11.5),
                                );

                                if ui.small_button(" + ").on_hover_text("放大字體 (Ctrl + +)").clicked() {
                                    self.font_scale = (self.font_scale + 0.1).min(2.5);
                                }
                                if ui.small_button(" − ").on_hover_text("縮小字體 (Ctrl + -)").clicked() {
                                    self.font_scale = (self.font_scale - 0.1).max(0.6);
                                }
                                if ui.small_button(" 1:1 ").on_hover_text("重設字體 (Ctrl + 0)").clicked() {
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
        if self.toc_open && matches!(self.view_mode, ViewMode::Markdown) && !self.content.is_empty() {
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

                    let scroll_id = ui.make_persistent_id("main_content_scroll_area");
                    let mut scroll_state = egui::scroll_area::State::load(ui.ctx(), scroll_id).unwrap_or_default();

                    if self.reset_scroll_to_top {
                        scroll_state.offset.y = 0.0_f32;
                        scroll_state.store(ui.ctx(), scroll_id);
                    } else if let Some(offset) = self.target_scroll_offset {
                        scroll_state.offset.y = offset;
                        scroll_state.store(ui.ctx(), scroll_id);
                    } else if self.keyboard_scroll_delta != 0.0_f32 {
                        // 當向下捲動 (keyboard_scroll_delta < 0)，offset.y 需增加；向上捲動 (keyboard_scroll_delta > 0)，offset.y 需減少
                        scroll_state.offset.y = (scroll_state.offset.y - self.keyboard_scroll_delta).max(0.0_f32);
                        scroll_state.store(ui.ctx(), scroll_id);
                    }

                    match self.view_mode {
                        ViewMode::Markdown => {
                            // Markdown 富文字渲染模式 (支援即時搜尋關鍵字高亮、搜尋項目自動跳轉、滾輪重置回頂部、鍵盤方向鍵上下捲動與動態閱讀進度條)
                            let scroll = ScrollArea::vertical()
                                .id_source(scroll_id)
                                .auto_shrink([false, false]);

                            let scroll_out = scroll.show(ui, |ui| {
                                let anchor_to_jump = self.target_anchor.clone();
                                let base_dir = self.current_file.as_ref().and_then(|p| p.parent());
                                let renderer = MarkdownRenderer::new(
                                    self.theme,
                                    self.font_scale,
                                    &self.search_query,
                                    active_match_idx,
                                    anchor_to_jump.as_deref(),
                                    base_dir,
                                );
                                if let Some(clicked_anchor) = renderer.render(ui, &self.content) {
                                    self.target_anchor = Some(clicked_anchor);
                                    ctx.request_repaint();
                                } else if self.target_anchor.is_some() {
                                    self.target_anchor = None;
                                }
                            });

                            let max_scroll = (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(1.0);
                            self.reading_progress = (scroll_out.state.offset.y / max_scroll).clamp(0.0, 1.0);

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
                            let scroll = ScrollArea::both()
                                .id_source(scroll_id)
                                .auto_shrink([false, false]);

                            scroll.show(ui, |ui| {
                                let table_data = crate::markdown::parse_csv_or_tsv(&self.content, separator);
                                let mut match_counter = 0;
                                crate::markdown::render_csv_table(
                                    ui,
                                    self.theme,
                                    self.font_scale,
                                    &table_data,
                                    &self.search_query,
                                    active_match_idx,
                                    &mut match_counter,
                                );
                            });
                        }
                        ViewMode::Code { ref lang } => {
                            // 程式碼全語法高亮模式 (支援行號、關鍵字高亮、縮排、即時搜尋高亮與跳轉定位、滾輪重置與鍵盤捲動)
                            let scroll = ScrollArea::both()
                                .id_source(scroll_id)
                                .auto_shrink([false, false]);

                            scroll.show(ui, |ui| {
                                render_code_viewer(ui, self.theme, self.font_scale, &self.content, lang, &self.search_query, active_match_idx);
                            });
                        }
                        ViewMode::PlainText => {
                            // 純文字檢視模式 (針對 .txt 或其他純文字檔，原汁原味顯示並支援搜尋高亮與跳轉定位、滾輪重置與鍵盤捲動，快取 LayoutJob 零拷貝)
                            let scroll = ScrollArea::both()
                                .id_source(scroll_id)
                                .auto_shrink([false, false]);

                            scroll.show(ui, |ui| {
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

                                let cache_id = ui.make_persistent_id(format!(
                                    "plaintext_job_{:p}_{}_{}_{}_{:?}_{:?}",
                                    self.content.as_ptr(),
                                    self.content.len(),
                                    (font_scale * 100.0) as u32,
                                    self.search_query,
                                    active_match_idx,
                                    self.theme
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
                                        crate::markdown::append_highlighted_text(
                                            &mut job,
                                            &self.content,
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

                                ui.label(text_job);
                            });
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

/// 自 ZIP 壓縮檔內直接即時讀取文字檔案內容 (無需使用者手動解壓縮)
#[allow(dead_code)]
fn read_text_from_zip(zip_path: &Path, entry_name: &str) -> Result<String, String> {
    let zip_str = zip_path.to_string_lossy().replace('\'', "''");
    let entry_clean = entry_name.replace('/', "\\").replace('\'', "''");
    let entry_filename = Path::new(entry_name)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| entry_name.to_string())
        .replace('\'', "''");

    let script = format!(
        r#"[System.Reflection.Assembly]::LoadWithPartialName("System.IO.Compression.FileSystem") | Out-Null; $z = [System.IO.Compression.ZipFile]::OpenRead('{}'); $e = $z.Entries | Where-Object {{ $_.FullName.Replace('/','\') -eq '{}' -or $_.Name -eq '{}' }} | Select-Object -First 1; if ($e) {{ $s = $e.Open(); $r = New-Object System.IO.StreamReader($s, [System.Text.Encoding]::UTF8); $t = $r.ReadToEnd(); $r.Close(); $s.Close(); Write-Output $t }}; $z.Dispose();"#,
        zip_str, entry_clean, entry_filename
    );

    let output = std::process::Command::new("powershell")
        .args(&["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("執行 PowerShell 讀取 ZIP 失敗: {}", e))?;

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout).to_string();
        if !content.is_empty() {
            Ok(content)
        } else {
            Err("壓縮檔內找不到指定檔案或檔案內容為空".to_string())
        }
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// 自 ZIP 壓縮檔內直接即時讀取二進制檔案數據 (圖片/SVG/圖示)
fn read_bytes_from_zip(zip_path: &Path, entry_name: &str) -> Result<Vec<u8>, String> {
    let zip_str = zip_path.to_string_lossy().replace('\'', "''");
    let entry_clean = entry_name.replace('/', "\\").replace('\'', "''");
    let entry_filename = Path::new(entry_name)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| entry_name.to_string())
        .replace('\'', "''");

    let script = format!(
        r#"[System.Reflection.Assembly]::LoadWithPartialName("System.IO.Compression.FileSystem") | Out-Null; $z = [System.IO.Compression.ZipFile]::OpenRead('{}'); $e = $z.Entries | Where-Object {{ $_.FullName.Replace('/','\') -eq '{}' -or $_.Name -eq '{}' }} | Select-Object -First 1; if ($e) {{ $s = $e.Open(); $ms = New-Object System.IO.MemoryStream; $s.CopyTo($ms); [System.Console]::OpenStandardOutput().Write($ms.ToArray(), 0, $ms.Length); $s.Close(); $ms.Close(); }}; $z.Dispose();"#,
        zip_str, entry_clean, entry_filename
    );

    let output = std::process::Command::new("powershell")
        .args(&["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("執行 PowerShell 讀取 ZIP 失敗: {}", e))?;

    if output.status.success() && !output.stdout.is_empty() {
        Ok(output.stdout)
    } else {
        Err("ZIP 壓縮檔內找不到指定圖片檔案".to_string())
    }
}


