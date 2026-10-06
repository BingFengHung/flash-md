use super::*;

pub(super) fn app() -> MdPreviewApp {
    let (_, hotkey_rx) = unbounded();
    let (watcher_tx, watcher_rx) = unbounded();
    let (_, tray_rx) = unbounded();
    let holder = Arc::new(Mutex::new(Some(Context::default())));
    let watcher = FileWatcher::new(watcher_tx, holder.clone());
    MdPreviewApp::empty(
        AppConfig::default(),
        true,
        true,
        watcher,
        hotkey_rx,
        watcher_rx,
        tray_rx,
        holder,
    )
}

#[test]
fn opening_or_reloading_a_failed_file_preserves_the_current_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.md");
    fs::write(&path, "original").unwrap();
    let mut app = app();
    app.open_document(&path);
    settle(&mut app);
    app.open_document(&dir.path().join("missing.md"));
    settle(&mut app);
    assert_eq!(app.current_file.as_deref(), Some(path.as_path()));
    assert_eq!(app.content, "original");
    app.content = "unsaved draft".to_string();
    app.is_modified = true;
    fs::write(&path, "external change").unwrap();
    app.reload_current_file();
    settle(&mut app);
    assert_eq!(app.content, "unsaved draft");
    assert!(app.is_modified);
}

#[test]
fn cancel_keeps_draft_and_failed_save_keeps_pending_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.md");
    let next = dir.path().join("next.md");
    fs::write(&path, "original").unwrap();
    fs::write(&next, "next").unwrap();
    let mut app = app();
    app.open_document(&path);
    settle(&mut app);
    app.content = "draft".to_string();
    app.is_modified = true;
    app.load_file(&next);
    assert!(app.pending_action.is_some());
    assert_eq!(app.content, "draft");
    app.resolve_pending_action(UnsavedChoice::Cancel);
    assert!(app.pending_action.is_none());
    assert!(app.is_modified);
    app.load_file(&next);
    fs::write(&path, "external").unwrap();
    app.resolve_pending_action(UnsavedChoice::Save);
    settle(&mut app);
    assert!(app.pending_action.is_some());
    assert_eq!(app.content, "draft");
    assert!(app.is_modified);
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    app.resolve_pending_action(UnsavedChoice::Discard);
    settle(&mut app);
    assert_eq!(app.current_file, Some(next));
    assert_eq!(app.content, "next");
    assert!(!app.is_modified);
}

#[test]
fn save_and_continue_writes_the_draft_before_switching() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.md");
    let next = dir.path().join("next.md");
    fs::write(&path, "original").unwrap();
    fs::write(&next, "next").unwrap();
    let mut app = app();
    app.open_document(&path);
    settle(&mut app);
    app.content = "draft".to_string();
    app.is_modified = true;
    app.load_file(&next);
    app.resolve_pending_action(UnsavedChoice::Save);
    settle(&mut app);
    assert_eq!(fs::read_to_string(&path).unwrap(), "draft");
    assert_eq!(app.current_file, Some(next));
    assert!(app.pending_action.is_none());
    assert!(!app.is_modified);
}

#[test]
fn image_reload_changes_bytes_and_derived_caches_are_invalidated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.png");
    fs::write(&path, b"old image").unwrap();
    let mut app = app();
    app.open_document(&path);
    settle(&mut app);
    let uri = app.image_uri.clone();
    let revision = app.content_revision;
    fs::write(&path, b"new image").unwrap();
    app.reload_current_file();
    settle(&mut app);
    assert_ne!(app.image_uri, uri);
    assert_eq!(app.image_bytes.as_deref(), Some(b"new image".as_slice()));
    assert!(app.content_revision > revision);
    app.mindmap_root = Some(crate::views::mindmap::parse_markdown_to_mindmap(
        "# old", "old",
    ));
    app.invalidate_content();
    assert!(app.mindmap_root.is_none());
}

#[test]
fn updater_failure_unlocks_the_ui_and_allows_retry() {
    let mut app = app();
    app.is_updating = true;
    app.handle_update_event(UpdateEvent::Installed(Err("download failed".to_string())));
    assert!(!app.is_updating);
    assert!(app
        .status_toast
        .as_ref()
        .unwrap()
        .0
        .contains("download failed"));
}

#[test]
fn hotkey_switching_documents_resets_view_and_keeps_drafts_guarded() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.md");
    let second = dir.path().join("second.md");
    fs::write(&first, "# First").unwrap();
    fs::write(&second, "# Second").unwrap();
    let mut app = app();
    app.handle_hotkey_preview(Some(first.clone()));
    settle(&mut app);
    let generation = app.preview_generation;
    app.current_scroll_offset = 500.0_f32;
    app.target_anchor = Some("first".to_string());
    app.handle_hotkey_preview(Some(second.clone()));
    settle(&mut app);
    assert_eq!(app.content, "# Second");
    assert_eq!(app.current_file.as_deref(), Some(second.as_path()));
    assert_eq!(app.current_scroll_offset, 0.0_f32);
    assert!(app.target_anchor.is_none());
    assert!(app.preview_generation > generation);
    assert!(app.visible);
    app.content = "draft".to_string();
    app.is_modified = true;
    app.handle_hotkey_preview(Some(first));
    assert_eq!(app.content, "draft");
    assert!(app.pending_action.is_some());
}

