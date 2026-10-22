use tauri::{plugin::TauriPlugin, AppHandle, Wry};
use tauri_plugin_global_shortcut::{Builder, GlobalShortcutExt, ShortcutState};

use crate::windows;

/// 打开剪贴板历史的默认热键。TODO(M4): 改为可配置，并支持接管 Win+V。
pub const OPEN_POPUP: &str = "alt+v";

pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                windows::toggle_popup(app);
            }
        })
        .build()
}

/// 注册失败（如已被其他程序占用）不影响启动，仍可从托盘打开。
pub fn register(app: &AppHandle) {
    match app.global_shortcut().register(OPEN_POPUP) {
        Ok(()) => log::info!("已注册热键 {OPEN_POPUP}"),
        Err(e) => log::warn!("注册热键 {OPEN_POPUP} 失败: {e}"),
    }
}
