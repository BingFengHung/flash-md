use egui::{
    pos2, vec2, Align2, Color32, FontId, Frame, Margin, Pos2, Rect, RichText, Rounding,
    Sense, Stroke, Vec2,
};
use crate::theme::AppTheme;

#[derive(Debug, Clone)]
pub struct MindmapNode {
    pub id: usize,
    pub title: String,
    pub level: usize,
    pub children: Vec<MindmapNode>,
    pub collapsed: bool,
    pub pos: Pos2,
    pub size: Vec2,
    pub subtree_height: f32,
}

#[derive(Debug, Clone)]
pub struct MindmapState {
    pub pan: Vec2,
    pub zoom: f32,
    pub collapsed_ids: std::collections::HashSet<usize>,
    pub initialized: bool,
}

impl Default for MindmapState {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0_f32,
            collapsed_ids: std::collections::HashSet::new(),
            initialized: false,
        }
    }
}

pub struct MindmapOutput {
    pub jump_to_anchor: Option<String>,
    pub switch_to_markdown: bool,
}

/// 解析 Markdown 內容為心智圖樹狀結構
pub fn parse_markdown_to_mindmap(content: &str, fallback_root_title: &str) -> MindmapNode {
    let mut root = MindmapNode {
        id: 0,
        title: if fallback_root_title.is_empty() {
            "Markdown Document".to_string()
        } else {
            fallback_root_title.to_string()
        },
        level: 0,
        children: Vec::new(),
        collapsed: false,
        pos: Pos2::ZERO,
        size: Vec2::ZERO,
        subtree_height: 0.0_f32,
    };

    let mut next_id = 1;
    let mut stack: Vec<(usize, MindmapNode)> = Vec::new(); // (level, node)

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let mut node_level = 0;
        let mut title = "";

        if line.starts_with('#') {
            let hash_count = line.chars().take_while(|&c| c == '#').count();
            if hash_count > 0 && hash_count <= 6 {
                let rest = line[hash_count..].trim();
                if !rest.is_empty() {
                    node_level = hash_count;
                    title = rest;
                }
            }
        } else if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
            let leading_spaces = raw_line.len() - raw_line.trim_start().len();
            let indent_level = (leading_spaces / 2) + 1;
            let rest = line[2..].trim();
            if !rest.is_empty() {
                node_level = 3 + indent_level; // 清單項層級接在主要標題後
                title = rest;
            }
        }

        if node_level > 0 && !title.is_empty() {
            // 清理 Markdown 連結或標記語法
            let clean_title = clean_markdown_inline(title);
            if clean_title.is_empty() {
                continue;
            }

            // 如果是第一個 H1 且 root 還是預設名稱，則直接升級 root 標題，後續子標題直接作為主分支
            if node_level == 1 && root.children.is_empty() && stack.is_empty() {
                root.title = clean_title;
                continue;
            }

            let new_node = MindmapNode {
                id: next_id,
                title: clean_title,
                level: node_level,
                children: Vec::new(),
                collapsed: false,
                pos: Pos2::ZERO,
                size: Vec2::ZERO,
                subtree_height: 0.0_f32,
            };
            next_id += 1;

            while let Some((last_level, _)) = stack.last() {
                if *last_level >= node_level {
                    let (_, popped_node) = stack.pop().unwrap();
                    if let Some((_, parent)) = stack.last_mut() {
                        parent.children.push(popped_node);
                    } else {
                        root.children.push(popped_node);
                    }
                } else {
                    break;
                }
            }

            stack.push((node_level, new_node));
        }
    }

    // 將 stack 剩餘節點回填
    while let Some((_, popped_node)) = stack.pop() {
        if let Some((_, parent)) = stack.last_mut() {
            parent.children.push(popped_node);
        } else {
            root.children.push(popped_node);
        }
    }

    // 若完全沒有抓到標題與清單，則產生一個提示節點
    if root.children.is_empty() && content.trim().is_empty() {
        root.title = "（空白文檔）".to_string();
    }

    root
}

