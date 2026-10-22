use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{paths::AppPaths, state::AppState, windows};

#[tauri::command]
pub fn hide_popup(app: AppHandle) {
    windows::hide_popup(&app);
}

/// 异步命令：Windows 上在同步命令里创建窗口会死锁。
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    windows::hide_popup(&app);
    windows::show_settings(&app);
}

#[tauri::command]
pub fn is_paused(state: State<'_, AppState>) -> bool {
    state.is_paused()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    data_dir: String,
    log_dir: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, paths: State<'_, AppPaths>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: paths.data_dir.display().to_string(),
        log_dir: paths.log_dir.display().to_string(),
    }
}
