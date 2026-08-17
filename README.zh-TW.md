# flash-md - Windows 快捷鍵極速 Markdown 預覽工具 ⚡

[English](README.md) | [繁體中文](README.zh-TW.md)

---

`flash-md` 是一款專為 Windows 打造、類似 macOS **Quick Look** 體驗的極速 Markdown 與文字檔案預覽工具。以純 **Rust** 與 **egui** 原生渲染技術開發，兼具極致輕量、無網頁框架包袱、閃電級啟動與現代美觀等特性。

只要在 **Windows 檔案總管** 或 **桌面** 選取任何 `.md` 檔案並按下 **`Alt + Space`**，即可瞬間彈出浮動視窗預覽內容！

---

## ✨ 核心特色 (Features)

- ⚡ **原生極速渲染**：基於 `egui` 與 `pulldown-cmark` 純 Rust 打造，無 Electron / Chromium 肥大負擔，開檔即顯。
- 🔍 **智慧檔案總管偵測**：背景常駐並透過 Windows Shell COM 介面，在觸發 `Alt + Space` 時自動取得檔案總管或桌面選取的檔案路徑。
- 🖼️ **圖片與 SVG 向量圖秒開**：支援 PNG, JPG, JPEG, GIF, WEBP, BMP, ICO, SVG, AVIF 等格式，支援滑鼠滾輪縮放、平移與適應視窗！
- 💻 **全語言程式碼語法高亮**：支援 Rust, Python, TypeScript, JavaScript, HTML, CSS, C++, Go, JSON, TOML, YAML, SQL 等 100+ 種程式碼檔案，內建專業行號欄 (Line Numbers Gutter) 與語法著色！
- 📝 **Markdown / 程式碼 / 純文字 / 圖片多軌分流**：依檔案類型自動選用最佳渲染器，並可隨時按 `Ctrl + M` 一鍵切換。
- 🎨 **現代深淺色主題**：支援深色 (Dark) 與淺色 (Light) 主題一鍵切換，內建 GitHub 風格精緻排版與語法邊框。
- 🔄 **檔案變更即時熱重載 (Live Reload)**：若預覽中的檔案在 VSCode、Obsidian 或其他編輯器被修改儲存，預覽視窗將自動即時更新。
- 📌 **視窗置頂與快速控制**：支援 `Esc` 快速隱藏、`Ctrl + P` 視窗置頂、`Ctrl + O` 在系統預設編輯器中開啟、`Ctrl + + / -` 即時縮放文字大小。
- 📥 **系統匣常駐 (System Tray)**：常駐於 Windows 工作列右下角系統匣，提供豐富捷徑選單。
- 🖥️ **CLI 模式支援**：可作為獨立命令列預覽器（例如 `flash-md README.md`）。

---

## ⌨️ 快捷鍵指南 (Keyboard Shortcuts)

| 快捷鍵 | 動作說明 |
| :--- | :--- |
| **`Alt + Space`** | **全域快捷鍵**：預覽檔案總管/桌面當前選取的檔案（再次按下可快速收起視窗） |
| **`Esc`** | 快速隱藏預覽視窗 / 關閉搜尋欄 |
| **`Ctrl + M`** | **切換檢視模式**：在 Markdown 渲染、程式碼高亮、純文字、圖片檢視模式之間循環切換 |
| **`Ctrl + O`** | 在系統預設應用程式 (如 VSCode / 記事本 / 圖片檢視器) 中開啟目前預覽的檔案 |
| **`Ctrl + Shift + C`** | 一鍵複製目前文件的完整內文或檔案路徑至剪貼簿 |
| **`Ctrl + P`** | 切換視窗是否置頂 (Always on Top) |
| **`Ctrl + +` / `Ctrl + =`** | 放大預覽文字字級 / 圖片比例 |
| **`Ctrl + -`** | 縮小預覽文字字級 / 圖片比例 |
| **`Ctrl + 0`** | 重設為預設文字字級 / 原始圖片大小 (100%) |
| **`Ctrl + F`** | 開啟 / 關閉內文搜尋欄 |

---

## 📦 安裝與使用方式 (Installation & Usage)

### 方式 1：直接下載發布執行檔 (推薦)
前往 [GitHub Releases](https://github.com/BingFengHung/flash-md/releases) 頁面下載最新版 `flash-md-windows-x86_64.zip`，解壓縮後執行 `flash-md.exe` 即可常駐於系統匣中。

### 方式 2：透過命令列執行或自動更新
```powershell
# 常駐背景監聽快捷鍵 (預設)
flash-md.exe

# 檢查並自動升級至 GitHub 最新版本 🔄
flash-md.exe --update

# 直接預覽指定檔案 (單獨視窗模式)
flash-md.exe path/to/document.md
```

### 方式 3：GUI 介面與系統匣一鍵更新
- **背景檢查**：程式啟動時會在背景自動檢查 GitHub Releases，若發現新版本將在頂部顯示「🚀 一鍵自動升級」按鈕。
- **系統匣選單**：右鍵點擊工作列右下角閃電圖示，選擇 **「🔄 檢查更新 (Check Update)...」** 即可隨時線上手動檢查並升級。

### 方式 3：開機自動啟動 (選用)
將 `flash-md.exe` 的捷徑放入 Windows 開機啟動資料夾：
1. 按下 `Win + R` 輸入 `shell:startup` 並按 Enter。
2. 將 `flash-md.exe` 的捷徑貼上至該資料夾中。

---

## 🛠️ 開發與架構說明 (Architecture)

```
flash-md/
├── .github/workflows/
│   └── release.yml     # 雲端 CI/CD 自動編譯與 Release 發布工作流
├── src/
│   ├── main.rs         # 程式進入點、CLI 參數處理、執行緒協調
│   ├── app.rs          # egui 預覽視窗 UI、工具列、操作邏輯
│   ├── explorer.rs     # Windows Shell COM API 檔案總管選取偵測
│   ├── hotkey.rs       # Win32 RegisterHotKey 全域快捷鍵監聽執行緒
│   ├── markdown.rs     # pulldown-cmark 解析與 syntect 語法高亮渲染引擎
│   ├── theme.rs        # 深色/淺色主題調色盤與設計系統
│   ├── tray.rs         # Windows 系統匣常駐圖示與右鍵功能選單
│   └── watcher.rs      # notify 檔案系統即時變更監視器 (熱重載)
├── Cargo.toml          # 專案相依套件與編譯設定
├── AGENTS.md           # 專案規範與 CI/CD 流程說明
├── README.md           # 英文說明文件
└── README.zh-TW.md     # 繁體中文說明文件
```

---

## 📄 授權條款 (License)

本專案採用 [MIT OR Apache-2.0](LICENSE) 雙重授權。
