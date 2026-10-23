//! 设置变更的统一入口：校验、生效、持久化并通知各窗口。设置窗口与托盘菜单都经由这里修改设置。

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    events, hotkey, janitor,
    paths::AppPaths,
    settings::{self, Settings, SettingsState},
    state::AppState,
    store::{now_ms, Store},
    tray,
};

pub fn current(app: &AppHandle) -> Settings {
    app.state::<SettingsState>().get().clone()
}

/// 应用新设置。热键注册失败时恢复原热键与注册表，设置保持不变并返回错误。
pub fn update(app: &AppHandle, next: Settings) -> anyhow::Result<Settings> {
    let next = next.validated().map_err(anyhow::Error::msg)?;
    let prev = current(app);
    if next == prev {
        return Ok(next);
    }

    let win_v_changed = prev.take_over_win_v != next.take_over_win_v;
    let hotkeys_changed =
        win_v_changed || prev.hotkey_open != next.hotkey_open || prev.hotkey_permanent != next.hotkey_permanent;
    if hotkeys_changed {
        if let Err(e) = apply_hotkeys(app, &next, win_v_changed) {
            rollback_hotkeys(app, &prev, win_v_changed);
            return Err(e);
        }
    }

    let store = app.state::<Store>();
    if let Err(e) = settings::save(&store, &next) {
        if hotkeys_changed {
            rollback_hotkeys(app, &prev, win_v_changed);
        }
        return Err(e);
    }
    app.state::<SettingsState>().set(next.clone());
    app.state::<AppState>().set_paused(next.paused);
    tray::sync(app, &next);

    let mut clips_changed = false;
    if prev.temp_ttl_secs != next.temp_ttl_secs {
        let n = store.recompute_expiry(next.capture().ttl_ms)?;
        log::info!("保留时长改为 {} 秒，已重算 {n} 条临时条目的过期时间", next.temp_ttl_secs);
        clips_changed = true;
    }
    if prev.temp_ttl_secs != next.temp_ttl_secs || prev.temp_max_items != next.temp_max_items {
        janitor::cleanup(&store, &app.state::<AppPaths>(), &next.capture(), now_ms())?;
        clips_changed = true;
    }
    if prev.popup_release_secs != next.popup_release_secs {
        crate::windows::on_release_setting_changed(app, next.popup_release_secs);
    }
    if prev.paused != next.paused {
        log::info!("记录状态: {}", if next.paused { "已暂停" } else { "记录中" });
    }

    events::settings_changed(app, &next);
    if clips_changed {
        events::clips_changed(app);
    }
    Ok(next)
}

fn apply_hotkeys(app: &AppHandle, next: &Settings, win_v_changed: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    if win_v_changed {
        crate::winv::set_released(next.take_over_win_v)?;
    }
    #[cfg(not(windows))]
    let _ = win_v_changed;
    hotkey::apply(app, next)
}

fn rollback_hotkeys(app: &AppHandle, prev: &Settings, win_v_changed: bool) {
    #[cfg(windows)]
    if win_v_changed {
        if let Err(e) = crate::winv::set_released(prev.take_over_win_v) {
            log::error!("恢复 Win+V 注册表失败: {e}");
        }
    }
    #[cfg(not(windows))]
    let _ = win_v_changed;
    if let Err(e) = hotkey::apply(app, prev) {
        log::error!("恢复原热键失败: {e:#}");
    }
}

pub fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

pub fn set_autostart(app: &AppHandle, enabled: bool) -> anyhow::Result<()> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()?;
    } else {
        launcher.disable()?;
    }
    log::info!("开机启动: {}", if enabled { "开启" } else { "关闭" });
    tray::sync_autostart(app, enabled);
    events::settings_changed(app, &current(app));
    Ok(())
}
