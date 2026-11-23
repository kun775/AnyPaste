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
    store::{
        now_ms, ClipContent, ClipItem, ClipKind, Counts, CreateSnippetError, Cursor, KindFilter, SnippetItem, Store,
        MAX_SNIPPETS,
    },
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

#[tauri::command]
pub fn set_popup_preview(app: AppHandle, mode: windows::PreviewMode) -> CmdResult<()> {
    windows::set_popup_preview(&app, mode).map_err(err)
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
    prefs: State<'_, SettingsState>,
    tab: Tab,
    kind: Option<KindFilter>,
    query: Option<String>,
    before: Option<Cursor>,
) -> CmdResult<Vec<ClipItem>> {
    let permanent = matches!(tab, Tab::Perm);
    // 频率排序仅用于永久条目；临时条目始终按最近复制时间排列，避免刚复制的内容沉底
    store
        .list_clips_filtered(
            permanent,
            kind,
            query.as_deref(),
            before,
            now_ms(),
            permanent && prefs.get().sort_by_frequency,
        )
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
    let content = clip_content(app, id)?;
    let result = paste_content_blocking(app, content, plain)?;
    // 只有真正粘贴到目标窗口才计入使用频率；从托盘打开时仅复制不算
    if result.pasted {
        if let Err(e) = app.state::<Store>().record_paste(id) {
            log::warn!("记录粘贴次数失败: {e}");
        }
    }
    Ok(result)
}

fn paste_content_blocking(app: &AppHandle, content: ClipContent, plain: bool) -> anyhow::Result<PasteResult> {
    write_clipboard(app, content, plain)?;
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
    tauri::async_runtime::spawn_blocking(move || copy_to_clipboard(&app, id))
        .await
        .map_err(err)?
        .map_err(err)
}

/// 写回剪贴板；写入带有 AnyPaste 标记，监听器不会重复记录，条目在列表中的位置保持不变。
fn copy_to_clipboard(app: &AppHandle, id: i64) -> anyhow::Result<()> {
    write_clipboard(app, clip_content(app, id)?, false)
}

fn clip_content(app: &AppHandle, id: i64) -> anyhow::Result<ClipContent> {
    app.state::<Store>().clip_content(id)?.context("条目不存在或已删除")
}