#[test]
fn sibling_navigation_uses_the_existing_directory_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("a.md");
    let second = dir.path().join("c.md");
    fs::write(&first, "first").unwrap();
    fs::write(&second, "second").unwrap();
    let mut app = app();
    app.open_document(&first);
    settle(&mut app);
    // New entries are picked up by DirectoryChanged, not a scan on every key.
    fs::write(dir.path().join("b.md"), "new entry").unwrap();
    app.navigate_sibling_file(true);
    settle(&mut app);
    assert_eq!(app.current_file.as_deref(), Some(second.as_path()));
    assert_eq!(app.content, "second");
    assert_eq!(app.siblings.len(), 2);
}

pub(super) fn settle(app: &mut MdPreviewApp) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.loading_request.is_some() {
        app.poll_document_loads();
        assert!(
            std::time::Instant::now() < deadline,
            "background load timed out"
        );
        if app.loading_request.is_some() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn navigation_continues_past_unreadable_files_without_replacing_the_previous_document() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("a.md");
    let bad = directory.path().join("b.bin");
    let last = directory.path().join("c.md");
    fs::write(&first, "first").unwrap();
    fs::write(&bad, [0xff, 0xfe, 0xff]).unwrap();
    fs::write(&last, "last").unwrap();
    let mut app = app();
    app.open_document(&first);
    settle(&mut app);
    app.navigate_sibling_file(true);
    settle(&mut app);
    assert_eq!(app.current_file.as_deref(), Some(first.as_path()));
    assert_eq!(app.content, "first");
    app.navigate_sibling_file(true);
    settle(&mut app);
    assert_eq!(app.current_file.as_deref(), Some(last.as_path()));
    assert_eq!(app.content, "last");
}

#[test]
fn pending_cross_directory_load_does_not_navigate_using_the_old_siblings() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let old = first.join("a.md");
    let next = second.join("b.md");
    fs::write(&old, "old").unwrap();
    fs::write(first.join("c.md"), "old neighbor").unwrap();
    fs::write(&next, "new folder").unwrap();
    let mut app = app();
    app.open_document(&old);
    settle(&mut app);
    app.open_document(&next);
    let request_id = app.loading_request.as_ref().unwrap().id;
    app.navigate_sibling_file(true);
    app.navigate_sibling_file(false);
    assert_eq!(app.loading_request.as_ref().unwrap().id, request_id);
    settle(&mut app);
    assert_eq!(app.current_file, Some(next));
    assert_eq!(app.content, "new folder");
}

#[test]
fn deleting_the_current_file_selects_the_nearest_surviving_neighbor_in_both_directions() {
    for (forward, expected) in [(true, "c.md"), (false, "a.md")] {
        let directory = tempfile::tempdir().unwrap();
        for name in ["a.md", "b.md", "c.md"] {
            fs::write(directory.path().join(name), name).unwrap();
        }
        let current = directory.path().join("b.md");
        let mut app = app();
        app.open_document(&current);
        settle(&mut app);
        fs::remove_file(&current).unwrap();
        app.request_directory_refresh();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app.directory_request.is_some() {
            app.poll_directory_scans();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.navigate_sibling_file(forward);
        settle(&mut app);
        assert_eq!(app.current_file, Some(directory.path().join(expected)));
    }
}

#[test]
fn late_directory_results_cannot_replace_a_new_folders_snapshot_or_a_cleared_document() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    fs::write(first.join("a.md"), "old").unwrap();
    fs::write(second.join("b.md"), "new").unwrap();
    let mut app = app();
    app.open_document(&first.join("a.md"));
    settle(&mut app);
    app.request_directory_refresh();
    app.open_document(&second.join("b.md"));
    settle(&mut app);
    app.poll_directory_scans();
    assert_eq!(app.siblings, vec![second.join("b.md")]);
    app.request_directory_refresh();
    app.execute_action(PendingAction::Clear);
    app.poll_directory_scans();
    assert!(app.directory_request.is_none());
    assert!(app.siblings.is_empty());
    assert!(app.current_file.is_none());
}

#[test]
fn json_formatting_and_saving_preserve_the_original_utf8_bom() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bom.json");
    let original = "\u{feff}{\"message\":\"中文\",\"number\":7}";
    fs::write(&path, original).unwrap();
    let mut app = app();
    app.open_document(&path);
    settle(&mut app);
    app.format_json_content();
    assert!(app.is_modified);
    assert!(app.save_current_file(false));
    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with('\u{feff}'));
    assert_eq!(crate::parsers::minify_json(&saved).unwrap(), original);
}
