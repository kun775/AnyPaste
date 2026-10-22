use tauri::{AppHandle, Emitter};

use crate::windows::POPUP;

/// 条目新增、更新或删除；弹出列表收到后重新加载。
pub const CLIPS_CHANGED: &str = "clips-changed";
/// 弹出列表即将显示；前端重置搜索与选中项。
pub const POPUP_SHOWN: &str = "popup-shown";

pub fn clips_changed(app: &AppHandle) {
    if let Err(e) = app.emit(CLIPS_CHANGED, ()) {
        log::warn!("发送 {CLIPS_CHANGED} 事件失败: {e}");
    }
}

pub fn popup_shown(app: &AppHandle) {
    if let Err(e) = app.emit_to(POPUP, POPUP_SHOWN, ()) {
        log::warn!("发送 {POPUP_SHOWN} 事件失败: {e}");
    }
}
