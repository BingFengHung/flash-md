use crossbeam_channel::Sender;
use log::{debug, error, info};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_NOREPEAT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
};

pub const HOTKEY_ID_PREVIEW: i32 = 1001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    TriggerPreview,
}

/// 啟動全域快捷鍵監聽執行緒 (預設監聽 Alt + Space)
pub fn start_hotkey_listener(
    sender: Sender<HotkeyEvent>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("hotkey-listener".to_string())
        .spawn(move || {
            info!("啟動 Windows 全域快捷鍵監聽執行緒 (Alt + Space)...");

            unsafe {
                let modifiers = MOD_ALT | MOD_NOREPEAT;
                let vk = VK_SPACE.0 as u32;

                // 註冊全域快捷鍵 ID: HOTKEY_ID_PREVIEW
                let reg_res = RegisterHotKey(HWND(0 as _), HOTKEY_ID_PREVIEW, modifiers, vk);
                if let Err(e) = reg_res {
                    error!(
                        "無法註冊全域快捷鍵 Alt + Space: {:?} (可能與其他軟體衝突)",
                        e
                    );
                } else {
                    info!("✅ 成功註冊全域快捷鍵: Alt + Space");
                }

                let mut msg = MSG::default();
                // Win32 Message Loop
                while running.load(Ordering::Relaxed) {
                    let ret = GetMessageW(&mut msg, HWND(0 as _), 0, 0);
                    if ret.0 <= 0 {
                        break;
                    }

                    if msg.message == WM_HOTKEY && msg.wParam == WPARAM(HOTKEY_ID_PREVIEW as usize) {
                        debug!("接收到全域快捷鍵 Alt + Space 事件！");
                        let _ = sender.send(HotkeyEvent::TriggerPreview);
                    }

                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // 解除註冊
                let _ = UnregisterHotKey(HWND(0 as _), HOTKEY_ID_PREVIEW);
                info!("全域快捷鍵已解除註冊");
            }
        })
        .expect("無法建立快捷鍵監聽執行緒")
}
