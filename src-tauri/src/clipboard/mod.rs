mod capture;
#[cfg(windows)]
mod formats;
#[cfg(windows)]
mod reader;
#[cfg(windows)]
mod source;
#[cfg(windows)]
mod writer;

#[cfg(windows)]
pub use monitor::spawn;
#[cfg(windows)]
pub use source::foreground_app;
#[cfg(windows)]
pub use writer::write;

/// 系统剪贴板序列号，内容每变化一次加一。
#[cfg(windows)]
pub fn sequence() -> Option<std::num::NonZeroU32> {
    clipboard_win::raw::seq_num()
}

/// 读写真实的系统剪贴板，会临时覆盖当前内容，默认忽略：
/// `cargo test -- --ignored --test-threads=1 clipboard::system_tests`
#[cfg(all(test, windows))]
mod system_tests {
    use std::fs;

    use clipboard_win::{formats as cf, raw};

    use super::{formats, reader, writer};
    use crate::store::{ClipContent, ClipKind};

    fn content(kind: ClipKind, text: &str) -> ClipContent {
        ClipContent { kind, text: Some(text.into()), html: None, rtf: None, image_file: None }
    }

    /// 测试结束后恢复原有的剪贴板文本。
    fn with_saved_clipboard(test: impl FnOnce()) {
        let saved = clipboard_win::get_clipboard_string().ok();
        test();
        if let Some(text) = saved {
            let _ = clipboard_win::set_clipboard_string(&text);
        }
    }

    #[test]
    #[ignore]
    fn rich_text_round_trip_and_marker_skip() {
        with_saved_clipboard(|| {
            let mut c = content(ClipKind::Rich, "AnyPaste 写回测试");
            c.html = Some("<b>AnyPaste</b>".into());
            c.rtf = Some(r"{\rtf1 AnyPaste}".into());
            writer::write(c, false, std::path::Path::new(".")).unwrap();
            // 带 AnyPaste 标记，监听器读取时应跳过
            assert!(reader::read().unwrap().is_none());

            let f = formats::formats();
            let _guard = formats::open_with_retry().unwrap();
            let mut text = Vec::new();
            raw::get_string(&mut text).unwrap();
            assert_eq!(String::from_utf8(text).unwrap(), "AnyPaste 写回测试");
            let mut html = Vec::new();
            raw::get_html(f.html.unwrap().get(), &mut html).unwrap();
            assert!(String::from_utf8_lossy(&html).contains("<b>AnyPaste</b>"));
            assert!(formats::avail(f.rtf));
        });
    }

    #[test]
    #[ignore]
    fn plain_paste_drops_formatting() {
        with_saved_clipboard(|| {
            let mut c = content(ClipKind::Rich, "plain only");
            c.html = Some("<i>plain only</i>".into());
            writer::write(c, true, std::path::Path::new(".")).unwrap();
            let _guard = formats::open_with_retry().unwrap();
            assert!(raw::is_format_avail(cf::CF_UNICODETEXT));
            assert!(!formats::avail(formats::formats().html));
        });
    }

    #[test]
    #[ignore]
    fn files_round_trip() {
        with_saved_clipboard(|| {
            let dir = tempfile::tempdir().unwrap();
            let a = dir.path().join("a.txt");
            fs::write(&a, "a").unwrap();
            let path = a.display().to_string();
            writer::write(content(ClipKind::Files, &path), false, dir.path()).unwrap();
            let _guard = formats::open_with_retry().unwrap();
            let mut files = Vec::new();
            raw::get_file_list(&mut files).unwrap();
            assert_eq!(files, vec![path]);
        });
    }

    #[test]
    #[ignore]
    fn image_round_trip() {
        with_saved_clipboard(|| {
            let dir = tempfile::tempdir().unwrap();
            let img = image::RgbaImage::from_pixel(64, 32, image::Rgba([10, 120, 200, 255]));
            img.save(dir.path().join("x.png")).unwrap();
            let mut c = content(ClipKind::Image, "64×32");
            c.image_file = Some("x.png".into());
            writer::write(c, false, dir.path()).unwrap();

            let _guard = formats::open_with_retry().unwrap();
            assert!(formats::avail(formats::formats().png));
            let mut bmp = Vec::new();
            raw::get_bitmap(&mut bmp).unwrap();
            let back = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).unwrap();
            assert_eq!((back.width(), back.height()), (64, 32));
        });
    }
}

