use std::{num::NonZeroU32, path::Path};

use anyhow::Context;
use clipboard_win::{formats, raw};
use image::ImageFormat;

use super::{
    capture::{is_image_path, Captured},
    formats::{avail, formats as registered, open_with_retry, Formats},
};

/// 截图工具（如 PixPin）导出临时图片文件时还可能使用这些扩展名，实际像素由剪贴板位图/PNG 读取。
const EXTRA_IMAGE_EXTS: &[&str] = &["avif", "tif", "tiff"];

/// 读取当前剪贴板。返回 `None` 表示内容被标记为隐私、为空或格式不支持。
/// 优先级：文件列表 > 文本（含 HTML/RTF）> 图片。
/// 特例：PixPin 等截图工具复制图片时会同时写入图片数据与单个临时图片文件（`CF_HDROP`），
/// 此时优先按图片记录；资源管理器复制图片文件不会附带图片数据，仍按文件记录。
/// Office 复制文字时也会附带位图，所以文本仍优先于图片。
pub fn read() -> anyhow::Result<Option<Captured>> {
    let _guard = open_with_retry()?;
    let f = registered();

    if is_private(f) {
        return Ok(None);
    }

    if raw::is_format_avail(formats::CF_HDROP) {
        let mut files = Vec::new();
        raw::get_file_list(&mut files).context("读取文件列表失败")?;
        if !files.is_empty() {
            if is_transient_clipboard_files(&files) {
                return Ok(None);
            }
            if is_single_image_file(&files) {
                match read_image(f) {
                    Ok(Some(image)) => return Ok(Some(image)),
                    Ok(None) => {}
                    Err(e) => log::warn!("剪贴板同时包含图片与文件，读取图片失败，回退按文件处理: {e:#}"),
                }
            }
            return Ok(Some(Captured::Files(files)));
        }
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

    read_image(f)
}

fn read_image(f: &Formats) -> anyhow::Result<Option<Captured>> {
    for fmt in [f.png, f.image_png] {
        if let Some(bytes) = read_bytes(fmt) {
            if !bytes.is_empty() {
                return Ok(Some(Captured::Image { bytes, format: ImageFormat::Png }));
            }
        }
    }
    if raw::is_format_avail(formats::CF_DIB)
        || raw::is_format_avail(formats::CF_DIBV5)
        || raw::is_format_avail(formats::CF_BITMAP)
    {
        let mut bytes = Vec::new();
        raw::get_bitmap(&mut bytes).context("读取位图失败")?;
        return Ok(Some(Captured::Image { bytes, format: ImageFormat::Bmp }));
    }
    Ok(None)
}

/// 微信输入法后台（`wetype_server.exe`）会在截图或复制图片约 1 秒后将位图转存至
/// `WeType\ClipboardTmp\<timestamp>_d.png` 并把剪贴板改写为仅含该路径的 `CF_HDROP`；
/// 该目录只保留最新缓存，旧文件很快会被删除，跳过此类内部缓存回写以避免产生重复且失效的文件记录。
fn is_transient_clipboard_files(files: &[String]) -> bool {
    !files.is_empty() && files.iter().all(|f| is_wetype_clipboard_tmp(Path::new(f)))
}

fn is_wetype_clipboard_tmp(path: &Path) -> bool {
    let mut prev_wetype = false;
    for comp in path.parent().into_iter().flat_map(Path::components) {
        let s = comp.as_os_str().to_str();
        if prev_wetype && s.is_some_and(|c| c.eq_ignore_ascii_case("ClipboardTmp")) {
            return true;
        }
        prev_wetype = s.is_some_and(|c| c.eq_ignore_ascii_case("WeType"));
    }
    false
}

fn is_single_image_file(files: &[String]) -> bool {
    let [single] = files else {
        return false;
    };
    let path = Path::new(single);
    is_image_path(path)
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| EXTRA_IMAGE_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_image_file_detection() {
        assert!(is_single_image_file(&["C:\\Temp\\PixPin_2026-11-23.png".into()]));
        assert!(is_single_image_file(&["C:\\Temp\\shot.JPG".into()]));
        assert!(is_single_image_file(&["C:\\Temp\\shot.avif".into()]));
        assert!(!is_single_image_file(&[]));
        assert!(!is_single_image_file(&["C:\\Temp\\a.txt".into()]));
        assert!(!is_single_image_file(&["C:\\Temp\\a.png".into(), "C:\\Temp\\b.png".into()]));
    }

    #[test]
    fn transient_wetype_clipboard_files_detection() {
        let wetype = "C:\\Users\\u\\AppData\\LocalLow\\Tencent\\WeType\\ClipboardTmp\\1795569871982_d.png";
        assert!(is_transient_clipboard_files(&[wetype.into()]));
        assert!(is_transient_clipboard_files(&[
            "D:/data/wetype/clipboardtmp/sub/1795569871982_d.png".into()
        ]));
        assert!(!is_transient_clipboard_files(&[]));
        assert!(!is_transient_clipboard_files(&["C:\\Temp\\PixPin_2026-11-23.png".into()]));
        assert!(!is_transient_clipboard_files(&["C:\\Users\\u\\Desktop\\1795569871982_d.png".into()]));
        assert!(!is_transient_clipboard_files(&[
            wetype.into(),
            "C:\\Users\\u\\Desktop\\normal.png".into(),
        ]));
    }
}