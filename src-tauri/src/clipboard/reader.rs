use std::{num::NonZeroU32, sync::OnceLock, thread, time::Duration};

use anyhow::Context;
use clipboard_win::{formats, raw, Clipboard};
use image::ImageFormat;

use super::capture::Captured;

/// Windows 约定的隐私标记格式：密码管理器等程序用它声明内容不应被记录。
const EXCLUDE_FROM_MONITOR: &str = "ExcludeClipboardContentFromMonitorProcessing";
const CAN_INCLUDE_IN_HISTORY: &str = "CanIncludeInClipboardHistory";
const VIEWER_IGNORE: &str = "Clipboard Viewer Ignore";
/// AnyPaste 自己写回剪贴板时附带的标记，避免把粘贴操作再记录一次。
const OWN_MARKER: &str = "AnyPaste.Internal";

struct Formats {
    html: Option<NonZeroU32>,
    rtf: Option<NonZeroU32>,
    png: Option<NonZeroU32>,
    exclude: Option<NonZeroU32>,
    can_include: Option<NonZeroU32>,
    viewer_ignore: Option<NonZeroU32>,
    own: Option<NonZeroU32>,
}

fn formats() -> &'static Formats {
    static FORMATS: OnceLock<Formats> = OnceLock::new();
    FORMATS.get_or_init(|| Formats {
        html: raw::register_format("HTML Format"),
        rtf: raw::register_format("Rich Text Format"),
        png: raw::register_format("PNG"),
        exclude: raw::register_format(EXCLUDE_FROM_MONITOR),
        can_include: raw::register_format(CAN_INCLUDE_IN_HISTORY),
        viewer_ignore: raw::register_format(VIEWER_IGNORE),
        own: raw::register_format(OWN_MARKER),
    })
}

fn avail(format: Option<NonZeroU32>) -> bool {
    format.is_some_and(|f| raw::is_format_avail(f.get()))
}

/// 读取当前剪贴板。返回 `None` 表示内容被标记为隐私、为空或格式不支持。
/// 优先级：文件列表 > 文本（含 HTML/RTF）> 图片。Office 复制文字时也会附带位图，所以文本优先于图片。
pub fn read() -> anyhow::Result<Option<Captured>> {
    let _guard = open_with_retry()?;
    let f = formats();

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

/// 其他程序可能正占用剪贴板，按 10/20/40/80/160ms 退避重试。
fn open_with_retry() -> anyhow::Result<Clipboard> {
    let mut delay = Duration::from_millis(10);
    for _ in 0..5 {
        if let Ok(clip) = Clipboard::new() {
            return Ok(clip);
        }
        thread::sleep(delay);
        delay *= 2;
    }
    Clipboard::new().map_err(|e| anyhow::anyhow!("剪贴板被占用，无法打开: {e}"))
}
