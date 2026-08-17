use crossbeam_channel::Sender;
use log::error;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use std::thread;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuAction {
    OpenFile,
    ToggleTheme,
    ToggleAlwaysOnTop,
    About,
    Exit,
}

pub struct TrayManager {
    _tray_icon: TrayIcon,
}

impl TrayManager {
    pub fn new(action_sender: Sender<TrayMenuAction>) -> Option<Self> {
        let menu = Menu::new();

        let title_item = MenuItem::new("flash-md v0.2.4 (Alt + Space)", false, None);
        let open_item = MenuItem::new("📂 開啟 Markdown 檔案...", true, None);
        let theme_item = MenuItem::new("🎨 切換深淺色主題", true, None);
        let pin_item = MenuItem::new("📌 切換視窗置頂", true, None);
        let about_item = MenuItem::new("ℹ️ 關於 flash-md", true, None);
        let separator = PredefinedMenuItem::separator();
        let exit_item = MenuItem::new("❌ 結束程式", true, None);

        let open_id = open_item.id().clone();
        let theme_id = theme_item.id().clone();
        let pin_id = pin_item.id().clone();
        let about_id = about_item.id().clone();
        let exit_id = exit_item.id().clone();

        let _ = menu.append_items(&[
            &title_item,
            &PredefinedMenuItem::separator(),
            &open_item,
            &theme_item,
            &pin_item,
            &about_item,
            &separator,
            &exit_item,
        ]);

        let icon = create_default_tray_icon();

        let tray_icon = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("flash-md - 快捷鍵 Alt+Space 快速預覽 Markdown")
            .with_icon(icon)
            .build()
        {
            Ok(t) => t,
            Err(e) => {
                error!("建立系統匣圖示失敗: {:?}", e);
                return None;
            }
        };

        // 監聽 Menu 點擊事件
        thread::spawn(move || {
            let menu_channel = MenuEvent::receiver();
            while let Ok(event) = menu_channel.recv() {
                if event.id == open_id {
                    let _ = action_sender.send(TrayMenuAction::OpenFile);
                } else if event.id == theme_id {
                    let _ = action_sender.send(TrayMenuAction::ToggleTheme);
                } else if event.id == pin_id {
                    let _ = action_sender.send(TrayMenuAction::ToggleAlwaysOnTop);
                } else if event.id == about_id {
                    let _ = action_sender.send(TrayMenuAction::About);
                } else if event.id == exit_id {
                    let _ = action_sender.send(TrayMenuAction::Exit);
                }
            }
        });

        Some(Self {
            _tray_icon: tray_icon,
        })
    }
}

/// 產生一個簡約美觀的預設 32x32 RGBA 圖示 (青藍閃電方塊)
fn create_default_tray_icon() -> Icon {
    let width = 32;
    let height = 32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            // 外框圓角方形與青藍色背景
            let is_border = x == 0 || x == width - 1 || y == 0 || y == height - 1;
            let is_inside = x >= 3 && x < width - 3 && y >= 3 && y < height - 3;

            if is_inside {
                // 背景青藍色 (Deep Sky Blue / Indigo)
                rgba.extend_from_slice(&[59, 130, 246, 255]); // #3b82f6
            } else if is_border {
                rgba.extend_from_slice(&[0, 0, 0, 0]); // 透明
            } else {
                rgba.extend_from_slice(&[37, 99, 235, 230]); // 外層漸層邊界
            }
        }
    }

    Icon::from_rgba(rgba, width, height).unwrap_or_else(|_| {
        Icon::from_rgba(vec![255; (width * height * 4) as usize], width, height).unwrap()
    })
}
