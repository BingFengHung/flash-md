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

為避免重複發生編譯與型別錯誤，於此專案（包含 Windows Shell COM API 與 egui）開發時請務必遵循以下規範：

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
  - `GetParent(hwnd)` 在 `windows` crate 中傳回 `Result<HWND, windows::core::Error>`，需進行模式匹配：
    ```rust
    match GetParent(curr) {
        Ok(p) if p.0 != 0 as _ => curr = p,
        _ => break,
    }
    ```
- **COM 方法列舉型別參數**：
  - 例如 `FindWindowSW` 中的 `SWC_DESKTOP` 與 `SWFO_NEEDDISPATCH` 已是型別安全的列舉常數，直接作為引數傳遞即可，切勿強制轉型為 `i32`。

### 2. egui 與 Rust 編譯警告規範
- **浮點數型別明確化**：
  - `egui` 中的 `Stroke::new(1.0_f32, color)` 建議帶上 `_f32` 型別後綴，避免觸發 `#[warn(float_literal_f32_fallback)]` 未來相容性警告。
- **乾淨編譯原則**：
  - 避免未使用的變數或未使用的匯入，迴圈中未使用的索引需以 `_` 開頭（如 `_i`, `_align`）。

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