fn clean_markdown_inline(text: &str) -> String {
    let mut s = text.to_string();
    // 移除粗體/斜體 ** * __ _ ` ~~
    s = s
        .replace("**", "")
        .replace("__", "")
        .replace('*', "")
        .replace('_', "")
        .replace('`', "")
        .replace("~~", "");
    // 移除連結格式 [text](url) -> text
    while let Some(start_bracket) = s.find('[') {
        if let Some(end_bracket) = s[start_bracket..].find(']') {
            let end_bracket = start_bracket + end_bracket;
            if end_bracket + 1 < s.len() && s.as_bytes()[end_bracket + 1] == b'(' {
                if let Some(end_paren) = s[end_bracket..].find(')') {
                    let end_paren = end_bracket + end_paren;
                    let inner_text = s[start_bracket + 1..end_bracket].to_string();
                    s.replace_range(start_bracket..=end_paren, &inner_text);
                    continue;
                }
            }
        }
        break;
    }
    s.trim().to_string()
}

/// 計算心智圖節點佈局尺寸與座標 (水平樹狀佈局 Left-to-Right)
fn layout_mindmap_tree(
    node: &mut MindmapNode,
    font_scale: f32,
    collapsed_set: &std::collections::HashSet<usize>,
) {
    node.collapsed = collapsed_set.contains(&node.id);

    // 根據層級計算節點文字與方塊尺寸
    let char_count = node.title.chars().count();
    let (font_size, pad_h, pad_v) = match node.level {
        0 => (16.0_f32 * font_scale, 18.0_f32, 10.0_f32),
        1 => (13.5_f32 * font_scale, 14.0_f32, 7.0_f32),
        2 => (12.5_f32 * font_scale, 12.0_f32, 6.0_f32),
        _ => (11.5_f32 * font_scale, 10.0_f32, 5.0_f32),
    };

    let approx_char_width = font_size * 0.95_f32;
    let text_width = (char_count as f32 * approx_char_width).clamp(40.0_f32, 380.0_f32);
    let text_height = font_size * 1.35_f32;

    let expand_btn_w = if !node.children.is_empty() { 18.0_f32 } else { 0.0_f32 };
    node.size = vec2(text_width + pad_h * 2.0_f32 + expand_btn_w, text_height + pad_v * 2.0_f32);

    if node.collapsed || node.children.is_empty() {
        node.subtree_height = node.size.y;
        return;
    }

    let mut total_child_h = 0.0_f32;
    let v_spacing = (14.0_f32 * font_scale).max(10.0_f32);

    for (idx, child) in node.children.iter_mut().enumerate() {
        layout_mindmap_tree(child, font_scale, collapsed_set);
        total_child_h += child.subtree_height;
        if idx > 0 {
            total_child_h += v_spacing;
        }
    }

    node.subtree_height = node.size.y.max(total_child_h);
}

/// 定位每個節點的絕對世界座標
fn position_mindmap_tree(
    node: &mut MindmapNode,
    origin: Pos2,
    h_spacing: f32,
    v_spacing: f32,
) {
    node.pos = origin;

    if node.collapsed || node.children.is_empty() {
        return;
    }

    let total_children_h: f32 = node
        .children
        .iter()
        .map(|c| c.subtree_height)
        .sum::<f32>()
        + (node.children.len().saturating_sub(1) as f32) * v_spacing;

    let mut current_y = origin.y - total_children_h / 2.0_f32;
    let child_x = origin.x + node.size.x + h_spacing;

    for child in node.children.iter_mut() {
        let child_center_y = current_y + child.subtree_height / 2.0_f32;
        let child_origin = pos2(child_x, child_center_y);
        position_mindmap_tree(child, child_origin, h_spacing, v_spacing);
        current_y += child.subtree_height + v_spacing;
    }
}

