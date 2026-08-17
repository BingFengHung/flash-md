use crossbeam_channel::Sender;
use egui::Context;
use log::{debug, error, info};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
};

pub const HOTKEY_ID_ALT_SPACE: i32 = 1001;
pub const HOTKEY_ID_ALT_SHIFT_SPACE: i32 = 1002;
pub const HOTKEY_ID_CTRL_SHIFT_SPACE: i32 = 1003;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    TriggerPreview,
}

/// 啟動全域快捷鍵監聽執行緒 (註冊 Alt+Space, Alt+Shift+Space, Ctrl+Shift+Space)
pub fn start_hotkey_listener(
    sender: Sender<HotkeyEvent>,
    ctx_holder: Arc<Mutex<Option<Context>>>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("hotkey-listener".to_string())
        .spawn(move || {
            info!("啟動 Windows 全域快捷鍵監聽執行緒...");

            unsafe {
                let vk = VK_SPACE.0 as u32;

                // 1. 主要快捷鍵: Alt + Space
                let reg1 = RegisterHotKey(
                    HWND(0 as _),
                    HOTKEY_ID_ALT_SPACE,
                    MOD_ALT | MOD_NOREPEAT,
                    vk,
                );
                if reg1.is_ok() {
                    info!("✅ 成功註冊全域快捷鍵: Alt + Space");
                } else {
                    error!(
                        "⚠️ 註冊 Alt + Space 失敗 (可能被系統選單或 PowerToys 占用)，嘗試備用快捷鍵..."
                    );
                }

                // 2. 備用快捷鍵 1: Alt + Shift + Space
                let _ = RegisterHotKey(
                    HWND(0 as _),
                    HOTKEY_ID_ALT_SHIFT_SPACE,
                    MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                    vk,
                );

                // 3. 備用快捷鍵 2: Ctrl + Shift + Space
                let _ = RegisterHotKey(
                    HWND(0 as _),
                    HOTKEY_ID_CTRL_SHIFT_SPACE,
                    MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                    vk,
                );

                let mut msg = MSG::default();
                // Win32 Message Loop
                while running.load(Ordering::Relaxed) {
                    let ret = GetMessageW(&mut msg, HWND(0 as _), 0, 0);
                    if ret.0 <= 0 {
                        break;
                    }

                    if msg.message == WM_HOTKEY {
                        let id = msg.wParam.0 as i32;
                        if id == HOTKEY_ID_ALT_SPACE
                            || id == HOTKEY_ID_ALT_SHIFT_SPACE
                            || id == HOTKEY_ID_CTRL_SHIFT_SPACE
                        {
                            debug!("接收到全域快捷鍵 (ID: {}) 觸發事件！", id);
                            let _ = sender.send(HotkeyEvent::TriggerPreview);

                            // 關鍵：立刻喚醒 egui 事件迴圈，避免視窗在隱藏/閒置時無法接收事件
                            if let Ok(guard) = ctx_holder.lock() {
                                if let Some(ref ctx) = *guard {
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }

                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // 解除註冊
                let _ = UnregisterHotKey(HWND(0 as _), HOTKEY_ID_ALT_SPACE);
                let _ = UnregisterHotKey(HWND(0 as _), HOTKEY_ID_ALT_SHIFT_SPACE);
                let _ = UnregisterHotKey(HWND(0 as _), HOTKEY_ID_CTRL_SHIFT_SPACE);
                info!("全域快捷鍵已解除註冊");
            }
        })
        .expect("無法建立快捷鍵監聽執行緒")
}
