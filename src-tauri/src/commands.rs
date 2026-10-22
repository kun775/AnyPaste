use std::fmt::Display;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::{
    events, janitor,
    paths::AppPaths,
    state::AppState,
    store::{now_ms, ClipItem, Counts, Cursor, Store},
    windows,
};

/// 命令错误以文本形式返回给前端。
type CmdResult<T> = Result<T, String>;

fn err(e: impl Display) -> String {
    format!("{e:#}")
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    Temp,
    Perm,
}

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

#[tauri::command]
pub async fn list_clips(
    store: State<'_, Store>,
    tab: Tab,
    query: Option<String>,
    before: Option<Cursor>,
) -> CmdResult<Vec<ClipItem>> {
    store
        .list_clips(matches!(tab, Tab::Perm), query.as_deref(), before, now_ms())
        .map_err(err)
}

#[tauri::command]
pub async fn clip_counts(store: State<'_, Store>) -> CmdResult<Counts> {
    store.counts(now_ms()).map_err(err)
}

/// 粘贴结果：`pasted` 为 false 表示没有粘贴目标（从托盘打开），内容只复制到了剪贴板。
#[derive(Serialize)]
pub struct PasteResult {
    pasted: bool,
}

#[tauri::command]
pub async fn paste_clip(app: AppHandle, id: i64, plain: bool) -> CmdResult<PasteResult> {
    // 写剪贴板与等待焦点切换会阻塞，放到阻塞线程池执行
    tauri::async_runtime::spawn_blocking(move || paste_blocking(&app, id, plain))
        .await
        .map_err(err)?
        .map_err(err)
}

fn paste_blocking(app: &AppHandle, id: i64, plain: bool) -> anyhow::Result<PasteResult> {
    copy_to_clipboard(app, id, plain)?;
    let Some(target) = app.state::<AppState>().paste_target() else {
        return Ok(PasteResult { pasted: false });
    };
    windows::hide_popup(app);
    #[cfg(windows)]
    crate::input::paste_into(target)?;
    #[cfg(not(windows))]
    let _ = target;
    Ok(PasteResult { pasted: true })
}

#[tauri::command]
pub async fn copy_clip(app: AppHandle, id: i64) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || copy_to_clipboard(&app, id, false))
        .await
        .map_err(err)?
        .map_err(err)
}

/// 写回剪贴板后置顶该条目；写入带有 AnyPaste 标记，监听器不会重复记录。
fn copy_to_clipboard(app: &AppHandle, id: i64, plain: bool) -> anyhow::Result<()> {
    let store = app.state::<Store>();
    let content = store.clip_content(id)?.context("条目不存在或已删除")?;
    #[cfg(windows)]
    crate::clipboard::write(content, plain, &app.state::<AppPaths>().images_dir)?;
    #[cfg(not(windows))]
    let _ = (content, plain);
    let settings = store.capture_settings()?;
    store.touch(id, now_ms(), settings.ttl_ms)?;
    events::clips_changed(app);
    Ok(())
}

#[tauri::command]
pub async fn set_permanent(app: AppHandle, store: State<'_, Store>, id: i64, permanent: bool) -> CmdResult<()> {
    let settings = store.capture_settings().map_err(err)?;
    store.set_permanent(id, permanent, now_ms(), settings.ttl_ms).map_err(err)?;
    events::clips_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_clip(app: AppHandle, store: State<'_, Store>, paths: State<'_, AppPaths>, id: i64) -> CmdResult<()> {
    let deleted = store.delete_clip(id).map_err(err)?;
    janitor::remove_images(&paths, &deleted.image_files);
    events::clips_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_temp(app: AppHandle, store: State<'_, Store>, paths: State<'_, AppPaths>) -> CmdResult<usize> {
    let deleted = store.clear_temp().map_err(err)?;
    janitor::remove_images(&paths, &deleted.image_files);
    log::info!("已清空临时条目 {} 条", deleted.rows);
    events::clips_changed(&app);
    Ok(deleted.rows)
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
