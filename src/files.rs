use crate::document::DocumentKind;
use crate::markdown::{is_image_extension, is_pdf_extension};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct LoadedDocument {
    pub path: PathBuf,
    pub watch_path: PathBuf,
    pub kind: DocumentKind,
    pub extension: String,
    pub content: String,
    pub image_bytes: Option<Vec<u8>>,
    pub image_pixels: Option<egui::ColorImage>,
    pub size: String,
    pub modified: String,
}

pub fn format_file_size(len: u64) -> String {
    if len < 1024 {
        format!("{} B", len)
    } else if len < 1024 * 1024 {
        format!("{:.1} KB", len as f64 / 1024.0)
    } else {
        format!("{:.2} MB", len as f64 / (1024.0 * 1024.0))
    }
}

/// Read everything into a new value before replacing any active UI state.
pub fn load_document(path: &Path) -> Result<LoadedDocument, String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let path = absolute.as_path();
    let (bytes, watch_path, archive) = match fs::read(path) {
        Ok(bytes) => (bytes, path.to_path_buf(), false),
        Err(original_error) => {
            let Some((zip_path, entry)) = zip_entry_path(path) else {
                return Err(original_error.to_string());
            };
            let bytes = read_bytes_from_zip(&zip_path, &entry)?;
            (bytes, zip_path, true)
        }
    };
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let size = format_file_size(bytes.len() as u64);
    let modified = if archive {
        "ZIP 壓縮檔".to_string()
    } else {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|time| {
                let datetime: chrono::DateTime<chrono::Local> = time.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            })
            .unwrap_or_default()
    };
    let image_pixels =
        if is_image_extension(&extension) && !matches!(extension.as_str(), "svg" | "gif") {
            image::load_from_memory(&bytes).ok().map(|decoded| {
                let rgba = decoded.to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied(
                    [rgba.width() as usize, rgba.height() as usize],
                    rgba.as_raw(),
                )
            })
        } else {
            None
        };
    let (kind, content, image_bytes) = if is_image_extension(&extension) {
        let kind = if extension == "svg" {
            DocumentKind::Svg
        } else {
            DocumentKind::Image
        };
        let content = if kind == DocumentKind::Svg {
            String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        (kind, content, Some(bytes))
    } else if is_pdf_extension(&extension) {
        let (text, _) = crate::markdown::extract_text_from_pdf_bytes(&bytes)?;
        (DocumentKind::Pdf, text, None)
    } else {
        (
            DocumentKind::Text,
            String::from_utf8(bytes).map_err(|e| format!("文字檔案不是有效的 UTF-8：{}", e))?,
            None,
        )
    };
    Ok(LoadedDocument {
        path: path.to_path_buf(),
        watch_path,
        kind: if archive { DocumentKind::Archive } else { kind },
        extension,
        content,
        image_bytes,
        image_pixels,
        size,
        modified,
    })
}

pub fn zip_entry_path(path: &Path) -> Option<(PathBuf, String)> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    lower.match_indices(".zip/").find_map(|(index, _)| {
        let archive = PathBuf::from(&normalized[..index + 4]);
        let entry = &normalized[index + 5..];
        (archive.is_file() && !entry.is_empty() && !entry.ends_with('/'))
            .then(|| (archive, entry.to_string()))
    })
}

pub fn is_preview_target(path: &Path) -> bool {
    path.is_file() || (!path.exists() && zip_entry_path(path).is_some())
}

