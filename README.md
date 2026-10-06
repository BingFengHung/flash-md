# flash-md - Blazing-Fast Quick Look Markdown Preview for Windows ⚡

[English](README.md) | [繁體中文](README.zh-TW.md)

---

`flash-md` is a lightweight, blazing-fast macOS **Quick Look** style Markdown and text preview utility built specifically for Windows using pure **Rust** and **egui**.

Simply select any `.md` file in **Windows File Explorer** or on the **Desktop** and press **`Alt + Space`** to instantly preview its rendered content in a sleek, modern floating window!

---

## v1.0.108 cold preview improvements

- CSV/TSV reuse actual font advances and kerning to measure short cells without generating every text mesh twice. Wrapped and multiline cells keep their real layout heights, and visible cells plus search targets are laid out on demand. DPI and font scale participate in the geometry cache key.
- Code previews allocate the complete preview extent and lay out only visible lines. Syntax colors arrive from a background worker that coalesces pending documents and rejects stale results; reading, scrolling and searching remain available while colors are prepared.
- Search indexes the full original source and uses actual galley cursor positions on both axes, including distant Unicode matches beyond the long-line preview limit. The copy button copies the complete original source through egui's clipboard output.
- Windows CI checks first-frame and distant-search CPU time for 10,000-row CSV and 5,000-line code, and measures warm code frames after background coloring is complete. These CPU checks do not establish physical device or GPU latency.

## v1.0.107 interaction and format fixes

- Search Enter / Shift+Enter keeps focus and navigates matches. Search from editing, slides or mindmaps returns to the document preview while keeping unsaved content. Wide Markdown table searches reveal the matching column as well as the row.
- Outline, preview and mindmap anchors use the same heading IDs, including headings with images and automatic IDs that would otherwise collide with explicit IDs. UTF-8 BOM files keep their first Markdown heading and slide frontmatter; JSON formatting preserves the original BOM when saving.
- Fast navigation during a cross-folder load cannot reuse the previous folder's entries. Directory changes scan on a background worker, discard stale results and choose the correct neighbor if the current file was deleted. Pending loads also respect search input and settings keyboard focus.
- Switching from slides to editing, search, outline or mindmap consistently exits presentation fullscreen. The outline stays hidden in the editor and slides. The toolbar Close button uses the Save / Discard / Cancel flow.
- GIF previews retain animation instead of decoding only the first frame, and TIFF decoding is explicitly enabled for the image viewer. Explorer selection excludes directories and supports ZIP entries inside folders whose names end in `.zip`. Format / Minify is offered for standard `.json` files; JSON5, JSONC and JSON Lines remain available for preview and editing.
- Regression checks exercise real search keys, painted table visibility, heading scroll offsets, toolbar clicks, two GIF frames, BOM roundtrips and asynchronous navigation. Native Windows checks also open JPEG, GIF, BMP, TIFF, BOM Markdown and an archive beneath a `.zip` folder.

## v1.0.106 Responsiveness and verification

- Read files, extract PDF/ZIP content, decode images, initialize syntax/Mermaid resources and scan new directories on one background worker. Rapid navigation keeps only the latest request; cancelling, closing or editing prevents late results from replacing the current document. Navigation can advance past unreadable files, and native window lookup verifies the process ID even when the caption changes.
- CSV/TSV tables cache stable column widths and multiline row heights, paint only visible rows/columns, and locate search matches outside the viewport. Markdown search navigation is deferred until nested tables finish rendering.
- Keyboard scrolling uses elapsed frame time. `Ctrl+P` only pins the window, preview modes complete their cycle, image zoom shortcuts affect images, and settings/input fields prevent accidental file navigation. Slides and editors keep separate document state.
- Auto-save always reschedules the remaining debounce time; JSON formatting participates in auto-save. Real IME events consume confirmation Enter once while normal Chinese text/paste keep intentional newlines. Editor layouts are cached and preference changes are persisted.
- CI includes full UI-frame regression tests, optimized CPU frame measurements for eight view scenarios, and native Windows startup/responsiveness checks for text, tables, code, images, SVG, PDF, ZIP and resident mode. The Windows VM uses a checksum-pinned software OpenGL driver for these checks only; it is excluded from release packages.
- Frame measurements report CPU layout/tessellation time on the CI runner. Windows Explorer tabs, system IMEs and physical display/GPU behavior still need device-level validation.

