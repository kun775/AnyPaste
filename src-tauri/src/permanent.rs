//! 永久复制：模拟一次复制，把采集到的内容直接存为永久条目。

use std::{thread, time::Duration};

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::{
    clipboard, events, input,
    state::AppState,
    store::{now_ms, Store},
};

/// 这些程序改用 Ctrl+Insert 复制（不区分大小写）。TODO(M4): 改为设置项。
const TERMINALS: &[&str] = &[
    "WindowsTerminal.exe",
    "OpenConsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
];

/// 等待目标程序响应复制、剪贴板序列号发生变化。
const COPY_WAIT: Duration = Duration::from_millis(600);
/// 等待监听线程完成采集（含 80ms 合并与图片编码）。
const CAPTURE_WAIT: Duration = Duration::from_millis(1500);

enum Outcome {
    Saved,
    PromotedLatest,
    Skipped,
    Nothing,
}

impl Outcome {
    fn message(&self) -> &'static str {
        match self {
            Self::Saved => "已永久保存",
            Self::PromotedLatest => "未检测到新的复制，已将最近一条临时记录设为永久",
            Self::Skipped => "未保存：内容为空、被标记为隐私或超过大小上限",
            Self::Nothing => "未保存：没有选中内容，也没有可设为永久的记录",
        }
    }
}

/// 热键触发。整个流程包含等待，放到独立线程执行；进行中再次按下热键会被忽略。
pub fn trigger(app: &AppHandle) {
    if !app.state::<AppState>().begin_permanent_copy() {
        return;
    }
    let worker = app.clone();
    let spawned = thread::Builder::new().name("permanent-copy".into()).spawn(move || {
        let outcome = run(&worker);
        worker.state::<AppState>().end_permanent_copy();
        let message = match outcome {
            Ok(o) => o.message(),
            Err(e) => {
                log::warn!("永久复制失败: {e:#}");
                "永久保存失败，详情见日志"
            }
        };
        notify(&worker, message);
    });
    if let Err(e) = spawned {
        log::error!("无法创建永久复制线程: {e}");
        app.state::<AppState>().end_permanent_copy();
    }
}

fn run(app: &AppHandle) -> anyhow::Result<Outcome> {
    let state = app.state::<AppState>();
    let before = clipboard::sequence();
    let terminal = clipboard::foreground_app().is_some_and(|name| is_terminal(&name));
    // 标记有效期覆盖两段等待，避免等待尚未结束标记就自行过期
    state.arm_permanent(COPY_WAIT + CAPTURE_WAIT + Duration::from_secs(1));
    if let Err(e) = input::send_copy(terminal) {
        state.disarm_permanent();
        return Err(e);
    }

    if input::wait_until(COPY_WAIT, || clipboard::sequence() != before) {
        if input::wait_until(CAPTURE_WAIT, || !state.permanent_armed()) {
            log::info!("永久复制完成");
            return Ok(Outcome::Saved);
        }
        state.disarm_permanent();
        return Ok(Outcome::Skipped);
    }

    // 没有选中内容时剪贴板不变：把最近一条临时条目设为永久
    state.disarm_permanent();
    let store = app.state::<Store>();
    let Some(id) = store.latest_temp(now_ms())? else {
        return Ok(Outcome::Nothing);
    };
    let ttl = store.capture_settings()?.ttl_ms;
    store.set_permanent(id, true, now_ms(), ttl)?;
    events::clips_changed(app);
    log::info!("永久复制未检测到新内容，已将条目 #{id} 设为永久");
    Ok(Outcome::PromotedLatest)
}

fn is_terminal(process: &str) -> bool {
    TERMINALS.iter().any(|t| t.eq_ignore_ascii_case(process))
}

fn notify(app: &AppHandle, body: &str) {
    if let Err(e) = app.notification().builder().title("AnyPaste").body(body).show() {
        log::warn!("显示通知失败: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_match_ignores_case() {
        assert!(is_terminal("windowsterminal.exe"));
        assert!(is_terminal("PWSH.EXE"));
        assert!(!is_terminal("Code.exe"));
    }
}
