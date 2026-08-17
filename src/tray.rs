use crossbeam_channel::Sender;
use egui::Context;
use log::error;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use std::sync::{Arc, Mutex};
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
    pub fn new(
        action_sender: Sender<TrayMenuAction>,
        ctx_holder: Arc<Mutex<Option<Context>>>,
    ) -> Option<Self> {
        let menu = Menu::new();

        let title_item = MenuItem::new("flash-md (Alt + Space)", false, None);
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
                let mut sent = false;
                if event.id == open_id {
                    let _ = action_sender.send(TrayMenuAction::OpenFile);
                    sent = true;
                } else if event.id == theme_id {
                    let _ = action_sender.send(TrayMenuAction::ToggleTheme);
                    sent = true;
                } else if event.id == pin_id {
                    let _ = action_sender.send(TrayMenuAction::ToggleAlwaysOnTop);
                    sent = true;
                } else if event.id == about_id {
                    let _ = action_sender.send(TrayMenuAction::About);
                    sent = true;
                } else if event.id == exit_id {
                    let _ = action_sender.send(TrayMenuAction::Exit);
                    sent = true;
                }

                if sent {
                    if let Ok(guard) = ctx_holder.lock() {
                        if let Some(ref ctx) = *guard {
                            ctx.request_repaint();
                        }
                    }
                }
            }
        });

        Some(Self {
            _tray_icon: tray_icon,
        })
    }
}

/// 產生精緻的 32x32 閃電發光圖示 (Squircle 藍色圓角背景 + 亮白閃電符號)
fn create_default_tray_icon() -> Icon {
    let width = 32usize;
    let height = 32usize;
    let mut rgba = Vec::with_capacity(width * height * 4);

    for y in 0..height {
        for x in 0..width {
            let fx = x as f32;
            let fy = y as f32;

            let cx = 15.5_f32;
            let cy = 15.5_f32;
            let dx = (fx - cx).abs();
            let dy = (fy - cy).abs();
            let corner_dist = if dx > 10.0 && dy > 10.0 {
                ((dx - 10.0).powi(2) + (dy - 10.0).powi(2)).sqrt()
            } else {
                0.0
            };

            let in_squircle = dx <= 14.5 && dy <= 14.5 && corner_dist <= 4.5;

            let is_lightning = (x >= 14 && x <= 18 && y >= 6 && y <= 13)
                || (x >= 11 && x <= 22 && y == 14)
                || (x >= 10 && x <= 20 && y == 15)
                || (x >= 9 && x <= 17 && y == 16)
                || (x >= 13 && x <= 17 && y >= 17 && y <= 25 && (x + y >= 32 && x <= y - 5));

            if !in_squircle {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else if is_lightning {
                rgba.extend_from_slice(&[255, 255, 255, 255]);
            } else {
                let gradient = (fy / 32.0_f32) * 40.0;
                let r = (2.0 - gradient * 0.05).clamp(0.0, 255.0) as u8;
                let g = (132.0 - gradient).clamp(0.0, 255.0) as u8;
                let b = (220.0 - gradient * 0.5).clamp(0.0, 255.0) as u8;
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
    }

    Icon::from_rgba(rgba, 32, 32).unwrap_or_else(|_| {
        Icon::from_rgba(vec![255; 32 * 32 * 4], 32, 32).unwrap()
    })
}
