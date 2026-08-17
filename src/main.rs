// 隱藏 Windows Release 模式下的額外終端機視窗 (如果是純 GUI 啟動)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod explorer;
mod hotkey;
mod markdown;
mod theme;
mod tray;
mod updater;
mod watcher;

use app::MdPreviewApp;
use clap::Parser;
use crossbeam_channel::unbounded;
use eframe::egui::ViewportBuilder;
use log::info;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use updater::{check_latest_release, perform_self_update, CURRENT_VERSION};
use watcher::FileWatcher;

#[cfg(windows)]
fn attach_parent_console() {
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "flash-md",
    author = "flash-md contributors",
    version = CURRENT_VERSION,
    about = "⚡ Windows 快捷鍵極速 Markdown 與全語言程式碼預覽工具 (Flash Quick Look for Windows)",
    long_about = "在 Windows 檔案總管或桌面選取 Markdown、程式碼或純文字檔案並按下 Alt + Space，即可閃電般彈出預覽視窗！亦可直接以命令列傳入檔案路徑預覽。"
)]
struct Cli {
    /// 直接開啟並預覽指定的 Markdown 或程式碼檔案路徑
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// 檢查並自動升級至 GitHub Releases 最新版本
    #[arg(short, long)]
    update: bool,

    /// 顯示目前 flash-md 版本號資訊
    #[arg(short = 'v', long = "version", action = clap::ArgAction::SetTrue)]
    version: bool,

    /// 以背景常駐模式啟動 (監聽 Alt + Space 快捷鍵與系統匣圖示)
    #[arg(short, long, default_value_t = true)]
    daemon: bool,
}

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    attach_parent_console();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();

    // 處理版本號查詢: flash-md --version 或 flash-md -v
    if cli.version {
        println!("⚡ flash-md v{} (Windows x86_64)", CURRENT_VERSION);
        println!("🚀 極速 macOS Quick Look 風格 Markdown 與全語言程式碼預覽工具");
        println!("🔗 專案首頁: https://github.com/BingFengHung/flash-md");
        std::process::exit(0);
    }

    // 處理命令列更新模式: flash-md --update
    if cli.update {
        println!("============================================================");
        println!("⚡ flash-md 自動更新檢查器 (目前本機版本: v{})", CURRENT_VERSION);
        println!("============================================================");
        println!("🔍 正在連線至 GitHub Releases 檢查最新版本發布...");
        
        if let Some(release) = check_latest_release() {
            println!("🎉 發現全新版本: {}！", release.tag_name);
            println!("📥 正在下載最新二進制發布檔並進行熱置換升級...");
            match perform_self_update(&release) {
                Ok(_) => {
                    println!("✨ 恭喜！flash-md 已成功自動升級至 {}！", release.tag_name);
                    println!("💡 您可以再次執行 flash-md 或按 Alt + Space 享受最新功能！");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("❌ 自動更新失敗: {}", e);
                    eprintln!("💡 您亦可手動前往下載: {}", release.html_url);
                    std::process::exit(1);
                }
            }
        } else {
            println!("✅ flash-md 目前已是最新版本 (v{})！無需進行更新。", CURRENT_VERSION);
            std::process::exit(0);
        }
    }

    info!("⚡ 啟動 flash-md v{}...", CURRENT_VERSION);

    let is_standalone = cli.file.is_some();
    let target_file = cli.file;

    // 通訊頻道
    let (hotkey_tx, hotkey_rx) = unbounded();
    let (watcher_tx, watcher_rx) = unbounded();
    let (tray_tx, tray_rx) = unbounded();

    let running = Arc::new(AtomicBool::new(true));
    let ctx_holder = Arc::new(Mutex::new(None));

    // 啟動全域快捷鍵掛鉤監聽 (WH_KEYBOARD_LL 攔截並吞噬 Alt + Space)
    let _hotkey_handle = hotkey::start_hotkey_listener(hotkey_tx, ctx_holder.clone(), running.clone());

    // 建立系統匣常駐圖示
    let _tray_manager = tray::TrayManager::new(tray_tx, ctx_holder.clone());

    // 建立檔案監視器
    let file_watcher = FileWatcher::new(watcher_tx, ctx_holder.clone());

    // 設定 eframe 原生視窗選項 (背景模式下完全不顯示黑框與視窗，真正安靜常駐)
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("flash-md - 快捷鍵 Markdown 預覽")
            .with_inner_size([940.0, 700.0])
            .with_min_inner_size([500.0, 400.0])
            .with_decorations(true)
            .with_transparent(false)
            .with_visible(is_standalone)
            .with_active(is_standalone),
        ..Default::default()
    };

    eframe::run_native(
        "flash-md",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(MdPreviewApp::new(
                cc,
                target_file,
                is_standalone,
                hotkey_rx,
                watcher_rx,
                tray_rx,
                file_watcher,
                ctx_holder,
            )))
        }),
    )
}
