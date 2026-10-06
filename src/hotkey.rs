use crate::explorer::{get_selected_file_from_explorer, wake_app_window};
use crossbeam_channel::{unbounded, Sender};
use egui::Context;
use log::{debug, error, info};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU, VK_SPACE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    TriggerPreviewWithFile {
        request_id: u64,
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy)]
struct PreviewRequest {
    id: u64,
    foreground: isize,
}

#[derive(Default)]
struct RequestSequence(AtomicU64);

impl RequestSequence {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }

    fn is_current(&self, id: u64) -> bool {
        self.0.load(Ordering::SeqCst) == id
    }
}

static REQUEST_SEQUENCE: RequestSequence = RequestSequence(AtomicU64::new(0));
static GLOBAL_REQUEST_SENDER: Mutex<Option<Sender<PreviewRequest>>> = Mutex::new(None);
static SPACE_PRESSED: AtomicBool = AtomicBool::new(false);
static GLOBAL_HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);

pub fn is_current_request(id: u64) -> bool {
    REQUEST_SEQUENCE.is_current(id)
}

fn resolve_preview_request(
    request: PreviewRequest,
    current: impl Fn(u64) -> bool,
    query: impl FnOnce(isize) -> Option<PathBuf>,
) -> Option<HotkeyEvent> {
    if !current(request.id) {
        return None;
    }
    let path = query(request.foreground);
    current(request.id).then_some(HotkeyEvent::TriggerPreviewWithFile {
        request_id: request.id,
        path,
    })
}

/// Keep COM work off the hook thread and always query the window that was
/// foreground when the key was pressed. A single worker coalesces rapid
/// presses, and both the worker and UI reject outdated results.
fn preview_worker(
    requests: crossbeam_channel::Receiver<PreviewRequest>,
    events: Sender<HotkeyEvent>,
    ctx_holder: Arc<Mutex<Option<Context>>>,
) {
    while let Ok(request) = requests.recv() {
        let request = requests.try_iter().last().unwrap_or(request);
        let Some(event) =
            resolve_preview_request(request, is_current_request, get_selected_file_from_explorer)
        else {
            continue;
        };
        if events.send(event).is_err() {
            break;
        }
        if is_current_request(request.id) {
            // Send first, then wake without taking focus. The UI focuses the
            // preview only after it has accepted and loaded the new document.
            wake_app_window();
            if let Ok(holder) = ctx_holder.lock() {
                if let Some(ctx) = holder.as_ref() {
                    ctx.request_repaint();
                }
            }
        }
    }
}

/// 攔截 Alt + Space，立即送出工作並吞噬按鍵，避免系統視窗選單。
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
            if kbd.vkCode == VK_SPACE.0 as u32 {
                if is_key_up && SPACE_PRESSED.swap(false, Ordering::Relaxed) {
                    return LRESULT(1);
                }
                if is_key_down && SPACE_PRESSED.load(Ordering::Relaxed) {
                    return LRESULT(1);
                }
                let alt_pressed = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0
                    || (kbd.flags.0 & 0x20) != 0;
                if alt_pressed {
                    if is_key_down && !SPACE_PRESSED.swap(true, Ordering::Relaxed) {
                        let request = PreviewRequest {
                            id: REQUEST_SEQUENCE.next(),
                            foreground: GetForegroundWindow().0 as isize,
                        };
                        debug!("⚡ Alt + Space 預覽請求 {}", request.id);
                        if let Ok(sender) = GLOBAL_REQUEST_SENDER.lock() {
                            if let Some(sender) = sender.as_ref() {
                                let _ = sender.send(request);
                            }
                        }
                    }
                    return LRESULT(1);
                }
            }
        }
    }
    let hook_val = GLOBAL_HOOK_HANDLE.load(Ordering::Relaxed);
    CallNextHookEx(HHOOK(hook_val as _), n_code, w_param, l_param)
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

            let (request_tx, request_rx) = unbounded();
            let worker = thread::Builder::new()
                .name("hotkey-preview-worker".to_string())
                .spawn(move || preview_worker(request_rx, sender, ctx_holder))
                .expect("無法建立預覽查詢執行緒");
            if let Ok(mut slot) = GLOBAL_REQUEST_SENDER.lock() {
                *slot = Some(request_tx);
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
                        if let Ok(mut slot) = GLOBAL_REQUEST_SENDER.lock() {
                            *slot = None;
                        }
                        let _ = worker.join();
                        return;
                    }
                };

                GLOBAL_HOOK_HANDLE.store(hook.0 as isize, Ordering::Relaxed);
                info!(
                    "✅ 成功啟用 WH_KEYBOARD_LL 全域鍵盤攔截器 (已攔截並吞噬 Alt+Space 系統選單)"
                );

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
                GLOBAL_HOOK_HANDLE.store(0, Ordering::Relaxed);
                info!("WH_KEYBOARD_LL 鍵盤掛鉤已安全解除");
            }
            if let Ok(mut slot) = GLOBAL_REQUEST_SENDER.lock() {
                *slot = None;
            }
            let _ = worker.join();
        })
        .expect("無法建立快捷鍵掛鉤監聽執行緒")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_results_cannot_override_a_newer_preview_request() {
        let requests = RequestSequence::default();
        let request = PreviewRequest {
            id: requests.next(),
            foreground: 100,
        };
        let result = resolve_preview_request(
            request,
            |id| requests.is_current(id),
            |_| {
                // A second key press arrives while the first COM query is running.
                requests.next();
                Some(PathBuf::from("old.md"))
            },
        );
        assert_eq!(result, None);
        assert!(requests.is_current(request.id + 1));
    }

    #[test]
    fn selection_uses_the_captured_foreground_and_skips_obsolete_queries() {
        let requests = RequestSequence::default();
        let old = requests.next();
        let latest = requests.next();
        let stale = resolve_preview_request(
            PreviewRequest {
                id: old,
                foreground: 100,
            },
            |id| requests.is_current(id),
            |_| panic!("A stale request must not issue a COM query"),
        );
        assert_eq!(stale, None);
        let selected = resolve_preview_request(
            PreviewRequest {
                id: latest,
                foreground: 200,
            },
            |id| requests.is_current(id),
            |foreground| {
                assert_eq!(foreground, 200);
                Some(PathBuf::from("new.md"))
            },
        );
        assert_eq!(
            selected,
            Some(HotkeyEvent::TriggerPreviewWithFile {
                request_id: latest,
                path: Some(PathBuf::from("new.md")),
            })
        );
    }
}