## v1.0.105 preview fixes

- Markdown tables measure stable column widths before painting, wrap long cells, align left/center/right, and keep every cell in a row at the same height. Narrow windows scroll horizontally instead of squeezing columns.
- Outline jumps are sent after nested tables finish rendering, so headings, duplicate titles and explicit IDs scroll to the correct section.
- Alt+Space captures the foreground window at keydown, queries on one worker, ignores stale results and wakes the preview without stealing focus before loading. Sibling navigation reuses the directory cache and watcher; entry changes refresh the cache on a background worker, and each opened document starts with fresh scroll state.
- Regression tests cover painted table geometry, narrow windows, actual heading scroll offsets, delayed hotkey queries and cached sibling navigation.

## v1.0.104 fixes

- Images, PDFs and files inside ZIP archives are read-only previews. Only text and SVG source can be saved. Unsaved changes prompt for Save, Discard or Cancel before switching files, closing, exiting or updating. Conflicting external edits keep your draft and stop the overwrite.
- Updates select the matching x86_64 / ARM64 package, check download/extraction and executable format, and restore the original executable if replacement fails. Failed downloads or installations allow retry.
- Selection detection is restricted to the foreground Explorer window or desktop. Standalone windows do not install global keyboard hooks. Parsed content, image textures, outlines, tables and syntax layouts are cached and refreshed after edits or reloads.
- Search navigates using actual text layout. CSV supports quoted newlines, JSON rejects invalid input, slides preserve code blocks, and outlines and mindmaps share unique heading anchors.
- The rustls security fix is locked. Branch and main CI run formatting, Clippy, regression tests, both architecture builds and dependency audits; the release uses the same checks. A new version merged into main is tagged and published automatically after all checks pass.

---

## ✨ Features

