//! 检查与安装更新。发布清单 latest.json 放在 GitHub 上名为 `updater` 的发布中（由发布工作流更新），
//! 安装包经 Tauri updater 校验签名后才会安装。

use std::{sync::Mutex, time::Duration};

use anyhow::{bail, Context};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

use crate::{paths::AppPaths, settings::SettingsState, store::now_ms, windows::POPUP};

pub const REPO_URL: &str = "https://github.com/kun775/AnyPaste";
pub const RELEASES_URL: &str = "https://github.com/kun775/AnyPaste/releases";
pub const STATUS_EVENT: &str = "update-status";

/// 启动后等待一段时间再检查，避免与开机时的其他程序争抢网络。
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub checking: bool,
    pub installing: bool,
    /// 下载进度 0–1；总大小未知时为空。
    pub progress: Option<f64>,
    pub available: Option<UpdateInfo>,
    pub last_checked_at: Option<i64>,
    pub error: Option<String>,
    /// 便携版不支持自动安装。
    pub portable: bool,
}

#[derive(Default)]
pub struct UpdateService {
    status: Mutex<UpdateStatus>,
    /// 已通知过的版本，避免每天重复提醒同一版本。
    notified: Mutex<Option<String>>,
    busy: tokio::sync::Mutex<()>,
}

impl UpdateService {
    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
        let status = {
            let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.clone()
        };
        let _ = app.emit(STATUS_EVENT, status);
    }
}

pub fn spawn(app: AppHandle) {
    let svc = UpdateService::default();
    svc.status.lock().unwrap_or_else(|e| e.into_inner()).portable = app.state::<AppPaths>().portable;
    app.manage(svc);

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            let (auto_check, auto_install) = {
                let s = app.state::<SettingsState>();
                let s = s.get();
                (s.auto_check_update, s.auto_install_update)
            };
            if auto_check {
                if let Ok(Some(info)) = check(&app).await {
                    notify_once(&app, &info);
                    if auto_install && !app.state::<AppPaths>().portable && !popup_visible(&app) {
                        log::info!("自动安装更新 {}", info.version);
                        if let Err(e) = install(&app).await {
                            log::warn!("自动安装更新失败: {e:#}");
                        }
                    }
                }
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// 检查是否有新版本，结果同时写入状态并通知前端。
pub async fn check(app: &AppHandle) -> anyhow::Result<Option<UpdateInfo>> {
    let svc = app.state::<UpdateService>();
    let _busy = svc.busy.lock().await;
    svc.update(app, |s| {
        s.checking = true;
        s.error = None;
    });
    let result = async {
        let update = app.updater()?.check().await?;
        Ok::<_, anyhow::Error>(update.map(|u| UpdateInfo {
            version: u.version.clone(),
            notes: u.body.clone(),
            date: u.date.map(|d| d.to_string()),
        }))
    }
    .await
    .map_err(friendly);

    svc.update(app, |s| {
        s.checking = false;
        s.last_checked_at = Some(now_ms());
        match &result {
            Ok(info) => s.available = info.clone(),
            Err(e) => s.error = Some(format!("{e:#}")),
        }
    });
    match &result {
        Ok(Some(info)) => log::info!("发现新版本 {}", info.version),
        Ok(None) => log::info!("已是最新版本"),
        Err(e) => log::warn!("检查更新失败: {e:#}"),
    }
    result
}

/// 下载并安装更新。Windows 上安装程序启动后本程序会退出，安装完成后自动重新打开。
pub async fn install(app: &AppHandle) -> anyhow::Result<()> {
    if app.state::<AppPaths>().portable {
        bail!("便携版不支持自动安装，请从发布页下载新版本并替换程序文件");
    }
    let svc = app.state::<UpdateService>();
    let _busy = svc.busy.lock().await;
    let update = app
        .updater()?
        .check()
        .await
        .map_err(|e| friendly(e.into()))?
        .context("已是最新版本")?;

    svc.update(app, |s| {
        s.installing = true;
        s.progress = Some(0.0);
        s.error = None;
    });
    let mut downloaded = 0u64;
    let mut last_reported = 0.0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let progress = total.map(|t| downloaded as f64 / t.max(1) as f64);
                // 每 2% 汇报一次，避免事件过多
                if progress.is_none_or(|p| p - last_reported >= 0.02 || p >= 1.0) {
                    last_reported = progress.unwrap_or(0.0);
                    svc.update(app, |s| s.progress = progress);
                }
            },
            || {},
        )
        .await;
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => {
            let e = friendly(e.into());
            svc.update(app, |s| {
                s.installing = false;
                s.progress = None;
                s.error = Some(format!("{e:#}"));
            });
            return Err(e);
        }
    };
    log::info!("更新 {} 已下载并通过签名校验，开始安装", update.version);
    // 安装程序接管后本进程退出；若启动安装失败则恢复状态
    if let Err(e) = update.install(bytes) {
        let e = friendly(e.into());
        svc.update(app, |s| {
            s.installing = false;
            s.error = Some(format!("{e:#}"));
        });
        return Err(e);
    }
    Ok(())
}

fn notify_once(app: &AppHandle, info: &UpdateInfo) {
    let svc = app.state::<UpdateService>();
    let mut notified = svc.notified.lock().unwrap_or_else(|e| e.into_inner());
    if notified.as_deref() == Some(info.version.as_str()) {
        return;
    }
    *notified = Some(info.version.clone());
    let body = format!("发现新版本 {}，可在 设置 → 关于 中更新", info.version);
    if let Err(e) = app.notification().builder().title("AnyPaste").body(body).show() {
        log::warn!("显示通知失败: {e}");
    }
}

fn popup_visible(app: &AppHandle) -> bool {
    app.get_webview_window(POPUP)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// 发布清单不存在（尚未发布支持自动更新的版本）等情况给出可读的提示。
fn friendly(e: anyhow::Error) -> anyhow::Error {
    let text = format!("{e:#}");
    if text.contains("404") || text.contains("Could not fetch a valid release JSON") {
        anyhow::anyhow!("暂未获取到可用的更新信息，请稍后再试，或前往发布页查看")
    } else if text.contains("signature") || text.contains("Signature") {
        anyhow::anyhow!("更新包签名校验失败，已取消安装")
    } else {
        e.context("检查更新失败")
    }
}
