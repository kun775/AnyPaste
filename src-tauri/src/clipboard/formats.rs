use std::{num::NonZeroU32, sync::OnceLock, thread, time::Duration};

use clipboard_win::{raw, Clipboard};

/// 运行时注册的剪贴板格式 ID。
pub struct Formats {
    pub html: Option<NonZeroU32>,
    pub rtf: Option<NonZeroU32>,
    pub png: Option<NonZeroU32>,
    /// Windows 约定的隐私标记：密码管理器等程序用它声明内容不应被记录。
    pub exclude: Option<NonZeroU32>,
    pub can_include: Option<NonZeroU32>,
    pub viewer_ignore: Option<NonZeroU32>,
    /// AnyPaste 写回剪贴板时附带的标记，监听器据此跳过自身写入。
    pub own: Option<NonZeroU32>,
}

pub fn formats() -> &'static Formats {
    static FORMATS: OnceLock<Formats> = OnceLock::new();
    FORMATS.get_or_init(|| Formats {
        html: raw::register_format("HTML Format"),
        rtf: raw::register_format("Rich Text Format"),
        png: raw::register_format("PNG"),
        exclude: raw::register_format("ExcludeClipboardContentFromMonitorProcessing"),
        can_include: raw::register_format("CanIncludeInClipboardHistory"),
        viewer_ignore: raw::register_format("Clipboard Viewer Ignore"),
        own: raw::register_format("AnyPaste.Internal"),
    })
}

pub fn avail(format: Option<NonZeroU32>) -> bool {
    format.is_some_and(|f| raw::is_format_avail(f.get()))
}

/// 其他程序可能正占用剪贴板，按 10/20/40/80/160ms 退避重试。
pub fn open_with_retry() -> anyhow::Result<Clipboard> {
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