- ⚡ **Native Blazing-Fast Rendering**: Built with pure Rust, `egui`, and `pulldown-cmark`. Zero Electron/Chromium overhead for instant startup times.
- 🧠 **Interactive Mindmap Mode**: Press **`F6`** or **`Ctrl + M`** to transform any Markdown outline into a **fluid, interactive vector mindmap**! Features smooth cubic Bezier connectors, pan & zoom, expandable/collapsible nodes (`[+]`/`[-]`), and 1-click jump back to markdown section!
- 📽️ **Full-Screen Markdown Slides Mode**: Press **`F5`** or **`P`** to instantly convert any Markdown document (split by `---`) into an elegant presentation deck with keyboard navigation (`←`/`→`/`Space`), floating controls, and fullscreen projection!
- ✏️ **In-Place Full-Screen Editor**: Press **`E`** or **`Ctrl + E`** during preview to seamlessly switch to an in-place markdown/text editor—modify files without launching heavy external editors!
- 🔤 **Smart IME Enter Filter**: Composition events prevent an IME confirmation Enter from adding an unwanted newline while preserving normal text and paste input.
- 💾 **Manual & Auto-Debounce Save**: Save manually with `Ctrl + S` or enable "Auto-debounce save (800ms)" in settings, with real-time status bar indication of unsaved changes!
- ⚙️ **Persistent User Preferences**: Click "⚙️ Settings" to customize and permanently persist your preferred **Dark/Light theme**, save mode, and font scale across reboots and updates!
- 🔍 **Smart Explorer Selection Detection**: Runs in the background and uses Windows Shell COM APIs to automatically detect the selected file when `Alt + Space` is pressed.
- 📊 **Instant Mermaid Diagrams**: Pure Rust, zero-browser in-memory rendering of ````mermaid ```` code blocks into crisp vector SVGs (flowcharts, sequence diagrams, mindmaps, state diagrams, etc.)!
- 📑 **Instant PDF Text Preview**: Instant in-memory text extraction for `.pdf` files, formatted into structured Markdown pages with TOC and search!
- 📊 **Word Count & Reading Time Estimation**: Real-time CJK / English word count, estimated reading time, and an elegant top reading progress bar!
- ⬅️➡️ **Keyboard Sibling File Navigation**: Press `←` / `→` (or click `◀` / `▶` buttons) to instantly browse previous/next files in the same directory, complete with index indicators `[3/18]`!
- 📜 **Smooth Document Keyboard Scrolling**: Scroll through long documents seamlessly using `↑` / `↓` or `PageUp` / `PageDown` / `Home` / `End` keys!
- ⚡ **Full Large-File Preview**: File reading and preparation run on a background worker; CSV/TSV keeps all rows available while painting only the visible cells.
- 📋 **1-Click Code Block Copying**: Code blocks in Markdown and the standalone Code Viewer now feature dedicated copy buttons with instant green "✓ Copied" feedback.
- 🔍 **Robust Full-Text Search (Ctrl + F or /)**: Live match count (`Match X / Y`), auto-focus on open, **vivid electric orange active focus highlight**, jump to next/previous matches via `Enter` / `n` or `Shift + Enter` / `N` / `F3`, and Unicode-safe text highlighting.
- ⚡ **Vim-Style Navigation**: Supports `/` to search, `n` / `N` to navigate matches, `h` / `l` for sibling files, `j` / `k` for smooth scrolling, and `g` / `G` to jump to top/bottom!
- 📑 **Markdown TOC Outline Sidebar (Ctrl + T)**: Toggle document table of contents outline to jump instantly to any heading!
- 📊 **CSV / TSV Zebra-Striped Data Tables**: Automatically renders structured tabular data with zebra striping, search highlighting, and smooth scrolling!
- ⚡ **Validated JSON Format & Minify**: One-click beautify (2-space indent) or compress minified JSON files directly in the toolbar.
- 📁 **Locate in Windows File Explorer (Ctrl + Shift + O)**: Instantly reveals and highlights the currently previewed file in Windows File Explorer.
- 🖼️ **Instant Image & SVG Vector Preview**: Supports PNG, JPG, JPEG, animated GIF, WEBP, BMP, ICO, TIFF and SVG formats with smooth mouse wheel zooming, panning, and auto-fit to window!
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
| **`F5`** or **`P`** | **Slides Mode**: Enter / exit full-screen Markdown presentation slides (supports arrow keys) |
| **`E`** or **`Ctrl + E`** | **In-place Editor**: Seamlessly toggle between rendered preview and full-screen editor |
| **`Ctrl + S`** | **Save File**: Manually save in-place modifications to disk (or enable auto-debounce save) |
| **`←` / `→`** or **`h` / `l`** | **Browse Files**: Navigate to previous / next file in the same directory (with `[3/18]` index) |
| **`↑` / `↓`** or **`j` / `k`** | **Scroll Document**: Scroll up / down inside current document (supports continuous smooth scrolling) |
| **`PageUp` / `PageDown`** | **Page Scroll**: Fast page up / page down scrolling |
| **`Home` / `End`** or **`g` / `G`** | **Jump to Top / Bottom**: Jump directly to the top or bottom of the document |
| **`Esc`** | Exit fullscreen / exit in-place editor / close search bar / instantly hide preview window |
| **`F11`** | **Fullscreen**: Toggle between fullscreen and windowed preview mode |
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
Download the latest `flash-md-windows-x86_64.zip` or `flash-md-windows-aarch64.zip` for your architecture from [GitHub Releases](https://github.com/BingFengHung/flash-md/releases), extract it, and run `flash-md.exe`.

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

### Option 4: Run on Windows Startup (Optional)
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
│   ├── app/            # Document state, unsaved prompts, shortcuts and updates
│   ├── document.rs     # Source types and atomic saves
│   ├── files.rs        # File and ZIP content loading
│   ├── parsers.rs      # Markdown, CSV, JSON and slide parsing
│   ├── search.rs       # Unicode search and layout-based navigation
│   ├── textures.rs     # Image decoding and texture caches
│   ├── app.rs          # egui preview UI, toolbar, interactive logic
│   ├── explorer.rs     # Windows Shell COM API file detection
│   ├── hotkey.rs       # Win32 WH_KEYBOARD_LL global hotkey thread
│   ├── markdown/       # Code, PDF and Mermaid renderers
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

This project is licensed under the [MIT](LICENSE-MIT) license.
