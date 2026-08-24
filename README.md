# flash-md - Blazing-Fast Quick Look Markdown Preview for Windows ⚡

[English](README.md) | [繁體中文](README.zh-TW.md)

---

`flash-md` is a lightweight, blazing-fast macOS **Quick Look** style Markdown and text preview utility built specifically for Windows using pure **Rust** and **egui**.

Simply select any `.md` file in **Windows File Explorer** or on the **Desktop** and press **`Alt + Space`** to instantly preview its rendered content in a sleek, modern floating window!

---

## ✨ Features

- ⚡ **Native Blazing-Fast Rendering**: Built with pure Rust, `egui`, and `pulldown-cmark`. Zero Electron/Chromium overhead for instant startup times.
- 🔍 **Smart Explorer Selection Detection**: Runs in the background and uses Windows Shell COM APIs to automatically detect the selected file when `Alt + Space` is pressed.
- 📊 **Instant Mermaid Diagrams**: Pure Rust, zero-browser in-memory rendering of ````mermaid ```` code blocks into crisp vector SVGs (flowcharts, sequence diagrams, mindmaps, state diagrams, etc.)!
- 📑 **Instant PDF Text Preview**: Instant in-memory text extraction for `.pdf` files, formatted into structured Markdown pages with TOC and search!
- 📊 **Word Count & Reading Time Estimation**: Real-time CJK / English word count, estimated reading time, and an elegant top reading progress bar!
- ⬅️➡️ **Keyboard Sibling File Navigation**: Press `←` / `→` (or click `◀` / `▶` buttons) to instantly browse previous/next files in the same directory, complete with index indicators `[3/18]`!
- 📜 **Smooth Document Keyboard Scrolling**: Scroll through long documents seamlessly using `↑` / `↓` or `PageUp` / `PageDown` / `Home` / `End` keys!
- 📋 **1-Click Code Block Copying**: Code blocks in Markdown and the standalone Code Viewer now feature dedicated copy buttons with instant green "✓ Copied" feedback.
- 🔍 **Robust Full-Text Search (Ctrl + F or /)**: Live match count (`Match X / Y`), auto-focus on open, **vivid electric orange active focus highlight**, jump to next/previous matches via `Enter` / `n` or `Shift + Enter` / `N` / `F3`, and Unicode-safe text highlighting.
- ⚡ **Vim-Style Navigation**: Supports `/` to search, `n` / `N` to navigate matches, `h` / `l` for sibling files, `j` / `k` for smooth scrolling, and `g` / `G` to jump to top/bottom!
- 📑 **Markdown TOC Outline Sidebar (Ctrl + T)**: Toggle document table of contents outline to jump instantly to any heading!
- 📊 **CSV / TSV Zebra-Striped Data Tables**: Automatically renders structured tabular data with zebra striping, search highlighting, and smooth scrolling!
- ⚡ **Zero-Dependency JSON Format & Minify**: One-click beautify (2-space indent) or compress minified JSON files directly in the toolbar.
- 📁 **Locate in Windows File Explorer (Ctrl + Shift + O)**: Instantly reveals and highlights the currently previewed file in Windows File Explorer.
- 🖼️ **Instant Image & SVG Vector Preview**: Supports PNG, JPG, JPEG, GIF, WEBP, BMP, ICO, SVG, AVIF formats with smooth mouse wheel zooming, panning, and auto-fit to window!
- 💻 **100+ Formats & Syntax Highlighting**: Supports Markdown, Rust, Python, TypeScript, JavaScript, HTML, CSS, C++, Go, JSON, TOML, YAML, CSV, SQL, Dockerfile, and more!
- 📝 **Multi-Track Mode Switching**: Automatically routes Markdown, Source Code, Plain Text, and Images to their optimal viewers, with instant cycling via `Ctrl + M`.
- 🎨 **Modern Dark & Light Themes**: Seamlessly toggle between dark and light modes with GitHub-style typography and clean borders.
- 🔄 **Live Hot-Reload**: Automatically detects file modifications when saved in external editors (VSCode, Obsidian, Notepad) and updates the preview in real-time.
- 📌 **Quick Window Controls**: `Esc` to instantly dismiss, `Ctrl + P` to toggle Always on Top, `Ctrl + O` to open in your default editor, `Ctrl + + / -` for smooth zoom scaling.
- 📥 **System Tray Resident**: Sits unobtrusively in the Windows taskbar system tray with quick action menus.
- 🖥️ **CLI Support**: Can also be used as a standalone terminal markdown viewer (e.g., `flash-md README.md`).

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Description |
| :--- | :--- |
| **`Alt + Space`** | **Global Hotkey**: Preview selected file in File Explorer / Desktop (press again to close) |
| **`←` / `→`** or **`h` / `l`** | **Browse Files**: Navigate to previous / next file in the same directory (with `[3/18]` index) |
| **`↑` / `↓`** or **`j` / `k`** | **Scroll Document**: Scroll up / down inside current document (supports continuous smooth scrolling) |
| **`PageUp` / `PageDown`** | **Page Scroll**: Fast page up / page down scrolling |
| **`Home` / `End`** or **`g` / `G`** | **Jump to Top / Bottom**: Jump directly to the top or bottom of the document |
| **`Esc`** | Instantly hide preview window / close search bar |
| **`Ctrl + F`** or **`/`** | Open search bar and auto-focus search input (Vim style) |
| **`Enter`** / **`n`** / **`F3`** | **Next Match**: Automatically scroll and jump to next search match (vivid orange focus highlight) |
| **`Shift + Enter`** / **`N`** / **`Shift + F3`** | **Previous Match**: Automatically scroll and jump to previous search match |
| **`Ctrl + T`** | **Outline / TOC**: Toggle Markdown TOC outline sidebar to navigate headings |
| **`Ctrl + Shift + O`** | **Locate in Explorer**: Reveal and select the file in Windows File Explorer |
| **`Ctrl + M`** | **Cycle View Mode**: Switch between Markdown, Data Table, Code Highlight, Plain Text, and Image view |
| **`Ctrl + O`** | Open current file in default system editor / image viewer |
| **`Ctrl + Shift + C`** | Copy entire document content or file path to clipboard |
| **`Ctrl + P`** | Toggle Always on Top window pin |
| **`Ctrl + +` / `Ctrl + =`** | Zoom in preview font size / image scale |
| **`Ctrl + -`** | Zoom out preview font size / image scale |
| **`Ctrl + 0`** | Reset preview font size / image scale (100%) |