fn read_bytes_from_zip(zip_path: &Path, entry_name: &str) -> Result<Vec<u8>, String> {
    // Literal PowerShell strings: quotes are escaped; backslashes are literal.
    let zip = zip_path.to_string_lossy().replace('\'', "''");
    let entry = entry_name.replace('\\', "/").replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop';
Add-Type -AssemblyName System.IO.Compression.FileSystem;
$z = [System.IO.Compression.ZipFile]::OpenRead('{}');
try {{
    $e = $z.Entries | Where-Object {{ $_.FullName.Replace('\','/') -eq '{}' }} | Select-Object -First 1;
    if (-not $e) {{ throw 'ZIP entry not found' }};
    $s = $e.Open(); $ms = New-Object System.IO.MemoryStream;
    try {{ $s.CopyTo($ms); $data = $ms.ToArray(); [Console]::OpenStandardOutput().Write($data, 0, $data.Length); }}
    finally {{ $ms.Dispose(); $s.Dispose(); }}
}} finally {{ $z.Dispose(); }}"#,
        zip, entry
    );
    let mut command = Command::new("powershell");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn sibling_files(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| !n.starts_with('.') && !n.starts_with("~$"))
        })
        .collect();
    files.sort_by_cached_key(|path| file_name_sort_key(path));
    files
}

pub fn file_name_sort_key(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_targets_skip_directories_and_find_archives_below_zip_named_folders() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("download.zip");
        fs::create_dir(&folder).unwrap();
        let archive = folder.join("archive.ZIP");
        fs::write(&archive, b"archive fixture").unwrap();
        let virtual_file = archive.join("中文 space.md");
        assert_eq!(
            zip_entry_path(&virtual_file),
            Some((archive.clone(), "中文 space.md".to_string()))
        );
        assert!(is_preview_target(&archive));
        assert!(is_preview_target(&virtual_file));
        assert!(!is_preview_target(&folder));
        assert!(!is_preview_target(
            &folder.join("missing.zip").join("file.md")
        ));
        assert!(!is_preview_target(&archive.join("folder/")));
    }

    #[test]
    fn supported_bitmap_formats_decode_and_gif_keeps_its_encoded_animation() {
        let directory = tempfile::tempdir().unwrap();
        for (extension, format) in [
            ("png", image::ImageFormat::Png),
            ("jpg", image::ImageFormat::Jpeg),
            ("webp", image::ImageFormat::WebP),
            ("gif", image::ImageFormat::Gif),
            ("bmp", image::ImageFormat::Bmp),
            ("ico", image::ImageFormat::Ico),
            ("tiff", image::ImageFormat::Tiff),
        ] {
            let mut encoded = std::io::Cursor::new(Vec::new());
            let image = if extension == "jpg" {
                image::DynamicImage::new_rgb8(3, 3)
            } else {
                image::DynamicImage::new_rgba8(3, 3)
            };
            image.write_to(&mut encoded, format).unwrap();
            let _ = image::load_from_memory(encoded.get_ref())
                .unwrap_or_else(|error| panic!("{extension}: {error}"));
            let path = directory.path().join(format!("fixture.{extension}"));
            fs::write(&path, encoded.get_ref()).unwrap();
            let loaded = load_document(&path).unwrap();
            assert_eq!(loaded.kind, DocumentKind::Image, "{extension}");
            assert_eq!(
                loaded.image_pixels.is_some(),
                extension != "gif",
                "{extension}"
            );
            assert_eq!(
                loaded.image_bytes.as_deref(),
                Some(encoded.get_ref().as_slice())
            );
        }
    }

    #[test]
    fn failing_load_does_not_return_partial_document() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_document(&dir.path().join("missing.md")).is_err());
        let binary = dir.path().join("invalid.txt");
        fs::write(&binary, [0xff, 0xfe]).unwrap();
        assert!(load_document(&binary).is_err());
    }

    #[test]
    fn disk_files_in_zip_named_directories_are_read_normally() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("download.zip");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("README.md");
        fs::write(&path, "# hello").unwrap();
        let loaded = load_document(&path).unwrap();
        assert_eq!(loaded.kind, DocumentKind::Text);
        assert_eq!(loaded.content, "# hello");
    }

    #[test]
    fn bitmap_loading_keeps_preview_text_empty_and_source_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");
        fs::write(&path, [137, 80, 78, 71]).unwrap();
        let loaded = load_document(&path).unwrap();
        assert_eq!(loaded.kind, DocumentKind::Image);
        assert!(!loaded.kind.can_save());
        assert!(loaded.content.is_empty());
        assert!(loaded.image_bytes.is_some());
    }
}
