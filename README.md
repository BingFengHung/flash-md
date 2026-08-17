# md-preview - Blazing-Fast Quick Look Markdown Preview for Windows ⚡

[English](README.md) | [繁體中文](README.zh-TW.md)

---

`md-preview` is a lightweight, blazing-fast macOS **Quick Look** style Markdown and text preview utility built specifically for Windows using pure **Rust** and **egui**.

Simply select any `.md` file in **Windows File Explorer** or on the **Desktop** and press **`Alt + Space`** to instantly preview its rendered content in a sleek, modern floating window!

---

## ✨ Features

- ⚡ **Native Blazing-Fast Rendering**: Built with pure Rust, `egui`, and `pulldown-cmark`. Zero Electron/Chromium overhead for instant startup times.
- 🔍 **Smart Explorer Selection Detection**: Runs in the background and uses Windows Shell COM APIs (`IShellWindows`) to automatically detect the selected file when `Alt + Space` is pressed.
- 🎨 **Modern Dark & Light Themes**: Seamlessly toggle between dark and light modes with GitHub-style typography and clean borders.
- 💻 **Syntax Highlighting**: Powered by `syntect` supporting dozens of languages (Rust, Python, TypeScript, JavaScript, HTML, CSS, C++, Go, JSON, YAML, etc.) with 1-click code copying.
- 🔄 **Live Hot-Reload**: Automatically detects file modifications when saved in external editors (VSCode, Obsidian, Notepad) and updates the preview in real-time.
- 📌 **Quick Window Controls**: `Esc` to instantly dismiss, `Ctrl + P` to toggle Always on Top, `Ctrl + O` to open in your default editor, `Ctrl + + / -` for smooth zoom scaling.
- 📥 **System Tray Resident**: Sits unobtrusively in the Windows taskbar system tray with quick action menus.
- 🖥️ **CLI Support**: Can also be used as a standalone terminal markdown viewer (e.g., `md-preview README.md`).

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Description |
| :--- | :--- |
| **`Alt + Space`** | **Global Hotkey**: Preview selected file in File Explorer / Desktop (press again to close) |
| **`Esc`** | Instantly hide preview window / close search bar |
| **`Ctrl + O`** | Open current file in default system editor (e.g. VSCode / Notepad) |
| **`Ctrl + Shift + C`** | Copy entire Markdown source to clipboard |
| **`Ctrl + P`** | Toggle Always on Top window pin |
| **`Ctrl + +` / `Ctrl + =`** | Zoom in preview font size |
| **`Ctrl + -`** | Zoom out preview font size |
| **`Ctrl + 0`** | Reset preview font size (100%) |
| **`Ctrl + F`** | Open / close search bar |

---

## 📦 Installation & Usage

### Option 1: Download Pre-built Binary (Recommended)
Download the latest `md-preview-windows-x86_64.zip` from [GitHub Releases](https://github.com/your-username/md-preview/releases), extract it, and run `md-preview.exe`.

### Option 2: CLI Usage
```powershell
# Run resident background daemon (default)
md-preview.exe

# Standalone preview of a specific file
md-preview.exe path/to/document.md
```

### Option 3: Run on Windows Startup (Optional)
To launch `md-preview` automatically when Windows starts:
1. Press `Win + R`, type `shell:startup`, and press Enter.
2. Place a shortcut to `md-preview.exe` into that folder.

---

## 🛠️ Architecture

```
md-preview/
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
