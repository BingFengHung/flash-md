# AGENTS.md - AI Agent & 開發者通用指南 (General Guidelines)

本文件定義通用型 AI Agent 輔助開發、版本控管、Git 提交規範以及 GitHub Actions CI/CD 工作流之標準原則。適用於各式跨平台軟體與 CLI 工具專案。

---

## 🎯 核心原則與開發規範 (Core Guidelines & Rules)

### 1. 🚫 地端零編譯原則 (Zero Local Heavy Compilation)
- **絕對不要在地端執行耗時編譯或產出發布用二進制檔**（例如 `cargo build`、大型原生編譯等）。
- 地端僅進行程式碼編寫、靜態分析、文件修訂與 Git 版本控制。
- 所有發布用的執行檔 (Executable, Binaries) 與產物編譯，**必須完全交由 GitHub Actions 雲端 CI/CD 矩陣執行**。

### 2. 🔢 語意化版本號規範 (Semantic Versioning & Release Tags)
- **每次更新或修改程式碼功能時，必須同時升級版本號**。
- 升級步驟：
  1. 更新專案版本檔中的 `version` 欄位（例如 `Cargo.toml` / `package.json` 的 `v0.x.x` -> `v0.y.y`）。
  2. 完成 Git 提交與推送主分支 (`main` 或 `master`)。
  3. 建立相對應的版本 Tag 並推送至 GitHub（例如 `git tag vX.Y.Z` -> `git push origin vX.Y.Z`）。

### 3. 💬 Commit Message 規範 (Commit Message Standard)
- **所有 Git Commit Message 必須使用繁體中文撰寫**。
- 訊息格式應簡潔明確，說明異動動機與變更內容（例如：`新增自動更新功能 (update 指令)、升級版本至 v0.2.0`）。

### 4. 📚 雙語文件維護 (Bilingual Documentation)
- 保持專案說明文件同步更新：
  - `README.md`（英文版）
  - `README.zh-TW.md`（繁體中文版）
- 兩份文件頂部需互相提供語系切換連結。

---

## 🛠️ Windows API 與 Rust 開發經驗庫 (Technical Gotchas & Best Practices)

為避免重複發生編譯、借用檢查與型別錯誤，於此專案（包含 Windows Shell COM API、egui 與背景服務）開發時請務必遵循以下規範：

### 1. Windows Crate (0.58+) COM 介面與型別規範
- **`IDispatch` 模組路徑**：
  - 正確：`windows::Win32::System::Com::IDispatch`
  - 錯誤：`windows::Win32::System::Ole::IDispatch`（此路徑不存在）。
- **COM 介面 `.cast::<T>()` 方法**：
  - 必須於檔案頂部匯入 `use windows_core::Interface;`，否則在 `IDispatch` 或 COM 物件上呼叫 `.cast()` 會出現 `no method named cast found`。
- **CoCreateInstance 實例結構名稱**：
  - 在 `windows 0.58+` 中，傳入 coclass 實例結構體而非舊版常數：`CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)`。
- **`SHANDLE_PTR` 轉型為 `HWND`**：
  - `SHANDLE_PTR` 是一個 tuple struct（`pub struct SHANDLE_PTR(pub isize)`），**嚴禁**使用 `hwnd as *mut _` 直轉。
  - 正確寫法：`HWND(hwnd_val.0 as _)`。
- **Win32 API 傳回值型別注意**：
  - `GetParent(hwnd)` 與 `FindWindowW(None, w!("..."))` 在 `windows` crate 中傳回 `Result<HWND, windows::core::Error>`，需先經 `if let Ok(hwnd) = FindWindowW(...)` 或模式匹配處理，不可直接當作純 `HWND` 操作。
    ```rust
    match GetParent(curr) {
        Ok(p) if p.0 != 0 as _ => curr = p,
        _ => break,
    }
    ```
