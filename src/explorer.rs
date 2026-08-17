use log::{debug, info, warn};
use std::path::PathBuf;
use windows::core::{w, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IDispatch, CLSCTX_LOCAL_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Shell::{
    IShellFolderViewDual, IShellWindows, ShellWindows, SWC_DESKTOP,
    SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetAncestor, GetClassNameW, GetForegroundWindow, GetParent,
    GetShellWindow, GetWindowThreadProcessId, SetForegroundWindow, SetWindowPos, ShowWindow,
    GA_ROOT, GA_ROOTOWNER, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SW_HIDE, SW_RESTORE, SW_SHOW,
};
use windows_core::Interface;

static APP_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[allow(dead_code)]
pub fn set_app_hwnd(hwnd: HWND) {
    APP_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
}

pub fn get_app_hwnd() -> Option<HWND> {
    let val = APP_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if val != 0 {
        Some(HWND(val as *mut std::ffi::c_void))
    } else {
        // 嘗試以視窗標題尋找 flash-md HWND
        unsafe {
            if let Ok(hwnd) = FindWindowW(None, w!("flash-md - 快捷鍵 Markdown 預覽")) {
                if hwnd.0 != 0 as _ {
                    APP_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
                    return Some(hwnd);
                }
            }
            None
        }
    }
}

/// 透過 Win32 原生 API 強制將 flash-md 視窗跳至最上層並取得焦點 (破除 Windows 前景鎖定限制)
pub fn show_and_focus_app_window() {
    if let Some(hwnd) = get_app_hwnd() {
        unsafe {
            let fg_hwnd = GetForegroundWindow();
            let fg_thread = if fg_hwnd.0 != 0 as _ {
                GetWindowThreadProcessId(fg_hwnd, None)
            } else {
                0
            };
            let cur_thread = GetCurrentThreadId();

            // 1. 綁定執行緒輸入權限 (獲得 Windows 前景焦點切換許可)
            if fg_thread != 0 && fg_thread != cur_thread {
                let _ = AttachThreadInput(cur_thread, fg_thread, true);
            }

            // 2. 顯示並還原視窗
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = BringWindowToTop(hwnd);

            // 3. 瞬間切換為 TOPMOST 彈至最前端，再還原為一般層級
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );

            // 4. 設定為前景焦點視窗
            let _ = SetForegroundWindow(hwnd);

            // 5. 解除輸入綁定
            if fg_thread != 0 && fg_thread != cur_thread {
                let _ = AttachThreadInput(cur_thread, fg_thread, false);
            }
        }
    }
}

