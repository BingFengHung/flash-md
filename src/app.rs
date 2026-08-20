use crate::explorer::{get_selected_file_from_explorer, hide_app_window, show_and_focus_app_window};
use crate::hotkey::HotkeyEvent;
use crate::markdown::{
    get_image_badge, get_language_badge, is_code_extension, is_image_extension,
    render_code_viewer, MarkdownRenderer,
};
use crate::theme::{setup_system_cjk_fonts, AppTheme};
use crate::tray::TrayMenuAction;
use crate::updater::{
    check_latest_release, perform_self_update, restart_with_new_version, ReleaseInfo,
    CURRENT_VERSION,
};
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
    Image { format: String },
}

pub struct MdPreviewApp {
    pub current_file: Option<PathBuf>,
    pub content: String,
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
            view_mode: ViewMode::Markdown,
            image_uri: None,
            image_bytes: None,
            image_zoom: 1.0,
            image_fit_mode: true,
            theme,
            font_scale: 1.0,
            always_on_top: false,
            visible: is_visible,
            is_standalone,
            search_open: false,
            search_query: String::new(),
            search_focus_requested: false,
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
        // 切換或載入檔案時自動將滾輪捲動至最頂部
        self.reset_scroll_to_top = true;

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

        // 1. 第一優先：直接嘗試以實體檔案讀取 (即使路徑中含有 .zip 資料夾名稱也能正常秒開)
        if path.exists() {
            if is_image_extension(&ext) {
                if let Ok(bytes) = fs::read(path) {
                    let path_str = path.to_string_lossy().to_string();
                    let uri = format!("file://{}", path_str.replace('\\', "/"));
                    self.image_uri = Some(uri);
                    self.image_bytes = Some(bytes.clone());
                    self.image_zoom = 1.0;
                    self.image_fit_mode = true;
                    self.view_mode = ViewMode::Image { format: ext.clone() };

                    // 若為 SVG，同時保留源碼供切換至 Code 檢視
                    if ext == "svg" {
                        self.content = String::from_utf8_lossy(&bytes).to_string();
                        self.line_count = self.content.lines().count();
                    } else {
                        self.content.clear();
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
            } else if let Ok(text) = fs::read_to_string(path) {
                self.image_uri = None;
                self.image_bytes = None;
                if matches!(ext.as_str(), "md" | "markdown" | "mdown" | "mkd") {
                    self.view_mode = ViewMode::Markdown;
                } else if is_code_extension(&ext) {
                    self.view_mode = ViewMode::Code { lang: ext.clone() };
                } else {
                    self.view_mode = ViewMode::PlainText;
                }

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
                let mode_desc = match self.view_mode {
                    ViewMode::Markdown => "Markdown 渲染".to_string(),
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

        // 2. 若直接讀取失敗，檢查是否位於未解壓縮之 .zip 虛擬目錄內
        let path_str = path.to_string_lossy().to_string();
        let lower_path = path_str.to_lowercase();

        if lower_path.contains(".zip\\") || lower_path.contains(".zip/") {
            if let Some(zip_idx) = lower_path.find(".zip\\").or_else(|| lower_path.find(".zip/")) {
                let zip_file_path_str = &path_str[..zip_idx + 4];
                let inner_entry_str = &path_str[zip_idx + 5..];
                let zip_path = PathBuf::from(zip_file_path_str);

                if zip_path.exists() {
                    info!("偵測到 ZIP 壓縮檔內虛擬路徑，嘗試即時解壓預覽: {:?} -> {}", zip_path, inner_entry_str);
                    self.set_toast(format!("📦 正在自 ZIP 壓縮檔即時讀取 {}...", inner_entry_str));

                    let ext = Path::new(inner_entry_str)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();

                    if is_image_extension(&ext) {
                        if let Ok(bytes) = read_bytes_from_zip(&zip_path, inner_entry_str) {
                            let uri = format!("bytes://{}", inner_entry_str);
                            self.image_uri = Some(uri);
                            self.image_bytes = Some(bytes.clone());
                            self.image_zoom = 1.0;
                            self.image_fit_mode = true;
                            self.view_mode = ViewMode::Image { format: ext.clone() };

                            if ext == "svg" {
                                self.content = String::from_utf8_lossy(&bytes).to_string();
                                self.line_count = self.content.lines().count();
                            } else {
                                self.content.clear();
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

                            let fname = Path::new(inner_entry_str).file_name().and_then(|f| f.to_str()).unwrap_or(inner_entry_str);
                            let (name, emoji) = get_image_badge(&ext);
                            self.set_toast(format!("⚡ 已自 ZIP 即時預覽: {} ({} {}) 📦", fname, emoji, name));
                            return;
                        }
                    } else if let Ok(text) = read_text_from_zip(&zip_path, inner_entry_str) {
                        self.image_uri = None;
                        self.image_bytes = None;
                        if matches!(ext.as_str(), "md" | "markdown" | "mdown" | "mkd") {
                            self.view_mode = ViewMode::Markdown;
                        } else if is_code_extension(&ext) {
                            self.view_mode = ViewMode::Code { lang: ext.clone() };
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
                        self.content = text;
                        self.current_file = Some(path.to_path_buf());
                        self.visible = true;
                        show_and_focus_app_window();

                        let fname = Path::new(inner_entry_str).file_name().and_then(|f| f.to_str()).unwrap_or(inner_entry_str);
                        self.set_toast(format!("⚡ 已自 ZIP 即時預覽: {} 📦", fname));
                        return;
                    }
                }
            }
        }

        // 3. 檔案真正無法開啟
        error!("無法開啟檔案 {:?}", path);
        self.set_toast(format!("無法開啟檔案: {:?}", path.file_name().unwrap_or_default()));
        self.visible = true;
        show_and_focus_app_window();
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

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd_open_file() {
            self.load_file(&path);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        // ESC: 隱藏或關閉視窗 (若搜尋列開啟則優先關閉搜尋列)
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

        // 鍵盤導航與瀏覽操作 (非文字編輯/搜尋輸入狀態下觸發)
        if !ctx.wants_keyboard_input() {
            // ← / →: 切換同目錄上一個 / 下一個檔案
            if self.current_file.is_some() {
                if input.key_pressed(egui::Key::ArrowLeft) {
                    self.navigate_sibling_file(false);
                } else if input.key_pressed(egui::Key::ArrowRight) {
                    self.navigate_sibling_file(true);
                }
            }

            // ↑ / ↓: 捲動瀏覽當前文件內容 (支援單擊與長按連續平滑捲動)
            let mut scroll_y = 0.0_f32;
            if input.key_pressed(egui::Key::ArrowDown) || input.key_down(egui::Key::ArrowDown) {
                scroll_y -= 32.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::ArrowUp) || input.key_down(egui::Key::ArrowUp) {
                scroll_y += 32.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::PageDown) {
                scroll_y -= 360.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::PageUp) {
                scroll_y += 360.0 * self.font_scale;
            }
            if input.key_pressed(egui::Key::Home) {
                self.reset_scroll_to_top = true;
            }
            if input.key_pressed(egui::Key::End) {
                scroll_y -= 100000.0;
            }

            if scroll_y != 0.0 {
                self.keyboard_scroll_delta += scroll_y;
                ctx.request_repaint();
            }
        }

        // Ctrl + F: 搜尋開關與自動聚焦
        if input.modifiers.command && input.key_pressed(egui::Key::F) {
            if !self.search_open {
                self.search_open = true;
            }
            self.search_focus_requested = true;
        }

        // Ctrl + O: 在外部預設編輯器開啟
        if input.modifiers.command && input.key_pressed(egui::Key::O) {
            if let Some(ref path) = self.current_file {
                let _ = open::that(path);
            }
        }

        // Ctrl + M: 切換 Markdown 預覽 / 程式碼語法高亮 / 純文字模式 / 圖片檢視模式
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
                    } else if is_code_extension(&ext) {
                        ViewMode::Code { lang: ext }
                    } else {
                        ViewMode::PlainText
                    }
                }
                ViewMode::Code { .. } => {
                    if is_image_extension(&ext) {
                        ViewMode::Image { format: ext }
                    } else {
                        ViewMode::PlainText
                    }
                }
                ViewMode::PlainText => {
                    if is_image_extension(&ext) {
                        ViewMode::Image { format: ext }
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

        // 頂部現代精緻導航列 (Fluent / macOS 玻璃質感風格，雙階層防遮擋設計)
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

                    // 模式切換膠囊 (支援 Markdown / 語言語法高亮 / 純文字 / 圖片向量圖)
                    if !self.content.is_empty() || self.image_uri.is_some() {
                        let (badge_text, badge_tip) = match self.view_mode {
                            ViewMode::Markdown => ("📄 Markdown".to_string(), "目前為 Markdown 模式 (點擊切換 Ctrl+M)".to_string()),
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
                                    } else if is_code_extension(&ext) {
                                        ViewMode::Code { lang: ext }
                                    } else {
                                        ViewMode::PlainText
                                    }
                                }
                                ViewMode::Code { .. } => {
                                    if is_image_extension(&ext) {
                                        ViewMode::Image { format: ext }
                                    } else {
                                        ViewMode::PlainText
                                    }
                                }
                                ViewMode::PlainText => {
                                    if is_image_extension(&ext) {
                                        ViewMode::Image { format: ext }
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

                                    let info_text = if let ViewMode::Image { ref format } = self.view_mode {
                                        format!("{}{format_upper}  •  {}  •  {}", sibling_str, self.file_size_str, self.last_modified_str, format_upper = format.to_uppercase())
                                    } else {
                                        format!("{}{} 行  •  {}  •  {}", sibling_str, self.line_count, self.file_size_str, self.last_modified_str)
                                    };
                                    ui.label(
                                        RichText::new(info_text)
                                            .size(10.5)
                                            .color(self.theme.text_secondary()),
                                    );
                                });
                        });
                    }
                });

                ui.add_space(5.0);

                // 第二階：現代精緻功能工具按鈕列
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    // 開啟檔案按鈕
                    if render_nav_button(ui, self.theme, "📂 開啟", false, "開啟本機 Markdown、程式碼或圖片檔案").clicked() {
                        self.open_file_dialog();
                    }

                    // 搜尋按鈕 (僅文字/程式碼模式可用)
                    if !matches!(self.view_mode, ViewMode::Image { .. }) {
                        if render_nav_button(ui, self.theme, "🔍 搜尋", self.search_open, "搜尋關鍵字 (Ctrl + F)").clicked() {
                            self.search_open = !self.search_open;
                            if self.search_open {
                                self.search_focus_requested = true;
                            }
                        }
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

                    // 第二階右側：視窗控制與主題切換
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
                        }

                        // 主題切換按鈕 (使用同字元家族的 🔆 與 🌙 保持一致的字圖間距)
                        let (theme_icon, theme_tip) = match self.theme {
                            AppTheme::Dark => ("🔆 淺色", "切換為淺色主題"),
                            AppTheme::Light => ("🌙 深色", "切換為深色主題"),
                        };
                        if render_nav_button(ui, self.theme, theme_icon, false, theme_tip).clicked() {
                            self.theme.toggle();
                            self.theme.apply_to_ctx(ctx);
                        }
                    });
                });

                // 搜尋列展開區 (Ctrl + F)
                if self.search_open && !matches!(self.view_mode, ViewMode::Image { .. }) {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍 尋找內文:").size(12.5).color(self.theme.accent_color()).strong());
                        let search_input_resp = ui.add(
                            TextEdit::singleline(&mut self.search_query)
                                .hint_text("輸入搜尋關鍵字...")
                                .desired_width(260.0),
                        );

                        if self.search_focus_requested {
                            search_input_resp.request_focus();
                            self.search_focus_requested = false;
                        }

                        let query_clean = self.search_query.trim();
                        if !query_clean.is_empty() {
                            let match_count = self.content.to_lowercase().matches(&query_clean.to_lowercase()).count();
                            let count_text = if match_count > 0 {
                                format!("找到 {} 筆相符", match_count)
                            } else {
                                "無相符項目".to_string()
                            };
                            let count_color = if match_count > 0 {
                                self.theme.accent_color()
                            } else {
                                self.theme.text_secondary()
                            };
                            ui.label(
                                RichText::new(count_text)
                                    .size(11.5)
                                    .color(count_color)
                                    .strong(),
                            );
                        }

                        if ui.button(RichText::new("✕ 清除").size(11.0)).clicked() {
                            self.search_query.clear();
                        }
                        if ui.button(RichText::new("關閉 (Esc)").size(11.0)).clicked() {
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

        // 主預覽渲染檢視區域 (Markdown / 全語言程式碼語法高亮 / 純文字 / 圖片向量圖)
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(self.theme.bg_color())
                    .inner_margin(Margin::symmetric(24.0, 16.0)),
            )
            .show(ctx, |ui| {
                if self.content.is_empty() && self.image_uri.is_none() {
                    // 極具現代質感的空狀態卡片介面 (Raycast / Linear Style)
                    self.render_empty_state(ui);
                } else {
                    match self.view_mode {
                        ViewMode::Markdown => {
                            // Markdown 富文字渲染模式 (支援即時搜尋關鍵字高亮、滾輪重置回頂部與鍵盤方向鍵上下捲動)
                            let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
                            if self.reset_scroll_to_top {
                                scroll = scroll.vertical_scroll_offset(0.0);
                            }
                            scroll.show(ui, |ui| {
                                if self.keyboard_scroll_delta != 0.0 {
                                    ui.scroll_with_delta(Vec2::new(0.0, self.keyboard_scroll_delta));
                                }
                                let renderer = MarkdownRenderer::new(self.theme, self.font_scale, &self.search_query);
                                renderer.render(ui, &self.content);
                            });
                        }
                        ViewMode::Code { ref lang } => {
                            // 程式碼全語法高亮模式 (支援行號、關鍵字高亮、縮排、即時搜尋高亮、滾輪重置與鍵盤捲動)
                            let mut scroll = ScrollArea::both().auto_shrink([false, false]);
                            if self.reset_scroll_to_top {
                                scroll = scroll.scroll_offset(Vec2::ZERO);
                            }
                            scroll.show(ui, |ui| {
                                if self.keyboard_scroll_delta != 0.0 {
                                    ui.scroll_with_delta(Vec2::new(0.0, self.keyboard_scroll_delta));
                                }
                                render_code_viewer(ui, self.theme, self.font_scale, &self.content, lang, &self.search_query);
                            });
                        }
                        ViewMode::PlainText => {
                            // 純文字檢視模式 (針對 .txt 或其他純文字檔，原汁原味顯示並支援搜尋高亮、滾輪重置與鍵盤捲動，快取 LayoutJob 零拷貝)
                            let mut scroll = ScrollArea::both().auto_shrink([false, false]);
                            if self.reset_scroll_to_top {
                                scroll = scroll.scroll_offset(Vec2::ZERO);
                            }
                            scroll.show(ui, |ui| {
                                if self.keyboard_scroll_delta != 0.0 {
                                    ui.scroll_with_delta(Vec2::new(0.0, self.keyboard_scroll_delta));
                                }
                                ui.add_space(4.0);
                                let font_scale = self.font_scale;
                                let font_id = FontId::monospace(14.0 * font_scale);
                                let text_color = self.theme.text_primary();
                                let hl_bg = match self.theme {
                                    AppTheme::Dark => Color32::from_rgba_unmultiplied(234, 179, 8, 180),
                                    AppTheme::Light => Color32::from_rgb(254, 240, 138),
                                };
                                let hl_fg = match self.theme {
                                    AppTheme::Dark => Color32::BLACK,
                                    AppTheme::Light => Color32::from_rgb(113, 63, 18),
                                };

                                let cache_id = ui.make_persistent_id(format!(
                                    "plaintext_job_{:p}_{}_{}_{}_{:?}",
                                    self.content.as_ptr(),
                                    self.content.len(),
                                    (font_scale * 100.0) as u32,
                                    self.search_query,
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
                                        crate::markdown::append_highlighted_text(&mut job, &self.content, &self.search_query, base_fmt, hl_bg, hl_fg);
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

        // 渲染完成後清除滾輪回到頂部與鍵盤捲動旗標，允許使用者後續正常捲動
        self.reset_scroll_to_top = false;
        self.keyboard_scroll_delta = 0.0;
    }
}

impl MdPreviewApp {
    fn render_bottom_tips(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!(
                "flash-md v{}  •  快捷鍵: Alt + Space (預覽)  •  ← / → (切換檔案)  •  ↑ / ↓ (捲動瀏覽)  •  Ctrl + F (搜尋)  •  Ctrl + M (切換模式)  •  Esc (隱藏)",
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
                .inner_margin(Margin::symmetric(36.0, 32.0))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        // 現代極光藍發光品牌圖示
                        Frame::none()
                            .fill(self.theme.accent_bg())
                            .rounding(Rounding::same(20.0))
                            .stroke(Stroke::new(1.5_f32, self.theme.accent_color()))
                            .inner_margin(Margin::symmetric(14.0, 10.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("⚡")
                                        .size(26.0)
                                        .strong()
                                        .color(self.theme.accent_color()),
                                );
                            });

                        ui.add_space(14.0);

                        ui.label(
                            RichText::new(format!("flash-md v{}", CURRENT_VERSION))
                                .size(19.0)
                                .strong()
                                .color(self.theme.text_primary()),
                        );

                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Windows 快捷鍵極速檔案預覽 • 毫秒級渲染")
                                .size(13.0)
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

                        ui.add_space(22.0);

                        // 選擇檔案按鈕
                        let browse_btn = ui.add_sized(
                            Vec2::new(180.0, 34.0),
                            egui::Button::new(
                                RichText::new("📂 瀏覽開啟檔案")
                                    .size(13.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(self.theme.accent_color())
                            .rounding(Rounding::same(7.0)),
                        );

                        if browse_btn.clicked() {
                            self.open_file_dialog();
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(10.0);

                        // 特色小標
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("⚡ 毫秒級預覽  •  📄 Markdown  •  💻 全語言程式碼高亮  •  🔄 即時同步")
                                    .size(11.0)
                                    .color(self.theme.text_secondary()),
                            );
                        });
                    });
                });
        });
    }
}

/// 繪製導覽列現代按鈕元件
fn render_nav_button(
    ui: &mut egui::Ui,
    theme: AppTheme,
    label: &str,
    is_active: bool,
    tooltip: &str,
) -> egui::Response {
    let bg = if is_active {
        theme.accent_bg()
    } else {
        theme.code_bg_color()
    };
    let border = if is_active {
        theme.accent_color()
    } else {
        theme.border_color()
    };
    let text_color = if is_active {
        theme.accent_color()
    } else {
        theme.text_secondary()
    };

    let btn = egui::Button::new(
        RichText::new(label)
            .size(11.5)
            .color(text_color),
    )
    .fill(bg)
    .stroke(Stroke::new(1.0_f32, border))
    .rounding(Rounding::same(5.0));

    ui.add(btn).on_hover_text(tooltip)
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

impl MdPreviewApp {
    /// 繪製圖片與 SVG 向量圖檢視畫布 (支援滾輪縮放、平移與自適應視窗)
    fn render_image_viewer(&mut self, ui: &mut egui::Ui) {
        if let Some(ref uri) = self.image_uri {
            let available = ui.available_size();

            // 監聽滾輪縮放
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta != 0.0 {
                if scroll_delta > 0.0 {
                    self.image_zoom = (self.image_zoom * 1.15).min(10.0);
                } else {
                    self.image_zoom = (self.image_zoom / 1.15).max(0.1);
                }
                self.image_fit_mode = false;
            }

            let mut scroll = ScrollArea::both().auto_shrink([false, false]);
            if self.reset_scroll_to_top {
                scroll = scroll.scroll_offset(Vec2::ZERO);
            }
            scroll.show(ui, |ui| {
                if self.keyboard_scroll_delta != 0.0 {
                    ui.scroll_with_delta(Vec2::new(0.0, self.keyboard_scroll_delta));
                }
                ui.centered_and_justified(|ui| {
                        let mut img = egui::Image::from_uri(uri.clone())
                            .rounding(Rounding::same(6.0));

                        if self.image_fit_mode {
                            let max_w = (available.x - 24.0).max(100.0);
                            let max_h = (available.y - 24.0).max(100.0);
                            img = img.max_size(Vec2::new(max_w, max_h));
                        } else {
                            img = img.fit_to_original_size(self.image_zoom);
                        }

                        ui.add(img);
                    });
                });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("無法載入圖片或向量圖").color(self.theme.text_secondary()));
            });
        }
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