- **COM 方法列舉型別參數**：
  - 例如 `FindWindowSW` 中的 `SWC_DESKTOP` 與 `SWFO_NEEDDISPATCH` 已是型別安全的列舉常數，直接作為引數傳遞即可，切勿強制轉型為 `i32`。
- **Win32 訊息迴圈 `must_use` 處理**：
  - `TranslateMessage(&msg)` 與 `DispatchMessageW(&msg)` 傳回 `BOOL`，必須以 `let _ = TranslateMessage(&msg);` 明確接收，避免 `#[warn(unused_must_use)]` 警告。
- **`PWSTR::to_string()` 傳回值型別注意**：
  - `windows_core::PWSTR`（例如 `IShellItem::GetDisplayName` 傳回之物件）呼叫 `.to_string()` 傳回的是 **`Result<String, FromUtf16Error>`** 而非純字串。必須以 `if let Ok(raw_path) = display_name.to_string()` 解包處理。

### 2. 全域快捷鍵攔截與 Windows 系統選單 (SC_KEYMENU) 衝突解決
- **`Alt + Space` 系統預設選單衝突**：
  - `Alt + Space` 在 Windows 中是歷史悠久的視窗控制選單快捷鍵（`SC_KEYMENU`：還原、移動、大小、最小化、最大化、關閉）。
  - 標準 `RegisterHotKey` 無法在所有視窗上吃掉此系統選單，導致按下快捷鍵時跳出選單而無法順暢預覽。
  - **解決方案**：採用 Windows 低階鍵盤掛鉤 **`SetWindowsHookExW(WH_KEYBOARD_LL, ...)`**，在驅動層攔截 `VK_SPACE + Alt`，並回傳 **`LRESULT(1)`** 徹底吞噬按鍵事件，防止 Windows 彈出系統選單。

