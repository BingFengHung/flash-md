use egui::{Context, TextureHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone)]
pub enum CachedImage {
    Raster(TextureHandle),
    Svg(String),
}

impl CachedImage {
    pub fn widget(&self) -> egui::Image<'static> {
        match self {
            Self::Raster(texture) => egui::Image::from_texture(texture),
            Self::Svg(uri) => egui::Image::from_uri(uri.clone()),
        }
    }
}

#[derive(Clone, Default)]
struct TextureCache(HashMap<String, CachedImage>);

pub fn cached_image(
    ctx: &Context,
    key: &str,
    bytes: &[u8],
    extension: &str,
) -> Option<CachedImage> {
    let id = egui::Id::new("flash-md-textures");
    if let Some(image) = ctx.data(|data| {
        data.get_temp::<TextureCache>(id)
            .and_then(|cache| cache.0.get(key).cloned())
    }) {
        return Some(image);
    }
    let image = if extension.eq_ignore_ascii_case("svg") {
        ctx.include_bytes(key.to_string(), bytes.to_vec());
        CachedImage::Svg(key.to_string())
    } else {
        let decoded = image::load_from_memory(bytes).ok()?;
        let size = [decoded.width() as usize, decoded.height() as usize];
        let rgba = decoded.to_rgba8();
        let pixels = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        CachedImage::Raster(ctx.load_texture(key, pixels, egui::TextureOptions::LINEAR))
    };
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<TextureCache>(id);
        if cache.0.len() >= 32 {
            cache.0.clear();
        }
        cache.0.insert(key.to_string(), image.clone());
    });
    Some(image)
}

#[derive(Clone)]
struct LocalImage {
    path: PathBuf,
    stamp: (u64, Option<SystemTime>),
    image: CachedImage,
}

#[derive(Clone, Default)]
struct LocalCache(HashMap<String, LocalImage>);

pub fn local_image(
    ctx: &Context,
    key: &str,
    resolve: impl FnOnce() -> Option<(PathBuf, Vec<u8>, &'static str)>,
) -> Option<CachedImage> {
    let id = egui::Id::new("flash-md-local-images");
    if let Some(cached) = ctx.data(|data| {
        data.get_temp::<LocalCache>(id)
            .and_then(|cache| cache.0.get(key).cloned())
    }) {
        if let Ok(metadata) = std::fs::metadata(&cached.path) {
            if (metadata.len(), metadata.modified().ok()) == cached.stamp {
                return Some(cached.image);
            }
        }
    }
    let (path, bytes, extension) = resolve()?;
    let metadata = std::fs::metadata(&path).ok()?;
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hash);
    bytes.hash(&mut hash);
    let uri = format!("bytes://local_{:x}.{}", hash.finish(), extension);
    let image = cached_image(ctx, &uri, &bytes, extension)?;
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<LocalCache>(id);
        if cache.0.len() >= 32 {
            cache.0.clear();
        }
        cache.0.insert(
            key.to_string(),
            LocalImage {
                path,
                stamp: (metadata.len(), metadata.modified().ok()),
                image: image.clone(),
            },
        );
    });
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_image_requests_reuse_the_same_texture() {
        let ctx = Context::default();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let first = cached_image(&ctx, "test-image", bytes.get_ref(), "png").unwrap();
        // Cached access succeeds even without any bytes to decode again.
        let second = cached_image(&ctx, "test-image", &[], "png").unwrap();
        let (CachedImage::Raster(first), CachedImage::Raster(second)) = (first, second) else {
            panic!("expected raster");
        };
        assert_eq!(first.id(), second.id());
    }
}
