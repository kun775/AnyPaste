use std::fmt::Display;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::{
    events, hotkey, janitor,
    paths::AppPaths,
    prefs,
    settings::{Settings, SettingsState},
    state::AppState,
    store::{now_ms, ClipItem, Counts, Cursor, Store},
    sync::{self, SecretsInput, SyncConfig, SyncService, SyncStatus},
    updater::{self, UpdateInfo, UpdateService, UpdateStatus},
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
    let ttl = app.state::<SettingsState>().get().capture().ttl_ms;
    store.touch(id, now_ms(), ttl)?;
    events::clips_changed(app);
    Ok(())
}

#[tauri::command]
pub async fn set_permanent(
    app: AppHandle,
    store: State<'_, Store>,
    prefs: State<'_, SettingsState>,
    id: i64,
    permanent: bool,
) -> CmdResult<()> {
    let ttl = prefs.get().capture().ttl_ms;
    store.set_permanent(id, permanent, now_ms(), ttl).map_err(err)?;
    events::clips_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_pinned(app: AppHandle, store: State<'_, Store>, id: i64, pinned: bool) -> CmdResult<()> {
    store.set_pinned(id, pinned, now_ms()).map_err(err)?;
    // 置顶只影响本机排序，不需要触发同步，只通知界面刷新
    if let Err(e) = tauri::Emitter::emit(&app, events::CLIPS_CHANGED, ()) {
        log::warn!("发送 {} 事件失败: {e}", events::CLIPS_CHANGED);
    }
    Ok(())
}

/// 图片条目的原图（PNG data URL），用于预览。
#[tauri::command]
pub async fn clip_image(store: State<'_, Store>, paths: State<'_, AppPaths>, id: i64) -> CmdResult<String> {
    let content = store.clip_content(id).map_err(err)?.ok_or("条目不存在或已删除")?;
    let file = content.image_file.ok_or("该条目不是图片")?;
    let bytes = std::fs::read(paths.images_dir.join(&file)).map_err(|e| format!("读取图片失败: {e}"))?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    ))
}

#[tauri::command]
pub async fn delete_clip(app: AppHandle, store: State<'_, Store>, paths: State<'_, AppPaths>, id: i64) -> CmdResult<()> {
    // 开启同步时永久条目留下墓碑，以便把删除同步到其他设备
    let tombstone = sync::is_enabled(&app) && store.is_permanent(id).map_err(err)?;
    let deleted = if tombstone {
        store.tombstone_clip(id, now_ms())
    } else {
        store.delete_clip(id)
    }
    .map_err(err)?;
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

/// 设置窗口所需的完整状态：用户设置 + 只能从系统读取的状态。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    settings: Settings,
    autostart: bool,
    /// 注册表中已让出系统 Win+V。
    win_v_released: bool,
    /// 本程序已成功注册 Win+V。
    win_v_active: bool,
}