---

## 📦 Installation & Usage

### Option 1: Download Pre-built Binary (Recommended)
Download the latest `flash-md-windows-x86_64.zip` from [GitHub Releases](https://github.com/BingFengHung/flash-md/releases), extract it, and run `flash-md.exe`.

### Option 2: CLI Usage & Auto-Update
```powershell
# Run resident background daemon (default)
flash-md.exe

# Check and auto-update to latest GitHub release 🔄
flash-md.exe --update

# Standalone preview of a specific file
flash-md.exe path/to/document.md
```

### Option 3: In-App & Tray 1-Click Update
- **Background Checks**: Automatically checks for new GitHub Releases on startup and displays an upgrade banner.
- **System Tray**: Right-click the system tray icon and select **"🔄 Check Update..."** to check and update anytime.

### Option 3: Run on Windows Startup (Optional)
To launch `flash-md` automatically when Windows starts:
1. Press `Win + R`, type `shell:startup`, and press Enter.
2. Place a shortcut to `flash-md.exe` into that folder.

---

## 🛠️ Architecture

```
flash-md/
├── .github/workflows/
│   └── release.yml     # Cloud CI/CD matrix build & GitHub release workflow
├── src/
│   ├── main.rs         # Entry point, CLI parsing, event coordination
│   ├── app.rs          # egui preview UI, toolbar, interactive logic
│   ├── explorer.rs     # Windows Shell COM API file detection
│   ├── hotkey.rs       # Win32 RegisterHotKey global hotkey thread
│   ├── markdown.rs     # pulldown-cmark parser & syntect syntax highlighter
│   ├── theme.rs        # Design tokens and Dark/Light palette
│   ├── tray.rs         # Windows system tray icon and context menu
│   └── watcher.rs      # notify live filesystem watcher
├── Cargo.toml          # Rust dependencies and configuration
├── AGENTS.md           # Developer guidelines & CI/CD workflow
├── README.md           # English documentation
└── README.zh-TW.md     # Traditional Chinese documentation
```

---

## 📄 License

This project is licensed under the terms of the [MIT OR Apache-2.0](LICENSE) dual license.
