use log::{debug, warn};
use std::path::PathBuf;
use windows::core::VARIANT;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch, CLSCTX_LOCAL_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FolderItems, IShellFolderViewDual, IShellWindows, ShellWindows, SWC_DESKTOP,
    SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetParent, GetShellWindow,
};
use windows_core::Interface;

/// 取得目前前景視窗（Windows 檔案總管或桌面）中所選取的檔案路徑
pub fn get_selected_file_from_explorer() -> Option<PathBuf> {
    unsafe {
        // 初始化 COM 元件 (STA 執行緒模式)
        let com_init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = get_selected_file_internal();
        if com_init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

unsafe fn get_selected_file_internal() -> Option<PathBuf> {
    let foreground_hwnd = GetForegroundWindow();
    if foreground_hwnd.0 == 0 as _ {
        debug!("未偵測到前景視窗");
        return None;
    }

    let mut class_name = [0u16; 256];
    let class_len = GetClassNameW(foreground_hwnd, &mut class_name);
    if class_len == 0 {
        return None;
    }
    let class_str = String::from_utf16_lossy(&class_name[..class_len as usize]);
    debug!("前景視窗類別: {}, HWND: {:?}", class_str, foreground_hwnd);

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

    // 1. 搜尋符合前景 HWND 的視窗 (支援 Windows 10/11 多分頁檔案總管)
    for i in 0..count {
        let variant_index = VARIANT::from(i);
        let item_disp = match shell_windows.Item(&variant_index) {
            Ok(disp) => disp,
            Err(_) => continue,
        };

        if let Some(path) = extract_selected_path_from_disp(&item_disp, foreground_hwnd) {
            return Some(path);
        }
    }

    // 2. 如果前景是桌面 (Progman 或 WorkerW)
    let shell_hwnd = GetShellWindow();
    if class_str == "Progman" || class_str == "WorkerW" || foreground_hwnd == shell_hwnd {
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

    // 3. Fallback: 嘗試從第一個有效選取項目取得
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

unsafe fn extract_selected_path_from_disp(
    item_disp: &IDispatch,
    target_hwnd: HWND,
) -> Option<PathBuf> {
    // 取得 IWebBrowserApp HWND 以比對前景視窗
    if let Ok(browser) = item_disp.cast::<windows::Win32::UI::Shell::IWebBrowserApp>() {
        if let Ok(hwnd_val) = browser.HWND() {
            let win_hwnd = HWND(hwnd_val as *mut _);
            if win_hwnd == target_hwnd || is_child_or_same(target_hwnd, win_hwnd) {
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
    // 透過 IWebBrowserApp / IShellBrowser 取得 Document (IShellFolderViewDual)
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

    let selected_items: FolderItems = match folder_view.SelectedItems() {
        Ok(items) => items,
        Err(_) => return None,
    };

    let count = match selected_items.Count() {
        Ok(c) => c,
        Err(_) => return None,
    };

    if count == 0 {
        return None;
    }

    // 取得第 0 個選取的項目
    let item_variant = VARIANT::from(0i32);
    let item = match selected_items.Item(&item_variant) {
        Ok(it) => it,
        Err(_) => return None,
    };

    let path_bstr = match item.Path() {
        Ok(p) => p,
        Err(_) => return None,
    };

    let path_str = path_bstr.to_string();
    if path_str.is_empty() {
        return None;
    }

    let path = PathBuf::from(path_str);
    debug!("偵測到選取檔案: {:?}", path);
    Some(path)
}