### 3. Windows COM 檔案總管路徑正規化 (URL 百分比解碼、Null Byte 與 file:/// 去除)
- **COM 回傳 URI 編碼或 Null Byte 結尾引發 `os error 3 (系統找不到指定的路徑)`**：
  - Windows COM BSTR 傳回的路徑常帶有不可見的結尾 Null Byte (`\0`)，或包含空格時回傳 `file:///C:/.../%20` 等 URL 編碼。
  - 直接以 `std::fs::read_to_string` 或 `path.exists()` 檢驗會直接失敗。
  - **解決方案**：
    1. 強制清除 Null 字元：`raw.trim().trim_matches('\0').trim()`。
    2. 大小寫不敏感去除 `file:///`、`file://` 前綴。
    3. 自訂純數字/位元運算 `url_decode` 還原 `%20` 等特殊字元。
    4. 將路徑斜線一律轉為 Windows 原生反斜線 `\` 比對是否存在。

### 4. 檔案總管多視窗與多分頁焦點競爭 (Strict Foreground Scope Isolation) 與舊檔案殘留防護
- **多個檔案總管視窗同時開啟時誤讀背景視窗問題**：
  - Windows COM 的 `IShellWindows` 列表是依「視窗建立時間」登記，歷史背景視窗會排在當前視窗之前。若背景資料夾先前點選過檔案，全域遍歷時會誤讀取背景視窗殘留的舊檔案。
  - **解決方案**：
    1. **前景視窗嚴格隔離 (Strict Foreground Scope)**：
       - 若前景為檔案總管 (`CabinetWClass` / `ShellTabWindowClass`)，**嚴格跳過所有不屬於該前景 Root HWND 的背景視窗** (`if root_win != root_foreground && win_hwnd != foreground_hwnd { continue; }`)。
       - 若前景為桌面，僅查詢桌面項目；若前景為其他非檔案總管應用程式（如瀏覽器），直接回傳 `None`，嚴禁回退搜尋背景歷史視窗！
    2. **雙軌精確選取 (`SelectedItems` + `FocusedItem` 備援)**：
       - 優先讀取 `SelectedItems()` 中已選取的實體檔案；
       - 若在單擊選取或方向鍵導航下 `SelectedItems` 為空，則透過 `FocusedItem()` 取得當前聚焦檔案（僅限前景視窗內），並嚴格檢驗 `path.is_file()`。
    3. **空狀態重置防護**：
       - 若在空白處點擊或無任何檔案被選取 (`None`)，呼叫 `handle_hotkey_preview` 時必須將 `current_file`、`content` 與圖片狀態完全清空，顯示純淨空狀態卡片，嚴禁殘留前一次開啟的檔案！

### 5. 檔案讀取優先級與 ZIP 記憶體即時解壓預覽
- **資料夾名稱含 `.zip` 造成誤判問題**：
  - 若解壓縮後的實體資料夾名稱帶有 `.zip`（例如 `Downloads\flash-md-windows-x86_64.zip\README.md`），若先做字串匹配會誤判為未解壓檔案。
  - **解決方案**：
    1. `load_file` **永遠第一優先直接嘗試 `fs::read_to_string(path)` 讀取實體檔案**。
    2. 僅在直接讀取失敗且路徑位於 `.zip` 內部時，才啟動 .NET `ZipFile` 記憶體即時解壓引擎進行穿透預覽。

### 6. egui 與 GUI 渲染規範 (CJK 字型與背景常駐喚醒)
- **Windows CJK 中文字型與 Emoji 載入**：
  - `egui` 預設僅內建 ASCII/拉丁字型，若不載入系統 CJK 字型，所有中文、日韓文及 Emoji 都會顯示為方塊 `□` (Tofu)。
  - 必須在 `egui::Context` 初始化時，自 `C:\Windows\Fonts\msjh.ttc`（微軟正黑體）與 `seguiemj.ttf`（Segoe UI Emoji）載入字型數據並插入至 `FontDefinitions` 的 `Proportional` 與 `Monospace` 家族首位。
- **eframe 隱藏視窗「凍結」與 Win32 HWND 強制喚醒機制**：
  - `winit` / `eframe` 在 Windows 下若視窗處於 `Visible(false)` 隱藏狀態，會抑制重繪事件（即使背景執行緒呼叫 `ctx.request_repaint()` 也會被忽略），導致常駐時快捷鍵與系統匣點擊無響應。
  - **解決方案**：
    1. 背景執行緒（快捷鍵、系統匣）收到事件時，必須直接透過 Win32 原生 API `ShowWindow(hwnd, SW_SHOW | SW_RESTORE)` 與 `SetForegroundWindow(hwnd)` 喚醒視窗，從 OS 層強制觸發 Windows 訊息。
    2. 在 `App::update` 中設置 `ctx.request_repaint_after(Duration::from_millis(100))` 保持背景 channel 敏捷輪詢。

### 7. Rust 所有權與借用檢查規範 (Borrow Checker Gotchas)
- **`move` 閉包捕獲 `self` 欄位導致 E0382 所有權轉移 (Move of `self`)**：
  - 在宣告 `let mut layouter = move |ui, string, _wrap_width| { ... }` 閉包時，若在閉包內直接引用 `self.font_scale`，Rust 的 `move` 語意會將整個 `self` 搬移進閉包中，造成閉包宣告之後的程式碼（如 `self.content.clone()`）觸發 `error[E0382]: borrow of moved value: self`。
  - **解決方式**：必須在 `move` 閉包宣告前，先將所有需要存取的 `self` 欄位以局部變數拷貝或克隆（如 `let font_scale = self.font_scale; let font_id_for_layouter = font_id.clone();`），確保閉包僅捕獲純局部變數，絕不搬移 `self`。
- **閉包內部修改 `self` 導致 E0500 借用衝突**：
  - 在 `if let Some(ref val) = self.field` 的外層借用範圍內，傳遞 `|ui|` 閉包並在閉包內部修改 `self` 會觸發 E0500。
  - **解決方式**：先將需要的欄位 `clone()` 或提取出局部變數，在閉包中僅記錄操作旗標（如 `let mut do_action = false;`），待閉包結束後再統一修改 `self`。
- **切片借用與變數所有權移動 E0505 衝突**：
  - 當 `let sub = x.trim_start_matches(...)` 借用了 `x`（傳回 `&str` 參照），隨後又在同作用域嘗試將 `x` 的所有權 move（例如建構結構體 `Struct { x, sub }`）時觸發。
  - **解決方式**：對借用的切片立即轉為獨立擁有之字串 `to_string()`，避免同時持有參照又搬移底層所有權。
- **暫時性值生命週期借用 E0716 錯誤**：
  - 在敘述式中建立暫時陣列切片如 `std::str::from_utf8(&[h1, h2])`，陣列生命週期會在該敘述式結束時被釋放，若外部變數試圖保留其參照會引發 E0716。
  - **解決方式**：使用獨立 `let` 綁定延長生命週期，或使用純數字/位元運算避免建構暫時參照。
- **未使用的公用輔助函式與結構體欄位 (`dead_code` 警告)**：
  - 跨模組公用函式或結構體欄位（如 `set_app_hwnd`、`ReleaseInfo`），若暫未在主流程直接調用，需加上 `#[allow(dead_code)]`，避免觸發編譯警告。

