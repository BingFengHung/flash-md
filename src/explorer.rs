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
    BringWindowToTop, FindWindowW, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetGUIThreadInfo, GetParent, GetShellWindow, GetWindowThreadProcessId, IsWindowVisible,
    SetForegroundWindow, SetWindowPos, ShowWindow, GA_ROOT, GUITHREADINFO,
    HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_RESTORE,
    SW_SHOW,
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
    if foreground_hwnd.0 == 0 as _ {
        return None;
    }

    let root_foreground = GetAncestor(foreground_hwnd, GA_ROOT);
    let shell_hwnd = GetShellWindow();

    let mut class_name = [0u16; 256];
    let class_len = GetClassNameW(foreground_hwnd, &mut class_name);
    let class_str = String::from_utf16_lossy(&class_name[..class_len as usize]);

    let mut root_class_name = [0u16; 256];
    let root_class_len = GetClassNameW(root_foreground, &mut root_class_name);
    let root_class_str = String::from_utf16_lossy(&root_class_name[..root_class_len as usize]);

    debug!(
        "🔍 前景視窗: class={}, root_class={}, HWND={:?}, root={:?}",
        class_str, root_class_str, foreground_hwnd, root_foreground
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

    // 1. 如果前景為 Windows 桌面 (Progman 或 WorkerW 或 ShellWindow)
    let is_desktop = class_str == "Progman"
        || class_str == "WorkerW"
        || root_class_str == "Progman"
        || root_class_str == "WorkerW"
        || foreground_hwnd == shell_hwnd
        || root_foreground == shell_hwnd;

    if is_desktop {
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
        // 桌面未選取任何檔案，直接返回 None，嚴禁跨視窗讀取背景資料夾
        return None;
    }

    // 2. 判斷前景是否為檔案總管 (CabinetWClass / ExploreWClass)
    let is_explorer_foreground = class_str == "CabinetWClass"
        || class_str == "ExploreWClass"
        || root_class_str == "CabinetWClass"
        || root_class_str == "ExploreWClass"
        || class_str == "ShellTabWindowClass"
        || root_class_str == "ShellTabWindowClass";

    // 若前景完全不是檔案總管也不是桌面，直接返回 None，絕不讀取背景歷史資料夾
    if !is_explorer_foreground {
        debug!("前景非檔案總管或桌面，跳過背景檔案查詢");
        return None;
    }

    let count = match shell_windows.Count() {
        Ok(c) => c,
        Err(_) => return None,
    };

    let mut fg_pid = 0u32;
    let fg_thread = GetWindowThreadProcessId(foreground_hwnd, Some(&mut fg_pid));
    let mut gui_info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    let focus_hwnd = if fg_thread != 0 && GetGUIThreadInfo(fg_thread, &mut gui_info).is_ok() {
        gui_info.hwndFocus
    } else {
        HWND(0 as _)
    };

    let mut candidates: Vec<(i32, IDispatch)> = Vec::new();

    for i in 0..count {
        let variant_index = VARIANT::from(i);
        let item_disp = match shell_windows.Item(&variant_index) {
            Ok(disp) => disp,
            Err(_) => continue,
        };

        if let Ok(browser) = item_disp.cast::<windows::Win32::UI::Shell::IWebBrowserApp>() {
            let is_browser_visible = browser.Visible().map(|v| v.0 != 0).unwrap_or(true);

            if let Ok(hwnd_val) = browser.HWND() {
                let win_hwnd = HWND(hwnd_val.0 as _);
                let root_win = GetAncestor(win_hwnd, GA_ROOT);
                let is_os_visible = IsWindowVisible(win_hwnd).as_bool();

                // 核心防護：嚴格限定只查詢當前前景檔案總管視窗內部的分頁！
                // 徹底隔離背景開啟過的其他資料夾視窗，絕不互相干擾！
                if root_win != root_foreground && win_hwnd != foreground_hwnd {
                    continue;
                }

                let mut score = 10;

                // 1. 最高權重：精確符合當前焦點控制項 (焦點 tab)
                if focus_hwnd.0 != 0 as _ && (win_hwnd == focus_hwnd || is_child_or_same(focus_hwnd, win_hwnd)) {
                    score += 300;
                }
                // 2. 前景視窗或其子父視窗
                if win_hwnd == foreground_hwnd || is_child_or_same(foreground_hwnd, win_hwnd) || is_child_or_same(win_hwnd, foreground_hwnd) {
                    score += 200;
                }
                // 3. 相同 Root 視窗 (同一個檔案總管視窗內部的分頁)
                if root_win == root_foreground && root_foreground.0 != 0 as _ {
                    score += 100;
                }
                // 4. 可見性加分 (Windows 11 中作用中分頁可見，背景分頁隱藏)
                if is_browser_visible {
                    score += 50;
                }
                if is_os_visible {
                    score += 30;
                }

                candidates.push((score, item_disp));
            }
        }
    }

    // 依權重降冪排序，最符合前景焦點的視窗排在最前面
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    for (score, item_disp) in candidates {
        debug!("嘗試查詢前景檔案總管候選分頁 (評分: {})...", score);
        if let Some(path) = extract_selected_from_folder_view(&item_disp) {
            return Some(path);
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

    // 1. 優先嘗試 SelectedItems() (多選或單擊選取)
    if let Ok(selected_items) = folder_view.SelectedItems() {
        if let Ok(count) = selected_items.Count() {
            if count > 0 {
                // 優先尋找已選取之「實體檔案」(非目錄)
                for i in 0..count {
                    let item_variant = VARIANT::from(i);
                    if let Ok(item) = selected_items.Item(&item_variant) {
                        if let Ok(path_bstr) = item.Path() {
                            let raw_path = path_bstr.to_string();
                            if !raw_path.is_empty() {
                                let path = normalize_explorer_path(&raw_path);
                                if path.is_file() {
                                    info!("✅ 成功自 SelectedItems 取得檔案: {:?} (原始: {})", path, raw_path);
                                    return Some(path);
                                }
                            }
                        }
                    }
                }

                // 若選取的包含資料夾或虛擬項目，取第 0 個
                let item_variant = VARIANT::from(0i32);
                if let Ok(item) = selected_items.Item(&item_variant) {
                    if let Ok(path_bstr) = item.Path() {
                        let raw_path = path_bstr.to_string();
                        if !raw_path.is_empty() {
                            let path = normalize_explorer_path(&raw_path);
                            info!("✅ 成功自 SelectedItems 取得項目: {:?} (原始: {})", path, raw_path);
                            return Some(path);
                        }
                    }
                }
            }
        }
    }

    // 2. 備用嘗試 FocusedItem() (當鍵盤導航或單擊時，SelectedItems 可能為空而 FocusedItem 存在)
    if let Ok(focused_item) = folder_view.FocusedItem() {
        if let Ok(path_bstr) = focused_item.Path() {
            let raw_path = path_bstr.to_string();
            if !raw_path.is_empty() {
                let path = normalize_explorer_path(&raw_path);
                if path.is_file() {
                    info!("✅ 成功自 FocusedItem 取得檔案: {:?} (原始: {})", path, raw_path);
                    return Some(path);
                }
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
