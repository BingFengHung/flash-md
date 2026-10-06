/// 判斷特定副檔名是否為 PDF 文件
pub fn is_pdf_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("pdf")
}

/// 自 PDF 二進制資料中即時擷取純文字與分頁結構，轉換為 Markdown 格式
pub fn extract_text_from_pdf_bytes(bytes: &[u8]) -> Result<(String, usize), String> {
    let doc = lopdf::Document::load_mem(bytes).map_err(|e| format!("PDF 解析失敗: {}", e))?;
    let page_numbers: Vec<u32> = doc.get_pages().keys().cloned().collect();
    let mut sorted_pages = page_numbers;
    sorted_pages.sort();

    let total_pages = sorted_pages.len();
    if total_pages == 0 {
        return Ok(("（此 PDF 文件為空或無頁面）".to_string(), 0));
    }

    let mut pages_text = Vec::new();
    for &page_num in &sorted_pages {
        let text = doc.extract_text(&[page_num]).unwrap_or_default();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            pages_text.push(format!(
                "### 📄 第 {} / {} 頁\n\n{}\n",
                page_num, total_pages, trimmed
            ));
        }
    }

    if pages_text.is_empty() {
        Ok((
            format!("### 📄 PDF 快速預覽 (共 {} 頁)\n\n> ⚠ 此 PDF 文件的頁面可能為純掃描圖檔或加密內容，未包含可提取的內嵌文字字串。", total_pages),
            total_pages,
        ))
    } else {
        Ok((pages_text.join("\n---\n\n"), total_pages))
    }
}
