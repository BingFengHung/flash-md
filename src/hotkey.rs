use crate::explorer::{get_selected_file_from_explorer, show_and_focus_app_window};
use crossbeam_channel::Sender;
use egui::Context;
use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU, VK_SPACE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    TriggerPreviewWithFile(Option<PathBuf>),
}

static GLOBAL_HOTKEY_SENDER: Mutex<Option<Sender<HotkeyEvent>>> = Mutex::new(None);
static GLOBAL_CTX_HOLDER: Mutex<Option<Arc<Mutex<Option<Context>>>>> = Mutex::new(None);
static GLOBAL_HOOK: Mutex<HHOOK> = Mutex::new(HHOOK(0 as _));

/// 全域低階鍵盤掛鉤 (WH_KEYBOARD_LL) 回呼函式
/// 攔截 Alt + Space 並直接吞噬該按鍵事件 (Swallow Key Event)，防止 Windows 彈出系統視窗選單！
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let msg_type = w_param.0 as u32;
        let is_key_down = msg_type == WM_KEYDOWN || msg_type == WM_SYSKEYDOWN;
        let is_key_up = msg_type == WM_KEYUP || msg_type == WM_SYSKEYUP;

        if is_key_down || is_key_up {
            let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
            let vk = kbd.vkCode;

            if vk == VK_SPACE.0 as u32 {
                // 檢查 Alt 鍵是否處於按下狀態 (透過 GetAsyncKeyState 或 LLKHF_ALTDOWN 旗標)
                let alt_pressed = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0
                    || (kbd.flags.0 & 0x20) != 0;

                if alt_pressed {
                    if is_key_down {
                        debug!("⚡ 成功攔截 Alt + Space！");

                        // ⚠️ 關鍵修正：低階鍵盤掛鉤回呼 (WH_KEYBOARD_LL) 是在 GetMessageW 內部
                        // 被同步呼叫的，Windows 對此有嚴格的逾時限制（約 300ms）。
                        // 跨行程 COM 呼叫 (CoCreateInstance CLSCTX_LOCAL_SERVER 至 explorer.exe)
                        // 需要訊息幫浦 (message pump) 配合進行 COM 封送 (marshaling)，
                        // 但掛鉤回呼正處於訊息處理流程中，訊息幫浦被阻塞，
                        // 會導致 COM 呼叫互鎖逾時或靜默失敗回傳 None！
                        //
                        // 因此必須將所有 COM 操作搬至獨立執行緒，讓掛鉤回呼立即返回！
                        let sender_clone = GLOBAL_HOTKEY_SENDER.lock().ok().and_then(|g| g.clone());
                        let ctx_clone = GLOBAL_CTX_HOLDER.lock().ok().and_then(|g| g.clone());

                        std::thread::spawn(move || {
                            // 1. 在獨立執行緒中執行 COM 操作（擁有獨立的 COM 初始化，不受掛鉤訊息幫浦限制）
                            let selected_file = get_selected_file_from_explorer();

                            // 2. 透過 Win32 原生 ShowWindow(SW_SHOW) 強制喚醒 OS 視窗與 winit 事件迴圈
                            show_and_focus_app_window();

                            // 3. 發送帶有檔案路徑的預覽事件至主佇列
                            if let Some(sender) = sender_clone {
                                let _ = sender.send(HotkeyEvent::TriggerPreviewWithFile(selected_file));
                            }

                            // 4. 喚醒 egui 繪製迴圈
                            if let Some(ctx_holder) = ctx_clone {
                                if let Ok(guard) = ctx_holder.lock() {
                                    if let Some(ref ctx) = *guard {
                                        ctx.request_repaint();
                                    }
                                }
                            }
                        });
                    }

                    // 關鍵：傳回 1 (非零) 徹底吞噬此按鍵，防止 Windows 彈出還原/最大化系統選單！
                    return LRESULT(1);
                }
            }
        }
    }

    let hook = GLOBAL_HOOK.lock().ok().map(|g| *g).unwrap_or(HHOOK(0 as _));
    CallNextHookEx(hook, n_code, w_param, l_param)
}

/// 啟動全域鍵盤掛鉤監聽執行緒
pub fn start_hotkey_listener(
    sender: Sender<HotkeyEvent>,
    ctx_holder: Arc<Mutex<Option<Context>>>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("hotkey-hook-listener".to_string())
        .spawn(move || {
            info!("啟動 Windows Low-Level Keyboard Hook (WH_KEYBOARD_LL) 監聽執行緒...");

            if let Ok(mut g) = GLOBAL_HOTKEY_SENDER.lock() {
                *g = Some(sender);
            }
            if let Ok(mut g) = GLOBAL_CTX_HOLDER.lock() {
                *g = Some(ctx_holder);
            }

            unsafe {
                // 設定低階鍵盤掛鉤 (WH_KEYBOARD_LL)
                let hook = match SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(low_level_keyboard_proc),
                    HINSTANCE(0 as _),
                    0,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        error!("❌ 無法設定 WH_KEYBOARD_LL 鍵盤掛鉤: {:?}", e);
                        return;
                    }
                };

                if let Ok(mut g) = GLOBAL_HOOK.lock() {
                    *g = hook;
                }
                info!("✅ 成功啟用 WH_KEYBOARD_LL 全域鍵盤攔截器 (已攔截並吞噬 Alt+Space 系統選單)");

                let mut msg = MSG::default();
                // Win32 Message Loop 維持掛鉤運作
                while running.load(Ordering::Relaxed) {
                    let ret = GetMessageW(&mut msg, HWND(0 as _), 0, 0);
                    if ret.0 <= 0 {
                        break;
                    }

                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }

                // 移除掛鉤
                let _ = UnhookWindowsHookEx(hook);
                if let Ok(mut g) = GLOBAL_HOOK.lock() {
                    *g = HHOOK(0 as _);
                }
                info!("WH_KEYBOARD_LL 鍵盤掛鉤已安全解除");
            }
        })
        .expect("無法建立快捷鍵掛鉤監聽執行緒")
}