### 8. Windows GUI 子系統 (`windows_subsystem = "windows"`) 與命令列終端機輸出 (`AttachConsole`)
- **CLI 指令輸出被 Windows 吞噬問題**：
  - 當程式為了消除啟動時黑色視窗而宣告 `#![windows_subsystem = "windows"]` 時，Windows OS 會預設分離 (Detach) 標準輸出入 (`stdout` / `stderr`)。
  - 這會導致在 PowerShell 或 cmd 中執行 `flash-md --version` 或 `flash-md --update` 時，`println!` 內容無法顯示於終端機中。
  - **解決方案**：在 `main()` 進入點開頭呼叫 Win32 API **`AttachConsole(ATTACH_PARENT_PROCESS)`**。若程式是從終端機呼叫，將自動接回父行程終端機輸出；若為桌面雙擊開啟，則靜默失敗維持 0 黑框！

### 9. egui::Image (0.29+) 縮放與 ViewMode 模式窮舉規範
- **`egui::Image` 縮放 API**：
  - `egui::Image` 在 0.29+ 中無 `.scale(f32)` 方法，正確的比例縮放方法為 **`.fit_to_original_size(scale: f32)`** 或 `.max_size(Vec2)`。
- **列舉型別 `ViewMode` 窮舉性檢驗**：
  - 當新增 `ViewMode::Image` 等新列舉成員時，務必同步更新所有 `match self.view_mode`（包含全域快捷鍵 `Ctrl + M` 切換邏輯、Toast 提示文字、狀態列以及 CentralPanel 檢視），以避免觸發 E0004 未窮舉錯誤。

### 10. Syntect 語法高亮 LinesWithEndings 與搜尋關鍵字 LayoutJob 規範
- **Syntect 換行字元遺失導致語法著色失效**：
  - `syntect` 的語法分析狀態機預設基於含結尾換行符號的行分析。若使用 `code.lines()`，會自動去除 `\r\n` 與 `\n`，導致跨行註解、多行字串與正規表示式狀態轉移異常。
  - **解決方案**：必須使用 **`syntect::util::LinesWithEndings::from(code)`** 保留行尾 `\n` 傳入 `highlighter.highlight_line(...)`。
- **全域關鍵字搜尋高亮 (Search Highlighting)**：
  - 搜尋關鍵字需支援中文與 Unicode 字元安全切片（使用 `char_indices`），在 `LayoutJob` 中動態將符合片段套用強調背景與前景對比色，支援 Markdown、獨立程式碼與純文字檢視。

### 11. Pulldown-cmark (0.12+) 列舉結構與型別匯入規範
- **`CodeBlockKind` 型別明確匯入**：
  - `CodeBlockKind` 屬於獨立列舉型別，若使用 `CodeBlockKind::Fenced(..)`，必須於檔案頂部匯入 `use pulldown_cmark::CodeBlockKind;`，否則會觸發 `error[E0433]: cannot find type CodeBlockKind in this scope`。
