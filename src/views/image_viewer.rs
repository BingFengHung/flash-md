use egui::{Rounding, ScrollArea, Vec2};

/// 繪製圖片與 SVG 向量圖檢視畫布 (支援滾輪縮放、平移與自適應視窗)
pub fn render_image_viewer(
    ui: &mut egui::Ui,
    image_bytes: Option<&[u8]>,
    image_uri: Option<&str>,
    format_ext: &str,
    image_zoom: &mut f32,
    image_fit_mode: &mut bool,
    scroll_target: Option<f32>,
) -> Option<(f32, f32)> {
    if let Some(bytes) = image_bytes {
        let available = ui.available_size();

        // 監聽滾輪縮放
        let hovered = ui.input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pos| ui.clip_rect().contains(pos))
        });
        let scroll_delta = if hovered {
            ui.input(|input| input.raw_scroll_delta.y)
        } else {
            0.0_f32
        };
        if scroll_delta != 0.0_f32 {
            if scroll_delta > 0.0_f32 {
                *image_zoom = (*image_zoom * 1.15_f32).min(10.0_f32);
            } else {
                *image_zoom = (*image_zoom / 1.15_f32).max(0.1_f32);
            }
            *image_fit_mode = false;
            ui.input_mut(|input| {
                input.raw_scroll_delta.y = 0.0_f32;
                input.smooth_scroll_delta.y = 0.0_f32;
            });
        }

        let mut scroll = ScrollArea::both()
            .id_salt("image_viewer_scroll_area")
            .auto_shrink([false, false]);
        if let Some(target) = scroll_target {
            scroll = scroll.vertical_scroll_offset(target);
        }

        let output = scroll.show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                let ext = if !format_ext.is_empty() {
                    format_ext
                } else {
                    "png"
                };

                if let Some(image) = crate::textures::cached_image(
                    ui.ctx(),
                    image_uri.unwrap_or("bytes://preview"),
                    bytes,
                    ext,
                ) {
                    let mut img = image.widget().rounding(Rounding::same(6.0_f32));
                    if *image_fit_mode {
                        img = img.max_size(Vec2::new(
                            (available.x - 24.0_f32).max(100.0_f32),
                            (available.y - 24.0_f32).max(100.0_f32),
                        ));
                    } else {
                        img = img.fit_to_original_size(*image_zoom);
                    }
                    ui.add(img);
                } else {
                    ui.label("圖片格式無法解碼");
                }
            });
        });
        Some((
            output.state.offset.y,
            (output.content_size.y - output.inner_rect.height()).max(0.0_f32),
        ))
    } else if let Some(uri) = image_uri {
        let available = ui.available_size();
        let scroll = ScrollArea::both().auto_shrink([false, false]);
        let output = scroll.show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                let img = egui::Image::from_uri(uri.to_string())
                    .rounding(Rounding::same(6.0_f32))
                    .max_size(Vec2::new(
                        (available.x - 24.0_f32).max(100.0_f32),
                        (available.y - 24.0_f32).max(100.0_f32),
                    ));
                ui.add(img);
            });
        });
        Some((
            output.state.offset.y,
            (output.content_size.y - output.inner_rect.height()).max(0.0_f32),
        ))
    } else {
        None
    }
}
