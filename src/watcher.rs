use crossbeam_channel::Sender;
use log::{debug, error, info};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum WatcherEvent {
    FileChanged(PathBuf),
}

pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    current_path: Option<PathBuf>,
    event_sender: Sender<WatcherEvent>,
}

impl FileWatcher {
    pub fn new(event_sender: Sender<WatcherEvent>) -> Self {
        Self {
            watcher: None,
            current_path: None,
            event_sender,
        }
    }

    pub fn watch_file(&mut self, path: &Path) {
        if self.current_path.as_deref() == Some(path) {
            return;
        }

        self.unwatch();

        let path_buf = path.to_path_buf();
        let target_path = path_buf.clone();
        let sender = self.event_sender.clone();

        let mut watcher = match RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            if event.paths.iter().any(|p| p == &target_path) {
                                debug!("檔案變更通知: {:?}", target_path);
                                let _ = sender.send(WatcherEvent::FileChanged(target_path.clone()));
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
    }
}
