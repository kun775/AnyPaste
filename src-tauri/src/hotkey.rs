use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use anyhow::Context;
use tauri::{plugin::TauriPlugin, AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{Builder, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::settings::Settings;

const WIN_V: &str = "super+v";

/// 可绑定热键的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    OpenPopup,
    PermanentCopy,
}

/// 已注册的快捷键 ID 到快捷键与动作的映射。
#[derive(Default)]
pub struct Hotkeys {
    bindings: Mutex<HashMap<u32, (Shortcut, Action)>>,
    /// Win+V 是否已成功注册（需注册表让出且资源管理器已重启）。
    win_v_active: AtomicBool,
}

impl Hotkeys {
    fn bindings(&self) -> std::sync::MutexGuard<'_, HashMap<u32, (Shortcut, Action)>> {
        self.bindings.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn win_v_active(&self) -> bool {
        self.win_v_active.load(Ordering::Relaxed)
    }
}

pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let action = app.state::<Hotkeys>().bindings().get(&shortcut.id()).map(|(_, a)| *a);
            match action {
                #[cfg(windows)]
                Some(Action::OpenPopup) => crate::windows::toggle_popup(app),
                #[cfg(windows)]
                Some(Action::PermanentCopy) => crate::permanent::trigger(app),
                _ => {}
            }
        })
        .build()
}

/// 按设置重新注册全部热键。失败时已注册的部分保持原样，调用方应以旧设置再次调用以回滚。
pub fn apply(app: &AppHandle, settings: &Settings) -> anyhow::Result<()> {
    suspend(app);
    bind(app, Action::OpenPopup, &settings.hotkey_open)
        .with_context(|| format!("热键 {} 注册失败，可能已被其他程序占用", display(&settings.hotkey_open)))?;
    bind(app, Action::PermanentCopy, &settings.hotkey_permanent)
        .with_context(|| format!("热键 {} 注册失败，可能已被其他程序占用", display(&settings.hotkey_permanent)))?;

    // Win+V 由系统占用时注册会失败，属于预期情况（需重启资源管理器），不视为错误
    let open_is_win_v = is_win_v(&settings.hotkey_open);
    let win_v = settings.take_over_win_v && !open_is_win_v && bind(app, Action::OpenPopup, WIN_V).is_ok();
    app.state::<Hotkeys>().win_v_active.store(win_v || open_is_win_v, Ordering::Relaxed);
    log::info!(
        "已注册热键 {}、{}{}",
        settings.hotkey_open,
        settings.hotkey_permanent,
        if win_v { "、super+v" } else { "" }
    );
    Ok(())
}

/// 暂停全部热键（录制新热键时使用，否则按下已注册的组合会被系统拦截）。
/// 不能用插件的 `unregister_all`：它持有插件内部锁等待主线程执行注销，而主线程分发热键事件时
/// 也要取这把锁；设置窗口在后台线程调用时恰好按下热键，两边互相等待，主线程卡死，托盘与热键全部失灵。
/// `unregister_multiple` 在主线程执行完毕后才取锁，没有这个问题。
pub fn suspend(app: &AppHandle) {
    let hotkeys = app.state::<Hotkeys>();
    let registered: Vec<Shortcut> = hotkeys.bindings().drain().map(|(_, (s, _))| s).collect();
    hotkeys.win_v_active.store(false, Ordering::Relaxed);
    if registered.is_empty() {
        return;
    }
    if let Err(e) = app.global_shortcut().unregister_multiple(registered) {
        log::warn!("注销热键失败: {e}");
    }
}

fn bind(app: &AppHandle, action: Action, accel: &str) -> anyhow::Result<()> {
    let shortcut: Shortcut = accel.parse()?;
    app.global_shortcut().register(shortcut)?;
    app.state::<Hotkeys>().bindings().insert(shortcut.id(), (shortcut, action));
    Ok(())
}

/// 按解析后的组合键比较，`Super+V`、`cmd+v` 等写法都视为 Win+V。
fn is_win_v(accel: &str) -> bool {
    match (accel.parse::<Shortcut>(), WIN_V.parse::<Shortcut>()) {
        (Ok(a), Ok(b)) => a.id() == b.id(),
        _ => false,
    }
}

/// `ctrl+shift+c` → `Ctrl+Shift+C`，用于提示信息。
fn display(accel: &str) -> String {
    accel
        .split('+')
        .map(|p| match p {
            "ctrl" | "control" => "Ctrl".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "super" | "win" | "cmd" | "meta" => "Win".to_string(),
            other => other.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::{display, is_win_v};

    #[test]
    fn win_v_spellings_match() {
        assert!(is_win_v("super+v"));
        assert!(is_win_v("Super+V"));
        assert!(is_win_v("cmd+v"));
        assert!(!is_win_v("ctrl+v"));
        assert!(!is_win_v("not a key"));
    }

    #[test]
    fn display_is_readable() {
        assert_eq!(display("ctrl+shift+c"), "Ctrl+Shift+C");
        assert_eq!(display("super+v"), "Win+V");
    }
}
