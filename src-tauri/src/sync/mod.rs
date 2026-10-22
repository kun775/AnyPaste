//! 云同步：配置、凭据、后台调度。同步逻辑见 [`engine`]。

mod crypto;
pub mod engine;

use std::{sync::Mutex, time::Duration};

use anyhow::{bail, Context};
use opendal::{services, Operator};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::{
    events, janitor,
    paths::AppPaths,
    store::{now_ms, Fingerprint, Store},
};
use engine::{RemoteState, Report};

const CONFIG_KEY: &str = "sync";
const LAST_SYNC_KEY: &str = "sync_last_at";
/// 本地变化后等待一段时间再同步，合并连续操作。
const DEBOUNCE: Duration = Duration::from_secs(30);
/// 单次同步的超时（HTTP 客户端默认不超时）。
const RUN_TIMEOUT: Duration = Duration::from_secs(600);
const TEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const STATUS_EVENT: &str = "sync-status";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Webdav,
    S3,
}

/// 非敏感的同步配置，存在数据库中；密码类信息存在 Windows 凭据管理器中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncConfig {
    pub enabled: bool,
    pub backend: Backend,
    pub webdav_endpoint: String,
    pub webdav_username: String,
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_access_key_id: String,
    /// 远端目录（WebDAV）或路径前缀（S3）。
    pub root: String,
    pub encrypt: bool,
    pub interval_mins: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: Backend::Webdav,
            webdav_endpoint: String::new(),
            webdav_username: String::new(),
            s3_endpoint: String::new(),
            s3_bucket: String::new(),
            s3_region: "auto".into(),
            s3_access_key_id: String::new(),
            root: "/AnyPaste".into(),
            encrypt: true,
            interval_mins: 15,
        }
    }
}

impl SyncConfig {
    /// 校验连接参数（测试连接与启用同步时都需要）。
    pub fn validated(mut self) -> Result<Self, String> {
        let trim = |s: &mut String| *s = s.trim().to_string();
        for s in [
            &mut self.webdav_endpoint,
            &mut self.webdav_username,
            &mut self.s3_endpoint,
            &mut self.s3_bucket,
            &mut self.s3_region,
            &mut self.s3_access_key_id,
            &mut self.root,
        ] {
            trim(s);
        }
        if self.root.is_empty() {
            self.root = "/".into();
        }
        if !(5..=1440).contains(&self.interval_mins) {
            return Err("同步间隔需在 5 分钟到 24 小时之间".into());
        }
        let endpoint = match self.backend {
            Backend::Webdav => &self.webdav_endpoint,
            Backend::S3 => &self.s3_endpoint,
        };
        if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
            return Err("服务器地址需以 http:// 或 https:// 开头".into());
        }
        if self.backend == Backend::S3 && (self.s3_bucket.is_empty() || self.s3_access_key_id.is_empty()) {
            return Err("请填写 Bucket 与 Access Key ID".into());
        }
        Ok(self)
    }
}

pub fn load_config(store: &Store) -> SyncConfig {
    store
        .get_setting(CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_config(store: &Store, config: &SyncConfig) -> anyhow::Result<()> {
    store.put_setting(CONFIG_KEY, &serde_json::to_string(config)?)?;
    Ok(())
}

/// 存在凭据管理器中的敏感信息。
#[derive(Debug, Clone, Default)]
pub struct Secrets {
    pub password: Option<String>,
    pub secret_key: Option<String>,
    pub passphrase: Option<String>,
}

/// 前端提交的敏感信息：`None` 表示不修改，空字符串表示清除。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsInput {
    pub password: Option<String>,
    pub secret_key: Option<String>,
    pub passphrase: Option<String>,
}

mod keystore {
    const SERVICE: &str = "AnyPaste";
    pub const PASSWORD: &str = "webdav-password";
    pub const SECRET_KEY: &str = "s3-secret-key";
    pub const PASSPHRASE: &str = "sync-passphrase";

    #[cfg(windows)]
    pub fn get(user: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, user).ok()?.get_password().ok()
    }

    #[cfg(windows)]
    pub fn set(user: &str, value: &str) -> anyhow::Result<()> {
        let entry = keyring::Entry::new(SERVICE, user)?;
        if value.is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.into()),
            }
        } else {
            Ok(entry.set_password(value)?)
        }
    }

    #[cfg(not(windows))]
    pub fn get(_user: &str) -> Option<String> {
        None
    }

    #[cfg(not(windows))]
    pub fn set(_user: &str, _value: &str) -> anyhow::Result<()> {
        anyhow::bail!("当前平台不支持保存凭据")
    }
}