/// 繪製心智圖畫布與節點
pub fn render_mindmap_view(
    ui: &mut egui::Ui,
    theme: AppTheme,
    font_scale: f32,
    root: &mut MindmapNode,
    state: &mut MindmapState,
) -> MindmapOutput {
    let mut output = MindmapOutput {
        jump_to_anchor: None,
        switch_to_markdown: false,
    };

    let available_rect = ui.available_rect_before_wrap();
    let center = available_rect.center();

    // 第一次開啟時自動居中
    if !state.initialized {
        state.pan = Vec2::ZERO;
        state.zoom = 1.0_f32;
        state.initialized = true;
    }

    // 1. 執行心智圖樹狀幾何運算
    layout_mindmap_tree(root, font_scale, &state.collapsed_ids);
    let h_spacing = (60.0_f32 * font_scale).max(40.0_f32);
    let v_spacing = (14.0_f32 * font_scale).max(10.0_f32);
    position_mindmap_tree(root, pos2(0.0_f32, 0.0_f32), h_spacing, v_spacing);

    // 2. 處理畫布拖曳平移 (Pan) 與滾輪縮放 (Zoom)
    let (response, painter) = ui.allocate_painter(available_rect.size(), Sense::click_and_drag());

    // 拖曳平移
    if response.dragged() {
        state.pan += response.drag_delta();
    }

    // 滾輪縮放
    let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
    if scroll_delta != 0.0_f32 && response.hovered() {
        let zoom_factor = if scroll_delta > 0.0_f32 { 1.12_f32 } else { 0.89_f32 };
        state.zoom = (state.zoom * zoom_factor).clamp(0.25_f32, 2.5_f32);
    }

    let world_to_screen = |p: Pos2| -> Pos2 {
        center + state.pan + (p.to_vec2() * state.zoom)
    };

    // 3. 繪製精緻背景網格點陣 (Grid Dots)
    let grid_color = match theme {
        AppTheme::Dark => Color32::from_rgba_unmultiplied(255, 255, 255, 12),
        AppTheme::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 15),
    };
    let grid_step = (32.0_f32 * state.zoom).max(16.0_f32);
    let offset_x = (center.x + state.pan.x) % grid_step;
    let offset_y = (center.y + state.pan.y) % grid_step;

    let mut x = available_rect.min.x + offset_x;
    while x < available_rect.max.x {
        let mut y = available_rect.min.y + offset_y;
        while y < available_rect.max.y {
            painter.circle_filled(pos2(x, y), 1.0_f32, grid_color);
            y += grid_step;
        }
        x += grid_step;
    }

    // 4. 遞迴繪製所有連接曲線與節點卡片
    let mut toggle_collapse_id: Option<usize> = None;
    let mut clicked_anchor_title: Option<String> = None;

    let cull_rect = available_rect.expand(60.0_f32);
    render_mindmap_node_and_edges(
        ui,
        &painter,
        root,
        world_to_screen,
        state.zoom,
        theme,
        font_scale,
        cull_rect,
        &mut toggle_collapse_id,
        &mut clicked_anchor_title,
    );

    if let Some(id) = toggle_collapse_id {
        if state.collapsed_ids.contains(&id) {
            state.collapsed_ids.remove(&id);
        } else {
            state.collapsed_ids.insert(id);
        }
        ui.ctx().request_repaint();
    }

    if let Some(title) = clicked_anchor_title {
        output.jump_to_anchor = Some(title);
        output.switch_to_markdown = true;
    }

    // 5. 繪製右上角懸浮工具列 (Zoom & Center Reset & Exit)
    let toolbar_rect = Rect::from_min_size(
        pos2(available_rect.max.x - 220.0_f32, available_rect.min.y + 14.0_f32),
        vec2(206.0_f32, 34.0_f32),
    );

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(toolbar_rect), |ui| {
        Frame::none()
            .fill(theme.card_bg_color())
            .rounding(Rounding::same(8.0_f32))
            .stroke(Stroke::new(1.0_f32, theme.border_color()))
            .inner_margin(Margin::symmetric(8.0_f32, 4.0_f32))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("➖").small()).on_hover_text("縮小 (滾輪向下)").clicked() {
                        state.zoom = (state.zoom * 0.88_f32).clamp(0.25_f32, 2.5_f32);
                    }

                    let zoom_pct = (state.zoom * 100.0_f32).round() as u32;
                    ui.label(
                        RichText::new(format!("{}%", zoom_pct))
                            .size(11.5_f32)
                            .strong()
                            .color(theme.accent_color()),
                    );

                    if ui.add(egui::Button::new("➕").small()).on_hover_text("放大 (滾輪向上)").clicked() {
                        state.zoom = (state.zoom * 1.14_f32).clamp(0.25_f32, 2.5_f32);
                    }

                    ui.separator();

                    if ui.add(egui::Button::new("🎯").small()).on_hover_text("重置視角居中").clicked() {
                        state.pan = Vec2::ZERO;
                        state.zoom = 1.0_f32;
                    }

                    if ui.add(
                        egui::Button::new(
                            RichText::new("📄 正文")
                                .size(11.5_f32)
                                .color(theme.text_primary()),
                        ).small(),
                    ).on_hover_text("切換回 Markdown 正文閱讀").clicked() {
                        output.switch_to_markdown = true;
                    }
                });
            });
    });

    output
}