fn settings_view(app: &AppHandle) -> SettingsView {
    SettingsView {
        settings: prefs::current(app),
        autostart: prefs::autostart_enabled(app),
        #[cfg(windows)]
        win_v_released: crate::winv::is_released(),
        #[cfg(not(windows))]
        win_v_released: false,
        win_v_active: app.state::<hotkey::Hotkeys>().win_v_active(),
    }
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> SettingsView {
    settings_view(&app)
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, settings: Settings) -> CmdResult<SettingsView> {
    prefs::update(&app, settings).map_err(err)?;
    Ok(settings_view(&app))
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<SettingsView> {
    prefs::set_autostart(&app, enabled).map_err(err)?;
    Ok(settings_view(&app))
}

/// 录制新热键期间暂停全部全局热键，结束后按当前设置恢复。
#[tauri::command]
pub async fn suspend_hotkeys(app: AppHandle, suspend: bool) -> CmdResult<()> {
    if suspend {
        hotkey::suspend(&app);
        Ok(())
    } else {
        hotkey::apply(&app, &prefs::current(&app)).map_err(err)
    }
}

#[tauri::command]
pub async fn restart_explorer(app: AppHandle) -> CmdResult<SettingsView> {
    #[cfg(windows)]
    {
        crate::winv::restart_explorer().map_err(err)?;
        // 资源管理器启动并注册其热键需要一点时间，之后再重新注册 Win+V
        std::thread::sleep(std::time::Duration::from_millis(1500));
        hotkey::apply(&app, &prefs::current(&app)).map_err(err)?;
    }
    Ok(settings_view(&app))
}

#[tauri::command]
pub async fn open_data_dir(paths: State<'_, AppPaths>) -> CmdResult<()> {
    std::process::Command::new("explorer.exe")
        .arg(&paths.data_dir)
        .spawn()
        .map_err(err)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    temp: usize,
    perm: usize,
    /// 数据库与图片文件占用的字节数。
    bytes: u64,
}

#[tauri::command]
pub async fn storage_stats(store: State<'_, Store>, paths: State<'_, AppPaths>) -> CmdResult<StorageStats> {
    let counts = store.counts(now_ms()).map_err(err)?;
    let file_size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mut bytes = file_size(&paths.db_file)
        + file_size(&paths.db_file.with_extension("db-wal"))
        + file_size(&paths.db_file.with_extension("db-shm"));
    if let Ok(entries) = std::fs::read_dir(&paths.images_dir) {
        bytes += entries.flatten().map(|e| file_size(&e.path())).sum::<u64>();
    }
    Ok(StorageStats { temp: counts.temp, perm: counts.perm, bytes })
}

/// 云同步设置页所需状态；凭据只返回是否已保存，不回传内容。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncView {
    config: SyncConfig,
    has_password: bool,
    has_secret_key: bool,
    has_passphrase: bool,
    status: SyncStatus,
}

fn sync_view(app: &AppHandle) -> SyncView {
    let secrets = sync::load_secrets();
    SyncView {
        config: sync::load_config(&app.state::<Store>()),
        has_password: secrets.password.is_some(),
        has_secret_key: secrets.secret_key.is_some(),
        has_passphrase: secrets.passphrase.is_some(),
        status: app.state::<SyncService>().status(),
    }
}

#[tauri::command]
pub async fn get_sync(app: AppHandle) -> SyncView {
    sync_view(&app)
}

#[tauri::command]
pub async fn save_sync(app: AppHandle, config: SyncConfig, secrets: SecretsInput) -> CmdResult<SyncView> {
    sync::save(&app, config, &secrets).map_err(err)?;
    Ok(sync_view(&app))
}

#[tauri::command]
pub async fn test_sync(config: SyncConfig, secrets: SecretsInput) -> CmdResult<String> {
    sync::test(config, &secrets).await.map_err(err)
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> CmdResult<()> {
    if !sync::is_enabled(&app) {
        return Err("请先启用云同步".into());
    }
    sync::request(&app);
    Ok(())
}

#[tauri::command]
pub async fn get_update_status(app: AppHandle) -> UpdateStatus {
    app.state::<UpdateService>().status()
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult<Option<UpdateInfo>> {
    updater::check(&app).await.map_err(err)
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    updater::install(&app).await.map_err(err)
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    Repo,
    Releases,
    Issues,
}

/// 用默认浏览器打开项目页面（只允许固定的几个地址）。
#[tauri::command]
pub async fn open_link(link: Link) -> CmdResult<()> {
    let url = match link {
        Link::Repo => updater::REPO_URL.to_string(),
        Link::Releases => updater::RELEASES_URL.to_string(),
        Link::Issues => format!("{}/issues", updater::REPO_URL),
    };
    std::process::Command::new("explorer.exe").arg(url).spawn().map_err(err)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    data_dir: String,
    log_dir: String,
    repo_url: &'static str,
    portable: bool,
}

#[tauri::command]
pub fn app_info(app: AppHandle, paths: State<'_, AppPaths>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: paths.data_dir.display().to_string(),
        log_dir: paths.log_dir.display().to_string(),
        repo_url: updater::REPO_URL,
        portable: paths.portable,
    }
}