pub fn load_secrets() -> Secrets {
    Secrets {
        password: keystore::get(keystore::PASSWORD),
        secret_key: keystore::get(keystore::SECRET_KEY),
        passphrase: keystore::get(keystore::PASSPHRASE),
    }
}

fn save_secrets(input: &SecretsInput) -> anyhow::Result<()> {
    for (user, value) in [
        (keystore::PASSWORD, &input.password),
        (keystore::SECRET_KEY, &input.secret_key),
        (keystore::PASSPHRASE, &input.passphrase),
    ] {
        if let Some(v) = value {
            keystore::set(user, v).context("保存凭据失败")?;
        }
    }
    Ok(())
}

/// 已保存的凭据与本次输入合并（用于测试尚未保存的配置）。
fn merge_secrets(stored: Secrets, input: &SecretsInput) -> Secrets {
    let pick = |s: Option<String>, i: &Option<String>| match i {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v.clone()),
        None => s,
    };
    Secrets {
        password: pick(stored.password, &input.password),
        secret_key: pick(stored.secret_key, &input.secret_key),
        passphrase: pick(stored.passphrase, &input.passphrase),
    }
}

pub fn operator(config: &SyncConfig, secrets: &Secrets) -> anyhow::Result<Operator> {
    let op = match config.backend {
        Backend::Webdav => {
            let mut b = services::Webdav::default().endpoint(&config.webdav_endpoint).root(&config.root);
            if !config.webdav_username.is_empty() {
                b = b.username(&config.webdav_username);
            }
            if let Some(p) = &secrets.password {
                b = b.password(p);
            }
            Operator::new(b)?.finish()
        }
        Backend::S3 => {
            let secret = secrets.secret_key.as_deref().context("请填写 Secret Access Key")?;
            let b = services::S3::default()
                .endpoint(&config.s3_endpoint)
                .bucket(&config.s3_bucket)
                .region(&config.s3_region)
                .access_key_id(&config.s3_access_key_id)
                .secret_access_key(secret)
                .root(&config.root)
                // 只使用界面中填写的凭据，不读取本机 AWS 配置与实例元数据
                .disable_config_load()
                .disable_ec2_metadata();
            Operator::new(b)?.finish()
        }
    };
    Ok(op)
}

/// 把常见的存储错误换成用户能理解的提示，其余保留原始错误链。
fn friendly(e: anyhow::Error) -> anyhow::Error {
    let kind = e.chain().find_map(|c| c.downcast_ref::<opendal::Error>()).map(|oe| oe.kind());
    match kind {
        Some(opendal::ErrorKind::PermissionDenied) => anyhow::anyhow!("认证失败：请检查用户名、密码或访问密钥"),
        Some(opendal::ErrorKind::ConfigInvalid) => anyhow::anyhow!("连接配置无效：{e:#}"),
        _ => e,
    }
}

