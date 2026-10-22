use std::num::NonZeroU32;

use anyhow::Context;
use clipboard_win::{formats, raw};
use image::ImageFormat;

use super::{
    capture::Captured,
    formats::{avail, formats as registered, open_with_retry, Formats},
};

/// 读取当前剪贴板。返回 `None` 表示内容被标记为隐私、为空或格式不支持。
/// 优先级：文件列表 > 文本（含 HTML/RTF）> 图片。Office 复制文字时也会附带位图，所以文本优先于图片。
pub fn read() -> anyhow::Result<Option<Captured>> {
    let _guard = open_with_retry()?;
    let f = registered();

    if is_private(f) {
        return Ok(None);
    }

    if raw::is_format_avail(formats::CF_HDROP) {
        let mut files = Vec::new();
        raw::get_file_list(&mut files).context("读取文件列表失败")?;
        return Ok((!files.is_empty()).then_some(Captured::Files(files)));
    }

    if raw::is_format_avail(formats::CF_UNICODETEXT) {
        let mut buf = Vec::new();
        raw::get_string(&mut buf).context("读取文本失败")?;
        let text = String::from_utf8_lossy(&buf).into_owned();
        if !text.trim().is_empty() {
            return Ok(Some(Captured::Text {
                html: read_html(f.html),
                rtf: read_bytes(f.rtf).map(|b| String::from_utf8_lossy(trim_nul(&b)).into_owned()),
                text,
            }));
        }
    }

    if let Some(bytes) = read_bytes(f.png) {
        return Ok(Some(Captured::Image { bytes, format: ImageFormat::Png }));
    }
    if raw::is_format_avail(formats::CF_DIB) || raw::is_format_avail(formats::CF_BITMAP) {
        let mut bytes = Vec::new();
        raw::get_bitmap(&mut bytes).context("读取位图失败")?;
        return Ok(Some(Captured::Image { bytes, format: ImageFormat::Bmp }));
    }

    Ok(None)
}

fn is_private(f: &Formats) -> bool {
    if avail(f.exclude) || avail(f.viewer_ignore) || avail(f.own) {
        return true;
    }
    // CanIncludeInClipboardHistory 为 DWORD，值为 0 时表示不允许进入历史
    read_bytes(f.can_include).is_some_and(|b| b.len() >= 4 && b[..4] == [0, 0, 0, 0])
}

fn read_bytes(format: Option<NonZeroU32>) -> Option<Vec<u8>> {
    let format = format.filter(|f| raw::is_format_avail(f.get()))?;
    let mut buf = Vec::new();
    raw::get_vec(format.get(), &mut buf).ok()?;
    Some(buf)
}

fn read_html(format: Option<NonZeroU32>) -> Option<String> {
    let format = format.filter(|f| raw::is_format_avail(f.get()))?;
    let mut buf = Vec::new();
    raw::get_html(format.get(), &mut buf).ok()?;
    let html = String::from_utf8_lossy(trim_nul(&buf)).into_owned();
    (!html.trim().is_empty()).then_some(html)
}

fn trim_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &bytes[..end]
}