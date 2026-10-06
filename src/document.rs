use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Text,
    Svg,
    Image,
    Pdf,
    Archive,
}

impl DocumentKind {
    pub fn can_save(self) -> bool {
        matches!(self, Self::Text | Self::Svg)
    }
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    Open(PathBuf),
    Clear,
    Close,
    Exit,
    Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

/// Validate the source independently of the currently selected view mode.
pub fn save_document(path: &Path, kind: DocumentKind, content: &str) -> io::Result<()> {
    if !kind.can_save() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "此預覽不支援寫回原始檔案",
        ));
    }
    if !path.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "原始檔案不存在"));
    }
    if fs::metadata(path)?.permissions().readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "原始檔案為唯讀",
        ));
    }
    atomic_write(path, content.as_bytes())
}

/// Write beside the destination, flush, then replace it. Failed writes leave
/// the original intact and remove the temporary file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "無效檔案路徑"))?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(
        ".flash-md-{}-{}.tmp",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let temp = parent.join(temp_name);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.is_file() && metadata.permissions().readonly() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "原始檔案為唯讀",
                ));
            }
            if metadata.is_file() {
                fs::set_permissions(&temp, metadata.permissions())?;
            }
        }
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_and_derived_previews_never_overwrite_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        for kind in [
            DocumentKind::Image,
            DocumentKind::Pdf,
            DocumentKind::Archive,
        ] {
            let path = dir.path().join(format!("{kind:?}"));
            let original = b"\x89PNG\r\n%PDF-1.7\x00\xff";
            fs::write(&path, original).unwrap();
            assert!(save_document(&path, kind, "").is_err());
            assert!(save_document(&path, kind, "# extracted text").is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }

    #[test]
    fn text_and_svg_save_atomically_without_leaving_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        for kind in [DocumentKind::Text, DocumentKind::Svg] {
            let path = dir.path().join(format!("{kind:?}"));
            fs::write(&path, "original").unwrap();
            save_document(&path, kind, "繁體中文\r\nnew content").unwrap();
            assert_eq!(fs::read_to_string(path).unwrap(), "繁體中文\r\nnew content");
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn failed_replace_preserves_original_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("directory");
        fs::create_dir(&destination).unwrap();
        assert!(atomic_write(&destination, b"new").is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
