use crate::explorer::show_and_focus_app_window;
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
    CheckUpdate,
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

        let title_item = MenuItem::new("⚡ flash-md (Alt + Space)", false, None);
        let open_item = MenuItem::new("📂 開啟檔案 (Open File)...", true, None);
        let theme_item = MenuItem::new("🎨 切換主題 (Toggle Theme)", true, None);
        let pin_item = MenuItem::new("📌 視窗置頂 (Always on Top)", true, None);
        let update_item = MenuItem::new("🔄 檢查更新 (Check Update)...", true, None);
        let about_item = MenuItem::new("ℹ️ 關於 flash-md (About)", true, None);
        let separator1 = PredefinedMenuItem::separator();
        let separator2 = PredefinedMenuItem::separator();
        let exit_item = MenuItem::new("✕ 結束程式 (Exit)", true, None);

        let open_id = open_item.id().clone();
        let theme_id = theme_item.id().clone();
        let pin_id = pin_item.id().clone();
        let update_id = update_item.id().clone();
        let about_id = about_item.id().clone();
        let exit_id = exit_item.id().clone();

        let _ = menu.append_items(&[
            &title_item,
            &separator1,
            &open_item,
            &theme_item,
            &pin_item,
            &update_item,
            &about_item,
            &separator2,
            &exit_item,
        ]);

        let icon = create_default_tray_icon();

        let tray_icon = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("flash-md - 快捷鍵 Alt+Space 閃電預覽")
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
                } else if event.id == update_id {
                    let _ = action_sender.send(TrayMenuAction::CheckUpdate);
                    sent = true;
                } else if event.id == about_id {
                    let _ = action_sender.send(TrayMenuAction::About);
                    sent = true;
                } else if event.id == exit_id {
                    let _ = action_sender.send(TrayMenuAction::Exit);
                    sent = true;
                }

                if sent {
                    // 透過 Win32 原生強制喚醒視窗並重繪
                    show_and_focus_app_window();
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

/// 產生 32x32 4x4 SSAA (16倍超採樣抗鋸齒) 的極致精緻電光藍 Squircle 閃電圖示
fn create_default_tray_icon() -> Icon {
    let width = 32usize;
    let height = 32usize;
    let mut rgba = Vec::with_capacity(width * height * 4);

    // 向量黃金比例閃電多邊形 (32x32 像素精確座標)
    let lightning_poly: [(f32, f32); 6] = [
        (18.0, 3.5),   // 頂端銳利尖點
        (9.5, 15.5),   // 中左外折角
        (15.2, 15.5),  // 中左內凹折
        (13.2, 28.5),  // 底部銳利尖點
        (22.5, 13.5),  // 中右外折角
        (16.8, 13.5),  // 中右內凹折
    ];

    let squircle_radius = 6.5_f32;
    let min_xy = 1.0_f32;
    let max_xy = 31.0_f32;

    // 4x4 超採樣採樣點偏移量 (Sub-pixel offsets)
    let sample_offsets = [0.125_f32, 0.375_f32, 0.625_f32, 0.875_f32];

    for y in 0..height {
        for x in 0..width {
            let mut accum_r = 0.0_f32;
            let mut accum_g = 0.0_f32;
            let mut accum_b = 0.0_f32;
            let mut accum_a = 0.0_f32;

            for &sy in &sample_offsets {
                for &sx in &sample_offsets {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;

                    // 1. 檢驗是否位於超橢圓圓角矩形 (Squircle) 內
                    let in_squircle = is_inside_rounded_rect(px, py, min_xy, min_xy, max_xy, max_xy, squircle_radius);

                    if in_squircle {
                        // 2. 檢驗是否位於向量閃電圖形內
                        let in_lightning = point_in_polygon(px, py, &lightning_poly);

                        if in_lightning {
                            // 閃電核心：純白帶微透電光青藍 (Pure Crisp White)
                            accum_r += 255.0;
                            accum_g += 255.0;
                            accum_b += 255.0;
                            accum_a += 255.0;
                        } else {
                            // 背景精緻漸層：現代曜石深灰 -> 極速湛藍 (Obsidian to Radiant Azure)
                            let t = (py - min_xy) / (max_xy - min_xy);
                            // 頂部: #0F172A (15, 23, 42) -> 底部: #2563EB (37, 99, 235)
                            let r = 15.0 * (1.0 - t) + 37.0 * t;
                            let g = 23.0 * (1.0 - t) + 99.0 * t;
                            let b = 42.0 * (1.0 - t) + 235.0 * t;

                            accum_r += r;
                            accum_g += g;
                            accum_b += b;
                            accum_a += 255.0;
                        }
                    }
                }
            }

            let final_r = (accum_r / 16.0).round().clamp(0.0, 255.0) as u8;
            let final_g = (accum_g / 16.0).round().clamp(0.0, 255.0) as u8;
            let final_b = (accum_b / 16.0).round().clamp(0.0, 255.0) as u8;
            let final_a = (accum_a / 16.0).round().clamp(0.0, 255.0) as u8;

            rgba.push(final_r);
            rgba.push(final_g);
            rgba.push(final_b);
            rgba.push(final_a);
        }
    }

    Icon::from_rgba(rgba, 32, 32).unwrap_or_else(|_| {
        Icon::from_rgba(vec![255; 32 * 32 * 4], 32, 32).unwrap()
    })
}

fn is_inside_rounded_rect(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if px < x0 || px > x1 || py < y0 || py > y1 {
        return false;
    }
    let left = x0 + r;
    let right = x1 - r;
    let top = y0 + r;
    let bottom = y1 - r;

    if px < left && py < top {
        let dx = px - left;
        let dy = py - top;
        return (dx * dx + dy * dy) <= (r * r);
    }
    if px > right && py < top {
        let dx = px - right;
        let dy = py - top;
        return (dx * dx + dy * dy) <= (r * r);
    }
    if px < left && py > bottom {
        let dx = px - left;
        let dy = py - bottom;
        return (dx * dx + dy * dy) <= (r * r);
    }
    if px > right && py > bottom {
        let dx = px - right;
        let dy = py - bottom;
        return (dx * dx + dy * dy) <= (r * r);
    }

    true
}

fn point_in_polygon(px: f32, py: f32, poly: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];

        let intersect = ((yi > py) != (yj > py))
            && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// 產生 64x64 4x4 SSAA 超採樣抗鋸齒的極致精緻電光藍 Squircle 閃電視窗圖示 (供 eframe / egui 視窗左上角與工作列使用)
pub fn create_app_icon_data() -> egui::IconData {
    let width = 64usize;
    let height = 64usize;
    let mut rgba = Vec::with_capacity(width * height * 4);

    let lightning_poly: [(f32, f32); 6] = [
        (36.5, 9.0),   // 頂端銳利尖點
        (19.5, 31.0),  // 中左外折角
        (30.8, 31.0),  // 中左內凹折
        (26.8, 55.0),  // 底部銳利尖點
        (44.5, 27.0),  // 中右外折角
        (33.2, 27.0),  // 中右內凹折
    ];

    let squircle_radius = 13.0_f32;
    let min_xy = 3.0_f32;
    let max_xy = 61.0_f32;
    let sample_offsets = [0.125_f32, 0.375_f32, 0.625_f32, 0.875_f32];

    for y in 0..height {
        for x in 0..width {
            let mut accum_r = 0.0_f32;
            let mut accum_g = 0.0_f32;
            let mut accum_b = 0.0_f32;
            let mut accum_a = 0.0_f32;

            for &sy in &sample_offsets {
                for &sx in &sample_offsets {
                    let px = x as f32 + sx;
                    let py = y as f32 + sy;

                    let in_squircle = is_inside_rounded_rect(px, py, min_xy, min_xy, max_xy, max_xy, squircle_radius);

                    if in_squircle {
                        let in_lightning = point_in_polygon(px, py, &lightning_poly);

                        if in_lightning {
                            accum_r += 255.0;
                            accum_g += 255.0;
                            accum_b += 255.0;
                            accum_a += 255.0;
                        } else {
                            let t = (py - min_xy) / (max_xy - min_xy);
                            let r = 15.0 * (1.0 - t) + 37.0 * t;
                            let g = 23.0 * (1.0 - t) + 99.0 * t;
                            let b = 42.0 * (1.0 - t) + 235.0 * t;

                            accum_r += r;
                            accum_g += g;
                            accum_b += b;
                            accum_a += 255.0;
                        }
                    }
                }
            }

            let final_r = (accum_r / 16.0).round().clamp(0.0, 255.0) as u8;
            let final_g = (accum_g / 16.0).round().clamp(0.0, 255.0) as u8;
            let final_b = (accum_b / 16.0).round().clamp(0.0, 255.0) as u8;
            let final_a = (accum_a / 16.0).round().clamp(0.0, 255.0) as u8;

            rgba.push(final_r);
            rgba.push(final_g);
            rgba.push(final_b);
            rgba.push(final_a);
        }
    }

    egui::IconData {
        rgba,
        width: 64,
        height: 64,
    }
}
