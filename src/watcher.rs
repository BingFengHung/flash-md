use crossbeam_channel::Sender;
use egui::Context;
use log::{debug, error, info};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum WatcherEvent {
    FileChanged(PathBuf),
    DirectoryChanged,
}

pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    current_path: Option<PathBuf>,
    target_path: Arc<Mutex<Option<PathBuf>>>,
    event_sender: Sender<WatcherEvent>,
    ctx_holder: Arc<Mutex<Option<Context>>>,
}

impl FileWatcher {
    pub fn new(
        event_sender: Sender<WatcherEvent>,
        ctx_holder: Arc<Mutex<Option<Context>>>,
    ) -> Self {
        Self {
            watcher: None,
            current_path: None,
            target_path: Arc::new(Mutex::new(None)),
            event_sender,
            ctx_holder,
        }
    }

    pub fn watch_file(&mut self, path: &Path) {
        if self.current_path.as_deref() == Some(path) {
            return;
        }

        if self.watcher.is_some()
            && self
                .current_path
                .as_ref()
                .is_some_and(|current| current.parent() == path.parent())
        {
            self.current_path = Some(path.to_path_buf());
            if let Ok(mut target) = self.target_path.lock() {
                *target = self.current_path.clone();
            }
            return;
        }

        self.unwatch();

        let path_buf = path.to_path_buf();
        let target_path = self.target_path.clone();
        let sender = self.event_sender.clone();
        let ctx_holder = self.ctx_holder.clone();

        let mut watcher = match RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                            let directory_changed = directory_entries_changed(&event.kind);
                            if directory_changed {
                                let _ = sender.send(WatcherEvent::DirectoryChanged);
                            }
                            let target = target_path
                                .lock()
                                .ok()
                                .and_then(|target| target.clone())
                                .filter(|target| event.paths.contains(target));
                            let file_changed = target.is_some();
                            if let Some(target) = target {
                                debug!("檔案變更通知: {:?}", target);
                                let _ = sender.send(WatcherEvent::FileChanged(target));
                            }
                            if directory_changed || file_changed {
                                if let Ok(guard) = ctx_holder.lock() {
                                    if let Some(ref ctx) = *guard {
                                        ctx.request_repaint();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        ) {
            Ok(w) => w,
            Err(e) => {
                error!("無法初始化檔案監視器: {:?}", e);
                return;
            }
        };

        if let Some(parent) = path.parent() {
            if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                error!("監視目錄失敗 {:?}: {:?}", parent, e);
            } else {
                info!("開始監視檔案變更: {:?}", path);
                self.watcher = Some(watcher);
                if let Ok(mut target) = self.target_path.lock() {
                    *target = Some(path_buf.clone());
                }
                self.current_path = Some(path_buf);
            }
        }
    }

    pub fn unwatch(&mut self) {
        if let Some(ref mut watcher) = self.watcher {
            if let Some(ref path) = self.current_path {
                if let Some(parent) = path.parent() {
                    let _ = watcher.unwatch(parent);
                }
            }
        }
        self.watcher = None;
        self.current_path = None;
        if let Ok(mut target) = self.target_path.lock() {
            *target = None;
        }
    }
}

fn directory_entries_changed(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(notify::event::ModifyKind::Name(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};

    #[test]
    fn directory_snapshot_changes_for_entries_not_content_modifications() {
        assert!(directory_entries_changed(&EventKind::Create(
            CreateKind::File
        )));
        assert!(directory_entries_changed(&EventKind::Remove(
            RemoveKind::File
        )));
        assert!(directory_entries_changed(&EventKind::Modify(
            ModifyKind::Name(RenameMode::Both)
        )));
        assert!(!directory_entries_changed(&EventKind::Modify(
            ModifyKind::Any
        )));
    }
}
