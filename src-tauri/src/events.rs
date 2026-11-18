use tauri::{AppHandle, Emitter};

use crate::{settings::Settings, windows::POPUP};

/// 条目新增、更新或删除；弹出列表收到后重新加载。
pub const CLIPS_CHANGED: &str = "clips-changed";
/// 常用片段新增、修改或删除。
pub const SNIPPETS_CHANGED: &str = "snippets-changed";
/// 弹出列表即将显示；前端重置搜索与选中项。
pub const POPUP_SHOWN: &str = "popup-shown";
/// 设置已修改，负载为最新设置；各窗口据此更新主题等。
pub const SETTINGS_CHANGED: &str = "settings-changed";

/// 仅通知界面刷新条目列表，不标记云同步待上传（用于置顶变更、云端下载落地等无需回传云端的场景）。
pub fn emit_clips_changed(app: &AppHandle) {
    if let Err(e) = app.emit(CLIPS_CHANGED, ()) {
        log::warn!("发送 {CLIPS_CHANGED} 事件失败: {e}");
    }
}

pub fn clips_changed(app: &AppHandle) {
    emit_clips_changed(app);
    // 永久条目若有变化，防抖后同步到云端（同步服务会比对指纹，无变化时跳过）
    crate::sync::mark_dirty(app);
}

pub fn snippets_changed(app: &AppHandle) {
    if let Err(e) = app.emit(SNIPPETS_CHANGED, ()) {
        log::warn!("发送 {SNIPPETS_CHANGED} 事件失败: {e}");
    }
}

pub fn popup_shown(app: &AppHandle) {
    if let Err(e) = app.emit_to(POPUP, POPUP_SHOWN, ()) {
        log::warn!("发送 {POPUP_SHOWN} 事件失败: {e}");
    }
}

pub fn settings_changed(app: &AppHandle, settings: &Settings) {
    if let Err(e) = app.emit(SETTINGS_CHANGED, settings) {
        log::warn!("发送 {SETTINGS_CHANGED} 事件失败: {e}");
    }
}
