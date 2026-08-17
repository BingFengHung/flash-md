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

## 🚀 標準開發與發布工作流程 (Standard Release Workflow)

當進行程式碼修改與功能更新時，請遵循以下四步驟標準流程：

```bash
# 1. 升級版本號與相關程式碼、雙語 README

# 2. 進行 Git 提交 (使用全中文 Commit Message)
git add .
git commit -m "更新說明與新功能描述..."

# 3. 推送主分支
git push origin main

# 4. 建立對應版本的 Release Tag 並推送 (觸發 GitHub Actions 雲端自動編譯與發布)
git tag vX.Y.Z
git push origin vX.Y.Z
```