/// 遞迴繪製心智圖連線與節點卡片（含視口視錐剔除 Viewport Culling）
fn render_mindmap_node_and_edges<F>(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    node: &MindmapNode,
    world_to_screen: F,
    zoom: f32,
    theme: AppTheme,
    font_scale: f32,
    cull_rect: Rect,
    toggle_collapse_id: &mut Option<usize>,
    clicked_anchor_title: &mut Option<String>,
) where
    F: Fn(Pos2) -> Pos2 + Copy,
{
    let screen_origin = world_to_screen(node.pos);
    let screen_node_rect = Rect::from_min_size(
        pos2(screen_origin.x, screen_origin.y - (node.size.y * zoom) / 2.0_f32),
        node.size * zoom,
    );

    // 1. 繪製連往子節點的平滑貝茲曲線 (Cubic Bezier Curves)
    if !node.collapsed && !node.children.is_empty() {
        let start_pt = pos2(screen_node_rect.max.x, screen_node_rect.center().y);

        for (idx, child) in node.children.iter().enumerate() {
            let child_screen_origin = world_to_screen(child.pos);
            let child_center_y = child_screen_origin.y;
            let end_pt = pos2(child_screen_origin.x, child_center_y);

            // 視口剔除：若曲線範圍不在視口內則跳過繪製
            let curve_rect = Rect::from_two_pos(start_pt, end_pt).expand(30.0_f32);
            if curve_rect.intersects(cull_rect) {
                // 依據子節點索引分配柔和層級色彩
                let line_color = get_branch_color(child.level, idx, theme);
                let ctrl_dist = (end_pt.x - start_pt.x) * 0.5_f32;
                let ctrl1 = pos2(start_pt.x + ctrl_dist, start_pt.y);
                let ctrl2 = pos2(end_pt.x - ctrl_dist, end_pt.y);

                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    [start_pt, ctrl1, ctrl2, end_pt],
                    false,
                    Color32::TRANSPARENT,
                    Stroke::new((1.8_f32 * zoom).clamp(1.0_f32, 2.5_f32), line_color),
                ));
            }

            render_mindmap_node_and_edges(
                ui,
                painter,
                child,
                world_to_screen,
                zoom,
                theme,
                font_scale,
                cull_rect,
                toggle_collapse_id,
                clicked_anchor_title,
            );
        }
    }

    // 視口邊界剔除 (Viewport Frustum Culling)：若節點卡片完全在螢幕外，直接跳過繪製以釋放 GPU 與 Painter 資源
    if !screen_node_rect.intersects(cull_rect) {
        return;
    }

    // 2. 計算節點配色與圓角
    let (bg_color, stroke_color, text_color, font_size, rounding) = match node.level {
        0 => (
            theme.accent_color(),
            Stroke::NONE,
            Color32::WHITE,
            15.0_f32 * font_scale * zoom,
            Rounding::same(10.0_f32 * zoom),
        ),
        1 => {
            let accent_subtle = match theme {
                AppTheme::Dark => Color32::from_rgb(30, 41, 59),
                AppTheme::Light => Color32::from_rgb(238, 246, 255),
            };
            (
                accent_subtle,
                Stroke::new((1.5_f32 * zoom).max(1.0_f32), theme.accent_color()),
                theme.text_primary(),
                13.0_f32 * font_scale * zoom,
                Rounding::same(8.0_f32 * zoom),
            )
        }
        2 => (
            theme.card_bg_color(),
            Stroke::new((1.0_f32 * zoom).max(1.0_f32), theme.border_color()),
            theme.text_primary(),
            12.0_f32 * font_scale * zoom,
            Rounding::same(6.0_f32 * zoom),
        ),
        _ => (
            theme.card_bg_color(),
            Stroke::new(0.8_f32 * zoom, theme.border_color()),
            theme.text_secondary(),
            11.0_f32 * font_scale * zoom,
            Rounding::same(5.0_f32 * zoom),
        ),
    };

    // 3. 繪製節點卡片底框
    let node_resp = ui.interact(
        screen_node_rect,
        ui.make_persistent_id(format!("mindmap_node_{}", node.id)),
        Sense::click(),
    );

    let final_bg = if node_resp.hovered() && node.level > 0 {
        match theme {
            AppTheme::Dark => Color32::from_rgb(51, 65, 85),
            AppTheme::Light => Color32::from_rgb(224, 238, 254),
        }
    } else {
        bg_color
    };

    painter.rect(screen_node_rect, rounding, final_bg, stroke_color);

    if node_resp.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if node_resp.clicked() {
        clicked_anchor_title.get_or_insert_with(|| node.title.clone());
    }

    // 4. 繪製節點文字
    let text_pos = pos2(
        screen_node_rect.min.x + (12.0_f32 * zoom),
        screen_node_rect.center().y,
    );

    painter.text(
        text_pos,
        Align2::LEFT_CENTER,
        &node.title,
        FontId::proportional(font_size.max(7.0_f32)),
        text_color,
    );

    // 5. 若有子節點，繪製展開/收折按鈕徽章 `[+]` / `[-]`
    if !node.children.is_empty() {
        let badge_center = pos2(
            screen_node_rect.max.x - (10.0_f32 * zoom),
            screen_node_rect.center().y,
        );
        let badge_radius = (7.0_f32 * zoom).clamp(4.0_f32, 10.0_f32);
        let badge_rect = Rect::from_center_size(badge_center, vec2(badge_radius * 2.0_f32, badge_radius * 2.0_f32));

        let badge_resp = ui.interact(
            badge_rect,
            ui.make_persistent_id(format!("collapse_btn_{}", node.id)),
            Sense::click(),
        );

        let badge_bg = if node.collapsed {
            theme.accent_color()
        } else {
            theme.border_color()
        };

        painter.circle_filled(badge_center, badge_radius, badge_bg);
        let icon = if node.collapsed { "+" } else { "−" };
        painter.text(
            badge_center,
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional((9.0_f32 * zoom).max(6.0_f32)),
            Color32::WHITE,
        );

        if badge_resp.clicked() {
            *toggle_collapse_id = Some(node.id);
        }
    }
}