- **`Tag` 與 `TagEnd` 結構體形式變體**：
  - 在 `pulldown-cmark 0.12+` 中，許多 Tag 改為帶有具名字位的結構體變體，如 `Tag::Heading { level, .. }`、`Tag::Link { dest_url, .. }`、`Tag::Image { dest_url, .. }`、`Tag::BlockQuote(..)`，匹配時務必使用結構體語法 `{ field, .. }` 或 `TagEnd::BlockQuote(..)`。

### 12. 編譯警告零容忍規範 (Zero Compiler Warnings Policy)
- **浮點數字面值明確型別後綴 (`_f32`)**：
  - 在 `Stroke::new(1.5_f32, color)` 或各類座標/邊距運算中，浮點數必須明確帶上 `_f32` 後綴，避免觸發 `#[warn(float_literal_f32_fallback)]` 未來相容性錯誤。
- **未使用的局部變數與匯入即時清除**：
  - 嚴格清理未使用的變數（如 `query_len`）與未使用的 Windows API 匯入（如 `GA_ROOTOWNER`），確保 CI/CD 雲端建置 0 警告、0 錯誤通過。

### 13. Windows 11 分頁式檔案總管與核取方塊模式選取規範 (IFolderView & SVGIO_CHECKED)
- **Windows 11 分頁與 XAML 容器 COM 介面降級問題**：
  - Windows 11 的分頁式檔案總管（`md-preview > src` 等分頁架構）在呼叫 `IWebBrowserApp.Document()` 時，若直接透過 `.cast::<IShellFolderViewDual>()` 會因 COM 介面版本跳轉回傳 `E_NOINTERFACE`，導致選取項目完全解析失敗。
- **檔案總管「項目核取方塊」模式 (`SVGIO_CHECKED`)**：
  - 當使用者開啟「項目核取方塊」模式勾選檔案（如 ☑ `main.rs`）時，選取狀態在 Windows 底層不屬於傳統的 `SVGIO_SELECTION`，而是登記於 `SVGIO_CHECKED`。
- **標準解決架構 (原生 ShellView 引擎)**：
  - **步驟 1**：自 `IShellWindows` 取得之分頁物件透過 `IServiceProvider.QueryService::<IShellBrowser>(&sid_s_top_level_browser)` 取得 `IShellBrowser`（`windows 0.58+` 中 `QueryService` 僅需傳入 service GUID，並由泛型型別推導回傳）。
  - **步驟 2**：呼叫 `shell_browser.QueryActiveShellView()` 取得 `IShellView`，並 `.cast::<IFolderView>()`。
  - **步驟 3**：依序呼叫 `folder_view.ItemCount(SVGIO_SELECTION)`、`folder_view.Items::<IShellItemArray>(SVGIO_SELECTION)`、`SVGIO_CHECKED`（支援核取方塊）以及 `GetFocusedItem()` + `SHGetPathFromIDListW(pidl, &mut [u16; 260])`。
  - **步驟 4**：透過 `IShellItemArray.GetItemAt(i).GetDisplayName(SIGDN_FILESYSPATH)` 取得絕對路徑，穿透所有 Windows 10/11 分頁與勾選模式！

---

## 🚀 標準開發與發布工作流程 (Standard Release Workflow)

當進行程式碼修改與功能更新時，請遵循以下四步驟標準流程：

```bash
# 1. 升級版本號與相關程式碼、雙語 README、AGENTS.md

# 2. 進行 Git 提交 (使用全中文 Commit Message)
git add .
git commit -m "更新說明與新功能描述..."

# 3. 推送主分支
git push origin main

# 4. 建立對應版本的 Release Tag 並推送 (觸發 GitHub Actions 雲端自動編譯與發布)
git tag vX.Y.Z
git push origin vX.Y.Z
```