/// 透過 Win32 原生 API 隱藏 flash-md 視窗
pub fn hide_app_window() {
    if let Some(hwnd) = get_app_hwnd() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// 取得目前前景視窗（Windows 檔案總管或桌面）中所選取的檔案路徑
pub fn get_selected_file_from_explorer() -> Option<PathBuf> {
    unsafe {
        // 初始化 COM 元件 (STA 執行緒模式)
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = get_selected_file_internal();
        result
    }
}

unsafe fn get_selected_file_internal() -> Option<PathBuf> {
    let foreground_hwnd = GetForegroundWindow();
    let root_foreground = if foreground_hwnd.0 != 0 as _ {
        GetAncestor(foreground_hwnd, GA_ROOT)
    } else {
        HWND(0 as _)
    };
    let root_owner = if foreground_hwnd.0 != 0 as _ {
        GetAncestor(foreground_hwnd, GA_ROOTOWNER)
    } else {
        HWND(0 as _)
    };

    let mut class_name = [0u16; 256];
    let class_len = if foreground_hwnd.0 != 0 as _ {
        GetClassNameW(foreground_hwnd, &mut class_name)
    } else {
        0
    };
    let class_str = String::from_utf16_lossy(&class_name[..class_len as usize]);
    debug!(
        "前景視窗類別: {}, HWND: {:?}, Root: {:?}, RootOwner: {:?}",
        class_str, foreground_hwnd, root_foreground, root_owner
    );

    // 建立 ShellWindows 實例
    let shell_windows: IShellWindows = match CoCreateInstance(
        &ShellWindows,
        None,
        CLSCTX_LOCAL_SERVER,
    ) {
        Ok(sw) => sw,
        Err(e) => {
            warn!("無法建立 IShellWindows: {:?}", e);
            return None;
        }
    };

    let count = match shell_windows.Count() {
        Ok(c) => c,
        Err(e) => {
            warn!("無法讀取 ShellWindows 數量: {:?}", e);
            return None;
        }
    };

    debug!("已開啟的 Shell 視窗數量: {}", count);

    // 1. 如果前景為 Windows 桌面 (Progman 或 WorkerW)
    let shell_hwnd = GetShellWindow();
    if class_str == "Progman"
        || class_str == "WorkerW"
        || foreground_hwnd == shell_hwnd
        || root_foreground == shell_hwnd
    {
        debug!("前景為 Windows 桌面，嘗試查詢桌面選取項目...");
        let pvar_loc = VARIANT::from(0i32); // CSIDL_DESKTOP
        let mut phwnd = 0i32;
        if let Ok(desktop_disp) = shell_windows.FindWindowSW(
            &pvar_loc,
            &VARIANT::default(),
            SWC_DESKTOP,
            &mut phwnd,
            SWFO_NEEDDISPATCH,
        ) {
            if let Some(path) = extract_selected_from_folder_view(&desktop_disp) {
                return Some(path);
            }
        }
    }

    // 2. 優先搜尋符合前景 HWND / 根視窗 HWND / 擁有者 HWND 的視窗 (支援 Windows 10/11 多分頁檔案總管)
    for i in 0..count {
        let variant_index = VARIANT::from(i);
        let item_disp = match shell_windows.Item(&variant_index) {
            Ok(disp) => disp,
            Err(_) => continue,
        };

        if let Some(path) = extract_selected_if_matching_hwnd(
            &item_disp,
            foreground_hwnd,
            root_foreground,
            root_owner,
        ) {
            return Some(path);
        }
    }

    // 3. Fallback: 尋找所有開啟的檔案總管視窗中第一個有選取或焦點的有效項目
    for i in 0..count {
        let variant_index = VARIANT::from(i);
        if let Ok(item_disp) = shell_windows.Item(&variant_index) {
            if let Some(path) = extract_selected_from_folder_view(&item_disp) {
                return Some(path);
            }
        }
    }

    None
}

unsafe fn extract_selected_if_matching_hwnd(
    item_disp: &IDispatch,
    target_hwnd: HWND,
    root_target: HWND,
    owner_target: HWND,
) -> Option<PathBuf> {
    if let Ok(browser) = item_disp.cast::<windows::Win32::UI::Shell::IWebBrowserApp>() {
        if let Ok(hwnd_val) = browser.HWND() {
            let win_hwnd = HWND(hwnd_val.0 as _);
            let root_win = GetAncestor(win_hwnd, GA_ROOT);
            let owner_win = GetAncestor(win_hwnd, GA_ROOTOWNER);

            let is_matched = win_hwnd == target_hwnd
                || root_win == root_target
                || owner_win == owner_target
                || win_hwnd == root_target
                || win_hwnd == owner_target
                || is_child_or_same(target_hwnd, win_hwnd)
                || is_child_or_same(win_hwnd, target_hwnd);

            if is_matched {
                return extract_selected_from_folder_view(item_disp);
            }
        }
    }
    None
}

unsafe fn is_child_or_same(child: HWND, parent: HWND) -> bool {
    if child == parent {
        return true;
    }
    let mut curr = child;
    while curr.0 != 0 as _ {
        if curr == parent {
            return true;
        }
        match GetParent(curr) {
            Ok(p) if p.0 != 0 as _ => curr = p,
            _ => break,
        }
    }
    false
}

unsafe fn extract_selected_from_folder_view(disp: &IDispatch) -> Option<PathBuf> {
    let web_browser: windows::Win32::UI::Shell::IWebBrowserApp = match disp.cast() {
        Ok(wb) => wb,
        Err(_) => return None,
    };

    let doc_disp = match web_browser.Document() {
        Ok(doc) => doc,
        Err(_) => return None,
    };

    let folder_view: IShellFolderViewDual = match doc_disp.cast() {
        Ok(fv) => fv,
        Err(_) => return None,
    };

    // 1. 優先嘗試 SelectedItems()
    if let Ok(selected_items) = folder_view.SelectedItems() {
        if let Ok(count) = selected_items.Count() {
            if count > 0 {
                let item_variant = VARIANT::from(0i32);
                if let Ok(item) = selected_items.Item(&item_variant) {
                    if let Ok(path_bstr) = item.Path() {
                        let raw_path = path_bstr.to_string();
                        if !raw_path.is_empty() {
                            let path = normalize_explorer_path(&raw_path);
                            info!("✅ 成功自 SelectedItems 取得檔案: {:?} (原始: {})", path, raw_path);
                            return Some(path);
                        }
                    }
                }
            }
        }
    }

    // 2. 備用嘗試 FocusedItem() (因點選時檔案必為 FocusedItem)
    if let Ok(focused_item) = folder_view.FocusedItem() {
        if let Ok(path_bstr) = focused_item.Path() {
            let raw_path = path_bstr.to_string();
            if !raw_path.is_empty() {
                let path = normalize_explorer_path(&raw_path);
                info!("✅ 成功自 FocusedItem 取得檔案: {:?} (原始: {})", path, raw_path);
                return Some(path);
            }
        }
    }

    None
}

/// 正規化檔案總管傳回的路徑（支援 null 結尾清除、file:/// 去除、URL 百分比解碼如 %20、路徑引號去除）
pub fn normalize_explorer_path(raw: &str) -> PathBuf {
    let mut s = raw.trim().trim_matches('\0').trim();

    // 去除前後引號
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s = &s[1..s.len() - 1];
    }

    // 大小寫不敏感去除 file:/// 或 file:// 前綴
    let lower = s.to_lowercase();
    if lower.starts_with("file:///") {
        s = &s[8..];
    } else if lower.starts_with("file://") {
        s = &s[7..];
    } else if lower.starts_with("file:/") {
        s = &s[6..];
    }

    // URL 百分比解碼 (%20 -> 空格, %28 -> (, %29 -> ), 等)
    let decoded = url_decode(s);

    // 優先以原生反斜線路徑驗證檔案是否存在
    let p_decoded_native = PathBuf::from(decoded.replace('/', "\\"));
    if p_decoded_native.exists() {
        return p_decoded_native;
    }

    let p_decoded = PathBuf::from(&decoded);
    if p_decoded.exists() {
        return p_decoded;
    }

    let p_raw_native = PathBuf::from(s.replace('/', "\\"));
    if p_raw_native.exists() {
        return p_raw_native;
    }

    let p_raw = PathBuf::from(s);
    if p_raw.exists() {
        return p_raw;
    }

    p_decoded_native
}

fn decode_hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn url_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                if let (Some(d1), Some(d2)) = (decode_hex_digit(h1), decode_hex_digit(h2)) {
                    bytes.push((d1 << 4) | d2);
                    continue;
                } else {
                    bytes.push(b'%');
                    bytes.push(h1);
                    bytes.push(h2);
                    continue;
                }
            } else {
                bytes.push(b'%');
                if let Some(h1) = h1 {
                    bytes.push(h1);
                }
                continue;
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).to_string()
}
