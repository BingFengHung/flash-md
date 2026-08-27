use egui::{Rounding, ScrollArea, Vec2};

/// 繪製圖片與 SVG 向量圖檢視畫布 (支援滾輪縮放、平移與自適應視窗)
pub fn render_image_viewer(
    ui: &mut egui::Ui,
    image_bytes: Option<&[u8]>,
    image_uri: Option<&str>,
    format_ext: &str,
    image_zoom: &mut f32,
    image_fit_mode: &mut bool,
    reset_scroll_to_top: bool,
    keyboard_scroll_delta: f32,
) {
    if let Some(bytes) = image_bytes {
        let available = ui.available_size();

        // 監聽滾輪縮放
        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0_f32 {
            if scroll_delta > 0.0_f32 {
                *image_zoom = (*image_zoom * 1.15_f32).min(10.0_f32);
            } else {
                *image_zoom = (*image_zoom / 1.15_f32).max(0.1_f32);
            }
            *image_fit_mode = false;
        }

        let mut scroll = ScrollArea::both().auto_shrink([false, false]);
        if reset_scroll_to_top {
            scroll = scroll.scroll_offset(Vec2::ZERO);
        }
        scroll.show(ui, |ui| {
            if keyboard_scroll_delta != 0.0_f32 {
                ui.scroll_with_delta(Vec2::new(0.0_f32, keyboard_scroll_delta));
            }
            ui.centered_and_justified(|ui| {
                let ext = if !format_ext.is_empty() {
                    format_ext
                } else {
                    "png"
                };

                if ext.eq_ignore_ascii_case("svg") {
                    let uri = format!("bytes://viewer_image_preview.{}", ext);
                    let mut img = egui::Image::from_bytes(uri, bytes.to_vec())
                        .rounding(Rounding::same(6.0_f32));

                    if *image_fit_mode {
                        let max_w = (available.x - 24.0_f32).max(100.0_f32);
                        let max_h = (available.y - 24.0_f32).max(100.0_f32);
                        img = img.max_size(Vec2::new(max_w, max_h));
                    } else {
                        img = img.fit_to_original_size(*image_zoom);
                    }

                    ui.add(img);
                } else if let Ok(dyn_img) = image::load_from_memory(bytes) {
                    let size = [dyn_img.width() as usize, dyn_img.height() as usize];
                    let rgba = dyn_img.to_rgba8().into_raw();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    let texture = ui.ctx().load_texture(
                        "viewer_image_texture",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    let mut img = egui::Image::from_texture(&texture)
                        .rounding(Rounding::same(6.0_f32));

                    if *image_fit_mode {
                        let max_w = (available.x - 24.0_f32).max(100.0_f32);
                        let max_h = (available.y - 24.0_f32).max(100.0_f32);
                        img = img.max_size(Vec2::new(max_w, max_h));
                    } else {
                        img = img.fit_to_original_size(*image_zoom);
                    }

                    ui.add(img);
                } else {
                    let uri = format!("bytes://viewer_image_preview.{}", ext);
                    let mut img = egui::Image::from_bytes(uri, bytes.to_vec())
                        .rounding(Rounding::same(6.0_f32));

                    if *image_fit_mode {
                        let max_w = (available.x - 24.0_f32).max(100.0_f32);
                        let max_h = (available.y - 24.0_f32).max(100.0_f32);
                        img = img.max_size(Vec2::new(max_w, max_h));
                    } else {
                        img = img.fit_to_original_size(*image_zoom);
                    }

                    ui.add(img);
                }
            });
        });
    } else if let Some(uri) = image_uri {
        let available = ui.available_size();
        let scroll = ScrollArea::both().auto_shrink([false, false]);
        scroll.show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                let img = egui::Image::from_uri(uri.to_string())
                    .rounding(Rounding::same(6.0_f32))
                    .max_size(Vec2::new((available.x - 24.0_f32).max(100.0_f32), (available.y - 24.0_f32).max(100.0_f32)));
                ui.add(img);
            });
        });
    }
}