/// 依節點層級與順序獲取柔和的分支連線顏色
fn get_branch_color(level: usize, idx: usize, theme: AppTheme) -> Color32 {
    let colors = match theme {
        AppTheme::Dark => [
            Color32::from_rgb(56, 189, 248),   // 青天藍
            Color32::from_rgb(52, 211, 153),   // 翠綠
            Color32::from_rgb(251, 146, 60),   // 珊瑚橘
            Color32::from_rgb(192, 132, 252),  // 薰衣草紫
            Color32::from_rgb(250, 204, 21),   // 琥珀金
            Color32::from_rgb(244, 114, 182),  // 玫瑰粉
        ],
        AppTheme::Light => [
            Color32::from_rgb(2, 132, 199),    // 海軍深藍
            Color32::from_rgb(5, 150, 105),    // 翡翠綠
            Color32::from_rgb(217, 119, 6),    // 暖橘
            Color32::from_rgb(147, 51, 234),   // 紫羅蘭
            Color32::from_rgb(202, 138, 4),    // 典雅金
            Color32::from_rgb(219, 39, 119),   // 洋紅
        ],
    };

    if level <= 1 {
        colors[idx % colors.len()]
    } else {
        match theme {
            AppTheme::Dark => Color32::from_rgba_unmultiplied(148, 163, 184, 140),
            AppTheme::Light => Color32::from_rgba_unmultiplied(100, 116, 139, 150),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_markdown_inline() {
        assert_eq!(clean_markdown_inline("**Bold Text**"), "Bold Text");
        assert_eq!(clean_markdown_inline("*Italic Text*"), "Italic Text");
        assert_eq!(clean_markdown_inline("`Code Block`"), "Code Block");
        assert_eq!(clean_markdown_inline("[Link Title](https://example.com)"), "Link Title");
        assert_eq!(clean_markdown_inline("Normal **Mixed** `Title`"), "Normal Mixed Title");
    }

    #[test]
    fn test_parse_markdown_to_mindmap_hierarchy() {
        let md = r#"
# Root Project
## Subsystem A
### Component 1
### Component 2
## Subsystem B
### Component 3
"#;
        let root = parse_markdown_to_mindmap(md, "Default Title");
        assert_eq!(root.title, "Root Project");
        assert_eq!(root.children.len(), 2);

        // Subsystem A
        let sub_a = &root.children[0];
        assert_eq!(sub_a.title, "Subsystem A");
        assert_eq!(sub_a.children.len(), 2);
        assert_eq!(sub_a.children[0].title, "Component 1");
        assert_eq!(sub_a.children[1].title, "Component 2");

        // Subsystem B
        let sub_b = &root.children[1];
        assert_eq!(sub_b.title, "Subsystem B");
        assert_eq!(sub_b.children.len(), 1);
        assert_eq!(sub_b.children[0].title, "Component 3");
    }

    #[test]
    fn test_parse_markdown_with_fallback_title_and_lists() {
        let md = r#"
## First Heading Without H1
- List item 1
- List item 2
"#;
        let root = parse_markdown_to_mindmap(md, "README.md");
        assert_eq!(root.title, "README.md");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].title, "First Heading Without H1");
        assert_eq!(root.children[0].children.len(), 2);
        assert_eq!(root.children[0].children[0].title, "List item 1");
    }

    #[test]
    fn test_layout_mindmap_tree_subtree_height() {
        let md = "# Title\n## Branch 1\n### Leaf 1\n### Leaf 2\n## Branch 2";
        let mut root = parse_markdown_to_mindmap(md, "Title");
        let collapsed_set = std::collections::HashSet::new();

        layout_mindmap_tree(&mut root, 1.0_f32, &collapsed_set);
        assert!(root.size.x > 0.0_f32);
        assert!(root.size.y > 0.0_f32);
        assert!(root.subtree_height >= root.size.y);

        // 測試收折狀態
        let mut collapsed_test_set = std::collections::HashSet::new();
        collapsed_test_set.insert(root.children[0].id); // 收折 Branch 1
        layout_mindmap_tree(&mut root, 1.0_f32, &collapsed_test_set);
        assert!(root.children[0].collapsed);
        assert_eq!(root.children[0].subtree_height, root.children[0].size.y);
    }
}
