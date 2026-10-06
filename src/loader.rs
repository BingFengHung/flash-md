use crate::files::{load_document, sibling_files, LoadedDocument};
use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::Context;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub id: u64,
    pub path: PathBuf,
    pub reset_view: bool,
    pub revision: u64,
    pub scan_directory: bool,
}

pub struct LoadResult {
    pub request: LoadRequest,
    pub document: Result<LoadedDocument, String>,
    pub siblings: Option<Vec<PathBuf>>,
}

pub struct DocumentLoader {
    sender: Sender<LoadRequest>,
    pub receiver: Receiver<LoadResult>,
    latest: Arc<AtomicU64>,
}

impl DocumentLoader {
    pub fn new(holder: Arc<Mutex<Option<Context>>>) -> Self {
        Self::spawn(holder, |request| {
            let document = load_document(&request.path)?;
            crate::markdown::prepare_document_rendering(&document.content);
            Ok(document)
        })
    }

    fn spawn(
        holder: Arc<Mutex<Option<Context>>>,
        read: impl Fn(&LoadRequest) -> Result<LoadedDocument, String> + Send + 'static,
    ) -> Self {
        let (sender, requests) = unbounded::<LoadRequest>();
        let (results, receiver) = unbounded();
        let latest = Arc::new(AtomicU64::new(0));
        let worker_latest = latest.clone();
        std::thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                let request = requests.try_iter().last().unwrap_or(request);
                if request.id != worker_latest.load(Ordering::Acquire) {
                    continue;
                }
                let document = read(&request);
                if request.id != worker_latest.load(Ordering::Acquire) {
                    continue;
                }
                let siblings = if request.scan_directory {
                    document
                        .as_ref()
                        .ok()
                        .map(|loaded| sibling_files(&loaded.path))
                } else {
                    None
                };
                if request.id != worker_latest.load(Ordering::Acquire) {
                    continue;
                }
                if results
                    .send(LoadResult {
                        request,
                        document,
                        siblings,
                    })
                    .is_err()
                {
                    break;
                }
                if let Ok(holder) = holder.lock() {
                    if let Some(ctx) = holder.as_ref() {
                        ctx.request_repaint();
                    }
                }
            }
        });
        Self {
            sender,
            receiver,
            latest,
        }
    }

    pub fn request(&self, mut request: LoadRequest) -> LoadRequest {
        request.id = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.sender.send(request.clone());
        request
    }

    pub fn cancel(&self) {
        self.latest.fetch_add(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn slow_loads_do_not_block_requests_and_only_the_latest_result_is_delivered() {
        let (started_tx, started_rx) = unbounded();
        let (resume_tx, resume_rx) = unbounded();
        let loader = DocumentLoader::spawn(Arc::new(Mutex::new(None)), move |request| {
            if request.path == std::path::Path::new("slow.md") {
                started_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            }
            Err(request.path.to_string_lossy().to_string())
        });
        let request = |path: &str| LoadRequest {
            id: 0,
            path: path.into(),
            reset_view: true,
            revision: 0,
            scan_directory: false,
        };
        loader.request(request("slow.md"));
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        loader.request(request("intermediate.md"));
        let latest = loader.request(request("latest.md"));
        assert!(loader.receiver.is_empty());
        resume_tx.send(()).unwrap();
        let result = loader
            .receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(result.request.id, latest.id);
        assert_eq!(result.document.err().as_deref(), Some("latest.md"));
        assert!(loader.receiver.is_empty());
    }
}
