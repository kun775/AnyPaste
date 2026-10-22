use std::{collections::HashMap, sync::Mutex};

use tauri::{plugin::TauriPlugin, AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{Builder, GlobalShortcutExt, Shortcut, ShortcutState};

/// 可绑定热键的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    OpenPopup,
    PermanentCopy,
}

impl Action {
    /// TODO(M4): 改为读取设置，并支持接管 Win+V。
    fn default_accelerator(self) -> &'static str {
        match self {
            Self::OpenPopup => "alt+v",
            Self::PermanentCopy => "ctrl+shift+c",
        }
    }
}

/// 已注册的快捷键 ID 到动作的映射。
#[derive(Default)]
pub struct Hotkeys {
    bindings: Mutex<HashMap<u32, Action>>,
}

impl Hotkeys {
    fn action(&self, shortcut: &Shortcut) -> Option<Action> {
        self.bindings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&shortcut.id())
            .copied()
    }
}

pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            match app.state::<Hotkeys>().action(shortcut) {
                #[cfg(windows)]
                Some(Action::OpenPopup) => crate::windows::toggle_popup(app),
                #[cfg(windows)]
                Some(Action::PermanentCopy) => crate::permanent::trigger(app),
                _ => {}
            }
        })
        .build()
}

/// 注册默认热键。注册失败（如已被其他程序占用）不影响启动，仍可从托盘使用。
pub fn register_defaults(app: &AppHandle) {
    app.manage(Hotkeys::default());
    for action in [Action::OpenPopup, Action::PermanentCopy] {
        let accel = action.default_accelerator();
        match bind(app, action, accel) {
            Ok(()) => log::info!("已注册热键 {accel}"),
            Err(e) => log::warn!("注册热键 {accel} 失败: {e:#}"),
        }
    }
}

fn bind(app: &AppHandle, action: Action, accel: &str) -> anyhow::Result<()> {
    let shortcut: Shortcut = accel.parse()?;
    app.global_shortcut().register(shortcut)?;
    app.state::<Hotkeys>()
        .bindings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(shortcut.id(), action);
    Ok(())
}
