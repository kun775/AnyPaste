mod capture;
#[cfg(windows)]
mod reader;
#[cfg(windows)]
mod source;

#[cfg(windows)]
pub use monitor::spawn;

#[cfg(windows)]
mod monitor {
    use std::{thread, time::Duration};

    use clipboard_win::{raw, Monitor};
    use tauri::{AppHandle, Manager};

    use super::{capture, reader, source};
    use crate::{
        janitor,
        paths::AppPaths,
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
        if app.state::<AppState>().is_paused() {
            return Ok(());
        }
        // 在读取前取前台进程，尽量贴近触发复制的程序
        let source_app = source::foreground_app();
        let Some(captured) = reader::read()? else {
            return Ok(());
        };

        let store = app.state::<Store>();
        let paths = app.state::<AppPaths>();
        let settings = store.capture_settings()?;
        let Some(clip) = capture::prepare(captured, source_app, &paths.images_dir, settings.max_item_bytes)? else {
            log::info!("内容超过 {} 字节上限，未记录", settings.max_item_bytes);
            return Ok(());
        };

        let now = now_ms();
        let result = store.upsert_clip(&clip, now, settings.ttl_ms)?;
        // 不记录内容本身，只记录类型与大小
        log::info!(
            "{} {} 条目 #{}（{} 字节，来自 {}）",
            if result.inserted { "新增" } else { "置顶" },
            clip.kind.as_str(),
            result.id,
            clip.size_bytes,
            clip.source_app.as_deref().unwrap_or("未知程序"),
        );
        if result.inserted {
            janitor::cleanup(&store, &paths, &settings, now)?;
        }
        Ok(())
    }
}