#[cfg(windows)]
mod monitor {
    use std::{thread, time::Duration};

    use clipboard_win::{raw, Monitor};
    use tauri::{AppHandle, Manager};

    use super::{capture, reader, source};
    use crate::{
        events, janitor,
        paths::AppPaths,
        settings::SettingsState,
        state::AppState,
        store::{now_ms, Store},
    };

    /// OLE 程序（Office、.NET 等）一次复制会先写入数据再 flush，触发两次通知；等待这段时间合并为一次处理。
    const COALESCE: Duration = Duration::from_millis(80);

    /// 在独立线程上监听 `WM_CLIPBOARDUPDATE`；消息窗口必须在创建它的线程上收消息。
    pub fn spawn(app: AppHandle) {
        let spawned = thread::Builder::new()
            .name("clipboard-monitor".into())
            .spawn(move || {
                let mut monitor = match Monitor::new() {
                    Ok(m) => m,
                    Err(e) => {
                        log::error!("无法启动剪贴板监听: {e}");
                        return;
                    }
                };
                log::info!("剪贴板监听已启动");
                let mut last_seq = None;
                loop {
                    match monitor.recv() {
                        Ok(true) => {
                            thread::sleep(COALESCE);
                            while matches!(monitor.try_recv(), Ok(true)) {}
                            // 合并后剪贴板序列号仍未变化（重复通知），无需再读
                            let seq = raw::seq_num();
                            if seq.is_some() && seq == last_seq {
                                continue;
                            }
                            last_seq = seq;
                            if let Err(e) = on_update(&app) {
                                log::warn!("记录剪贴板失败: {e:#}");
                            }
                        }
                        Ok(false) => break,
                        Err(e) => log::warn!("剪贴板消息异常: {e}"),
                    }
                }
            });
        if let Err(e) = spawned {
            log::error!("无法创建剪贴板监听线程: {e}");
        }
    }

    fn on_update(app: &AppHandle) -> anyhow::Result<()> {
        let state = app.state::<AppState>();
        // 永久复制是用户的明确操作，暂停记录时也照常保存
        if state.is_paused() && !state.permanent_armed() {
            return Ok(());
        }
        // 在读取前取前台进程，尽量贴近触发复制的程序
        let source_app = source::foreground_app();
        let prefs = app.state::<SettingsState>().get().clone();
        let armed = state.permanent_armed();
        // 忽略列表与内容类型只约束自动记录；永久复制是明确操作，不受限制
        if !armed && source_app.as_deref().is_some_and(|p| prefs.is_ignored(p)) {
            return Ok(());
        }
        let Some(captured) = reader::read()? else {
            return Ok(());
        };
        let captured = if armed {
            captured
        } else {
            match capture::KindFilter::from(&prefs).apply(captured) {
                Some(c) => c,
                None => return Ok(()),
            }
        };

        let store = app.state::<Store>();
        let paths = app.state::<AppPaths>();
        let settings = prefs.capture();
        let Some(mut clip) = capture::prepare(captured, source_app, &paths.images_dir, settings.max_item_bytes)?
        else {
            log::info!("内容超过 {} 字节上限，未记录", settings.max_item_bytes);
            return Ok(());
        };
        // 只有真正得到内容时才消费永久标记，永久复制线程据此判断是否成功
        clip.permanent = state.take_permanent_armed();

        let now = now_ms();
        let result = store.upsert_clip(&clip, now, settings.ttl_ms)?;
        // 不记录内容本身，只记录类型与大小
        log::info!(
            "{} {}{} 条目 #{}（{} 字节，来自 {}）",
            if result.inserted { "新增" } else { "置顶" },
            if clip.permanent { "永久 " } else { "" },
            clip.kind.as_str(),
            result.id,
            clip.size_bytes,
            clip.source_app.as_deref().unwrap_or("未知程序"),
        );
        if result.inserted {
            janitor::cleanup(&store, &paths, &settings, now)?;
        }
        events::clips_changed(app);
        Ok(())
    }
}