fn write_clipboard(app: &AppHandle, content: ClipContent, plain: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    crate::clipboard::write(content, plain, &app.state::<AppPaths>().images_dir)?;
    #[cfg(not(windows))]
    let _ = (content, plain);
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
    events::emit_clips_changed(&app);
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

/// 全文预览最多返回的字符数，避免超长文本拖慢 IPC 和渲染。
const TEXT_PREVIEW_CHARS: usize = 200_000;

/// 条目的完整文本，用于全文预览；文件条目为换行分隔的路径。
#[derive(Serialize)]
pub struct ClipText {
    text: String,
    /// 文本的总字符数。
    chars: usize,
    /// 超出上限，`text` 只含开头部分。
    truncated: bool,
}

fn text_preview_of(mut text: String) -> ClipText {
    let chars = text.chars().count();
    let truncated = chars > TEXT_PREVIEW_CHARS;
    if let Some((end, _)) = text.char_indices().nth(TEXT_PREVIEW_CHARS) {
        text.truncate(end);
    }
    ClipText { text, chars, truncated }
}

#[tauri::command]
pub async fn clip_text(store: State<'_, Store>, id: i64) -> CmdResult<ClipText> {
    let content = store.clip_content(id).map_err(err)?.ok_or("条目不存在或已删除")?;
    Ok(text_preview_of(content.text.unwrap_or_default()))
}

#[tauri::command]
pub async fn snippet_text(store: State<'_, Store>, id: i64) -> CmdResult<ClipText> {
    let text = store.snippet_content(id).map_err(err)?.ok_or("片段不存在或已删除")?;
    Ok(text_preview_of(text))
}

#[tauri::command]
pub async fn delete_clip(app: AppHandle, store: State<'_, Store>, paths: State<'_, AppPaths>, id: i64) -> CmdResult<()> {
    // 开启同步时，永久条目或刚从永久转为临时的条目留下墓碑，以便把删除同步到其他设备
    let tombstone = sync::is_enabled(&app) && store.needs_tombstone(id).map_err(err)?;
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

const MAX_SNIPPET_TITLE_CHARS: usize = 80;

fn validate_snippet(title: &str, content: &str, max_bytes: usize) -> CmdResult<(String, String)> {
    let title = title.trim();
    if title.is_empty() {
        return Err("片段标题不能为空".into());
    }
    if title.chars().count() > MAX_SNIPPET_TITLE_CHARS {
        return Err(format!("片段标题不能超过 {MAX_SNIPPET_TITLE_CHARS} 个字符"));
    }
    if content.trim().is_empty() {
        return Err("片段内容不能为空".into());
    }
    if content.len() > max_bytes {
        return Err(format!("片段内容超出大小上限（{} MB）", max_bytes / 1024 / 1024));
    }
    Ok((title.to_string(), content.to_string()))
}

/// 从文本首行生成片段标题；过长时保留前 40 个字符。
fn snippet_title(text: &str) -> String {
    let first = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
    let mut title: String = first.chars().take(40).collect();
    if first.chars().count() > 40 {
        title.push('…');
    }
    if title.is_empty() {
        "剪贴板片段".into()
    } else {
        title
    }
}

/// 校验并新建片段，两条创建路径（设置页新建、从剪贴板保存）共用。
fn create_snippet(app: &AppHandle, store: &Store, title: &str, content: &str, max_bytes: usize) -> CmdResult<SnippetItem> {
    let (title, content) = validate_snippet(title, content, max_bytes)?;
    let item = store.create_snippet(&title, &content, now_ms()).map_err(|e| match e {
        CreateSnippetError::LimitReached => format!("最多保存 {MAX_SNIPPETS} 个常用片段，请先删除不再使用的片段"),
        CreateSnippetError::Db(e) => err(e),
    })?;
    events::snippets_changed(app);
    Ok(item)
}

#[tauri::command]
pub async fn list_snippets(
    store: State<'_, Store>,
    prefs: State<'_, SettingsState>,
    query: Option<String>,
) -> CmdResult<Vec<SnippetItem>> {
    store
        .list_snippets(query.as_deref(), prefs.get().sort_by_frequency)
        .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetDraft {
    id: i64,
    title: String,
    content: String,
}

#[tauri::command]
pub async fn get_snippet(store: State<'_, Store>, id: i64) -> CmdResult<SnippetDraft> {
    let (title, content) = store
        .snippet_draft(id)
        .map_err(err)?
        .ok_or("片段不存在或已删除")?;
    Ok(SnippetDraft { id, title, content })
}

#[tauri::command]
pub async fn save_snippet(
    app: AppHandle,
    store: State<'_, Store>,
    prefs: State<'_, SettingsState>,
    id: Option<i64>,
    title: String,
    content: String,
) -> CmdResult<SnippetItem> {
    let max_bytes = prefs.get().max_item_bytes;
    match id {
        None => create_snippet(&app, &store, &title, &content, max_bytes),
        Some(id) => {
            let (title, content) = validate_snippet(&title, &content, max_bytes)?;
            let item = store
                .update_snippet(id, &title, &content, now_ms())
                .map_err(err)?
                .ok_or("片段不存在或已删除")?;
            events::snippets_changed(&app);
            Ok(item)
        }
    }
}

#[tauri::command]
pub async fn delete_snippet(app: AppHandle, store: State<'_, Store>, id: i64) -> CmdResult<()> {
    if !store.delete_snippet(id).map_err(err)? {
        return Err("片段不存在或已删除".into());
    }
    events::snippets_changed(&app);
    Ok(())
}

/// 把文本条目按首行标题保存为片段。
#[tauri::command]
pub async fn save_clip_as_snippet(
    app: AppHandle,
    prefs: State<'_, SettingsState>,
    id: i64,
) -> CmdResult<SnippetItem> {
    let content = app
        .state::<Store>()
        .clip_content(id)
        .map_err(err)?
        .ok_or("条目不存在或已删除")?;
    if !matches!(content.kind, ClipKind::Text | ClipKind::Rich) {
        return Err("只有文本条目可以存为片段".into());
    }
    let text = content.text.unwrap_or_default();
    let title = snippet_title(&text);
    create_snippet(&app, &app.state::<Store>(), &title, &text, prefs.get().max_item_bytes)
}

fn load_snippet(app: &AppHandle, id: i64) -> anyhow::Result<ClipContent> {
    let text = app.state::<Store>().snippet_content(id)?.context("片段不存在或已删除")?;
    Ok(ClipContent {
        kind: ClipKind::Text,
        text: Some(text),
        html: None,
        rtf: None,
        image_file: None,
    })
}

#[tauri::command]
pub async fn paste_snippet(app: AppHandle, id: i64) -> CmdResult<PasteResult> {
    tauri::async_runtime::spawn_blocking(move || paste_snippet_blocking(&app, id))
        .await
        .map_err(err)?
        .map_err(err)
}

fn paste_snippet_blocking(app: &AppHandle, id: i64) -> anyhow::Result<PasteResult> {
    let content = load_snippet(app, id)?;
    let result = paste_content_blocking(app, content, true)?;
    if result.pasted {
        if let Err(e) = app.state::<Store>().record_snippet_paste(id) {
            log::warn!("记录片段粘贴次数失败: {e}");
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn copy_snippet(app: AppHandle, id: i64) -> CmdResult<()> {
    let content = load_snippet(&app, id).map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || write_clipboard(&app, content, true))
        .await
        .map_err(err)?
        .map_err(err)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_text_truncates_on_char_boundary() {
        let short = text_preview_of("你好".into());
        assert_eq!((short.text.as_str(), short.chars, short.truncated), ("你好", 2, false));

        let long = text_preview_of("字".repeat(TEXT_PREVIEW_CHARS + 3));
        assert_eq!(long.chars, TEXT_PREVIEW_CHARS + 3);
        assert!(long.truncated);
        assert_eq!(long.text.chars().count(), TEXT_PREVIEW_CHARS);
    }

    #[test]
    fn snippet_titles_come_from_first_line() {
        assert_eq!(snippet_title("\n  工作邮件  \n正文"), "工作邮件");
        assert_eq!(snippet_title(&"长".repeat(50)), format!("{}…", "长".repeat(40)));
        assert_eq!(snippet_title("\n "), "剪贴板片段");
    }

    #[test]
    fn snippet_validation_rejects_empty_and_oversized() {
        assert!(validate_snippet("", "正文", 100).is_err());
        assert!(validate_snippet("标题", " ", 100).is_err());
        assert!(validate_snippet("标题", "正文", 2).is_err());
        assert!(validate_snippet(" 标题 ", "正文", 100).is_ok());
    }
}