fn passphrase(config: &SyncConfig, secrets: &Secrets) -> anyhow::Result<Option<String>> {
    if !config.encrypt {
        return Ok(None);
    }
    match &secrets.passphrase {
        Some(p) if !p.is_empty() => Ok(Some(p.clone())),
        _ => bail!("已开启端到端加密，请填写同步口令"),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncState {
    #[default]
    Disabled,
    Idle,
    Syncing,
    Error,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub state: SyncState,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_report: Option<Report>,
}

/// 后台同步服务。
#[derive(Default)]
pub struct SyncService {
    manual: Notify,
    dirty: Notify,
    status: Mutex<SyncStatus>,
    /// 上次成功同步后的本地指纹；本地变化触发的同步在指纹未变时跳过。
    synced: Mutex<Option<Fingerprint>>,
    running: tokio::sync::Mutex<()>,
}

impl SyncService {
    pub fn status(&self) -> SyncStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update_status(&self, app: &AppHandle, f: impl FnOnce(&mut SyncStatus)) {
        let status = {
            let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.clone()
        };
        let _ = app.emit(STATUS_EVENT, status);
    }
}

/// 立即同步（托盘菜单、设置窗口、保存配置后）。
pub fn request(app: &AppHandle) {
    if let Some(svc) = app.try_state::<SyncService>() {
        svc.manual.notify_one();
    }
}

/// 本地条目有变化，防抖后同步。
pub fn mark_dirty(app: &AppHandle) {
    if let Some(svc) = app.try_state::<SyncService>() {
        svc.dirty.notify_one();
    }
}

pub fn is_enabled(app: &AppHandle) -> bool {
    load_config(&app.state::<Store>()).enabled
}

pub fn spawn(app: AppHandle) {
    let svc = SyncService::default();
    let last = app
        .state::<Store>()
        .get_setting(LAST_SYNC_KEY)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok());
    {
        let mut s = svc.status.lock().unwrap_or_else(|e| e.into_inner());
        s.last_sync_at = last;
        s.state = if is_enabled(&app) { SyncState::Idle } else { SyncState::Disabled };
    }
    app.manage(svc);

    tauri::async_runtime::spawn(async move {
        // 启动后先同步一次
        let mut first = true;
        loop {
            let config = load_config(&app.state::<Store>());
            let svc = app.state::<SyncService>();
            let from_dirty = if first {
                first = false;
                false
            } else {
                let interval = Duration::from_secs(u64::from(config.interval_mins.max(5)) * 60);
                tokio::select! {
                    _ = tokio::time::sleep(interval) => false,
                    _ = svc.manual.notified() => false,
                    _ = svc.dirty.notified() => {
                        tokio::time::sleep(DEBOUNCE).await;
                        true
                    }
                }
            };

            let config = load_config(&app.state::<Store>());
            if !config.enabled {
                svc.update_status(&app, |s| s.state = SyncState::Disabled);
                continue;
            }
            if from_dirty {
                let fp = app.state::<Store>().sync_fingerprint().ok();
                if fp.is_some() && fp == *svc.synced.lock().unwrap_or_else(|e| e.into_inner()) {
                    continue;
                }
            }
            run(&app).await;
        }
    });
}

async fn run(app: &AppHandle) {
    let svc = app.state::<SyncService>();
    let _guard = svc.running.lock().await;
    svc.update_status(app, |s| s.state = SyncState::Syncing);

    let result = match tokio::time::timeout(RUN_TIMEOUT, run_once(app)).await {
        Ok(r) => r.map_err(friendly),
        Err(_) => Err(anyhow::anyhow!("同步超时")),
    };
    match result {
        Ok(report) => {
            let now = now_ms();
            let _ = app.state::<Store>().put_setting(LAST_SYNC_KEY, &now.to_string());
            *svc.synced.lock().unwrap_or_else(|e| e.into_inner()) = app.state::<Store>().sync_fingerprint().ok();
            log::info!(
                "同步完成：上传 {}，下载 {}，删除 {}",
                report.uploaded,
                report.downloaded,
                report.deleted
            );
            svc.update_status(app, |s| {
                s.state = SyncState::Idle;
                s.last_sync_at = Some(now);
                s.last_error = None;
                s.last_report = Some(report);
            });
            if report.changed_local() {
                events::clips_changed(app);
            }
        }
        Err(e) => {
            log::warn!("同步失败: {e:#}");
            svc.update_status(app, |s| {
                s.state = SyncState::Error;
                s.last_error = Some(format!("{e:#}"));
            });
        }
    }
}

async fn run_once(app: &AppHandle) -> anyhow::Result<Report> {
    let store = app.state::<Store>();
    let config = load_config(&store).validated().map_err(anyhow::Error::msg)?;
    let secrets = load_secrets();
    let op = operator(&config, &secrets)?;
    let cipher = engine::open(&op, passphrase(&config, &secrets)?.as_deref()).await?;
    let paths = app.state::<AppPaths>();
    let sync = engine::Sync {
        op: &op,
        store: &store,
        images_dir: &paths.images_dir,
        cipher: cipher.as_ref(),
        now: now_ms(),
    };
    let (report, removed_images) = sync.run().await?;
    janitor::remove_images(&paths, &removed_images);
    Ok(report)
}

/// 保存配置与凭据，并立即按新配置同步一次。
pub fn save(app: &AppHandle, config: SyncConfig, secrets: &SecretsInput) -> anyhow::Result<()> {
    let config = if config.enabled {
        config.validated().map_err(anyhow::Error::msg)?
    } else {
        config
    };
    save_secrets(secrets)?;
    if config.enabled && config.encrypt && load_secrets().passphrase.is_none() {
        bail!("已开启端到端加密，请填写同步口令");
    }
    save_config(&app.state::<Store>(), &config)?;
    // 配置变化后之前的指纹失效
    *app.state::<SyncService>().synced.lock().unwrap_or_else(|e| e.into_inner()) = None;
    log::info!("云同步配置已保存（{}）", if config.enabled { "已启用" } else { "未启用" });
    crate::tray::sync_cloud(app, config.enabled);
    if !config.enabled {
        app.state::<SyncService>().update_status(app, |s| s.state = SyncState::Disabled);
    }
    request(app);
    Ok(())
}

/// 测试连接：写入、读取并删除一个探测文件，再检查云端已有数据的加密状态与口令。
pub async fn test(config: SyncConfig, input: &SecretsInput) -> anyhow::Result<String> {
    let config = config.validated().map_err(anyhow::Error::msg)?;
    let secrets = merge_secrets(load_secrets(), input);
    let op = operator(&config, &secrets)?;
    let check = async {
        let probe = format!(".anypaste-probe-{}", uuid::Uuid::new_v4());
        op.write(&probe, b"ok".to_vec()).await.context("写入测试文件失败")?;
        let back = op.read(&probe).await.context("读取测试文件失败")?.to_vec();
        op.delete(&probe).await.context("删除测试文件失败")?;
        if back != b"ok" {
            bail!("读回的内容与写入不一致");
        }
        let state = engine::inspect(&op).await?;
        let pass = passphrase(&config, &secrets)?;
        Ok::<_, anyhow::Error>(match (state, pass) {
            (RemoteState::Empty, _) => "连接成功，可读写；云端尚无数据".to_string(),
            (RemoteState::Plain, None) => "连接成功，可读写；云端已有未加密的数据".to_string(),
            (RemoteState::Plain, Some(_)) => bail!("云端已有未加密的数据，不能直接改为加密。如需加密，请换一个新的远端目录"),
            (RemoteState::Encrypted, None) => bail!("云端数据已加密，请开启端到端加密并填写同步口令"),
            (RemoteState::Encrypted, Some(p)) => {
                engine::open(&op, Some(&p)).await?;
                "连接成功，可读写；同步口令正确".to_string()
            }
        })
    };
    tokio::time::timeout(TEST_TIMEOUT, check)
        .await
        .map_err(|_| anyhow::anyhow!("连接超时"))?
        .map_err(friendly)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validation() {
        let ok = SyncConfig { webdav_endpoint: " https://dav.example.com/dav/ ".into(), ..Default::default() };
        assert_eq!(ok.validated().unwrap().webdav_endpoint, "https://dav.example.com/dav/");
        assert!(SyncConfig::default().validated().is_err());
        let s3 = SyncConfig { backend: Backend::S3, s3_endpoint: "https://s3.example.com".into(), ..Default::default() };
        assert!(s3.clone().validated().is_err());
        let s3 = SyncConfig { s3_bucket: "b".into(), s3_access_key_id: "k".into(), ..s3 };
        assert!(s3.validated().is_ok());
        let bad_interval = SyncConfig { interval_mins: 1, webdav_endpoint: "https://x".into(), ..Default::default() };
        assert!(bad_interval.validated().is_err());
    }

    #[test]
    fn friendly_maps_auth_errors() {
        let e = anyhow::Error::new(opendal::Error::new(opendal::ErrorKind::PermissionDenied, "401")).context("读取失败");
        assert_eq!(friendly(e).to_string(), "认证失败：请检查用户名、密码或访问密钥");
        let other = anyhow::anyhow!("其他错误");
        assert_eq!(friendly(other).to_string(), "其他错误");
    }

    #[test]
    fn secrets_merge() {
        let stored = Secrets { password: Some("old".into()), secret_key: None, passphrase: Some("p".into()) };
        let input = SecretsInput { password: Some("new".into()), secret_key: None, passphrase: Some(String::new()) };
        let m = merge_secrets(stored, &input);
        assert_eq!(m.password.as_deref(), Some("new"));
        assert_eq!(m.secret_key, None);
        assert_eq!(m.passphrase, None);
    }
}
