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

#[derive(Parser, Debug)]
#[command(
    name = "flash-md",
    author = "flash-md contributors",
    version = CURRENT_VERSION,
    about = "⚡ Windows 快捷鍵極速 Markdown 預覽工具 (Flash Quick Look for Windows)",
    long_about = "在 Windows 檔案總管或桌面選取 Markdown 檔案並按下 Alt + Space，即可閃電般彈出預覽視窗！亦可直接以命令列傳入檔案路徑預覽。"
)]
struct Cli {
    /// 直接開啟並預覽指定的 Markdown 或文字檔案路徑
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// 檢查並自動升級至 GitHub Releases 最新版本
    #[arg(short, long)]
    update: bool,

    /// 以背景常駐模式啟動 (監聽 Alt + Space 快捷鍵與系統匣圖示)
    #[arg(short, long, default_value_t = true)]
    daemon: bool,
}

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();

    // 處理命令列更新模式: flash-md --update
    if cli.update {
        println!("🔍 正在檢查 flash-md 最新版本 (目前版本: v{})...", CURRENT_VERSION);
        if let Some(release) = check_latest_release() {
            println!("🎉 發現新版本: {}！", release.tag_name);
            println!("📥 正在下載並自動更新...");
            match perform_self_update(&release) {
                Ok(_) => {
                    println!("✨ 恭喜！flash-md 已成功升級至 {}！", release.tag_name);
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("❌ 更新失敗: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            println!("✅ flash-md 目前已是最新版本 (v{})！", CURRENT_VERSION);
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

    // 啟動全域快捷鍵監聽 (Alt + Space 與備用快捷鍵)
    let _hotkey_handle = hotkey::start_hotkey_listener(hotkey_tx, ctx_holder.clone(), running.clone());

    // 建立系統匣常駐圖示
    let _tray_manager = tray::TrayManager::new(tray_tx, ctx_holder.clone());

    // 建立檔案監視器
    let file_watcher = FileWatcher::new(watcher_tx, ctx_holder.clone());

    // 設定 eframe 原生視窗選項
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("flash-md - 快捷鍵 Markdown 預覽")
            .with_inner_size([940.0, 700.0])
            .with_min_inner_size([500.0, 400.0])
            .with_decorations(true)
            .with_transparent(false)
            .with_visible(is_standalone || target_file.is_some()),
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
