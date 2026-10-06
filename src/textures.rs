use egui::{Context, TextureHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone)]
pub enum CachedImage {
    Raster(TextureHandle),
    Encoded(String),
}

impl CachedImage {
    pub fn widget(&self) -> egui::Image<'static> {
        match self {
            Self::Raster(texture) => egui::Image::from_texture(texture),
            Self::Encoded(uri) => egui::Image::from_uri(uri.clone()),
        }
    }
}

#[derive(Clone, Default)]
struct TextureCache(HashMap<String, CachedImage>);

pub fn store_raster(ctx: &Context, key: &str, pixels: egui::ColorImage) {
    let image = CachedImage::Raster(ctx.load_texture(key, pixels, egui::TextureOptions::LINEAR));
    ctx.data_mut(|data| {
        let cache =
            data.get_temp_mut_or_default::<TextureCache>(egui::Id::new("flash-md-textures"));
        if cache.0.len() >= 32 {
            cache.0.clear();
        }
        cache.0.insert(key.to_string(), image);
    });
}

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
    let image = if extension.eq_ignore_ascii_case("svg") || extension.eq_ignore_ascii_case("gif") {
        ctx.include_bytes(key.to_string(), bytes.to_vec());
        CachedImage::Encoded(key.to_string())
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
    fn animated_gif_uploads_different_frames_and_schedules_repaints() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .unwrap();
            for color in [image::Rgba([255, 0, 0, 255]), image::Rgba([0, 0, 255, 255])] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(2, 2, color),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
            }
        }
        let ctx = Context::default();
        egui_extras::install_image_loaders(&ctx);
        let cached = cached_image(&ctx, "bytes://animation.gif", &bytes, "gif").unwrap();
        assert!(matches!(cached, CachedImage::Encoded(_)));
        for (time, expected) in [
            (0.0_f64, egui::Color32::RED),
            (0.15_f64, egui::Color32::BLUE),
        ] {
            let output = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add(cached.widget());
                    });
                },
            );
            assert!(
                output
                    .textures_delta
                    .set
                    .iter()
                    .any(|(_, delta)| match &delta.image {
                        egui::ImageData::Color(pixels) =>
                            pixels.size == [2, 2]
                                && pixels.pixels.iter().all(|color| *color == expected),
                        _ => false,
                    }),
                "GIF did not upload the expected frame at {time}"
            );
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                    < std::time::Duration::from_secs(1)
            );
        }
    }

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
