use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

use crate::{prefs, settings::Settings, windows};

const TOOLTIP: &str = "AnyPaste";
const TOOLTIP_PAUSED: &str = "AnyPaste（已暂停记录）";

/// 需要随设置变化更新的菜单项。
pub struct TrayMenu {
    open: MenuItem<Wry>,
    pause: CheckMenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

pub fn create(app: &AppHandle, settings: &Settings, autostart_on: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开剪贴板历史", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "暂停记录", true, settings.paused, None::<&str>)?;
    // TODO(M5): 接入云同步后启用
    let sync_now = MenuItem::with_id(app, "sync", "立即同步", false, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "开机启动", true, autostart_on, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 AnyPaste", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &pause,
            &sync_now,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::InvalidIcon(std::io::Error::other("缺少默认图标")))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            // 从托盘打开时前台是任务栏，没有可粘贴的目标窗口，选中条目只复制
            "open" => windows::show_popup(app, None),
            "pause" => toggle_pause(app),
            "autostart" => toggle_autostart(app),
            "settings" => windows::show_settings(app),
            "quit" => {
                log::info!("从托盘退出");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::show_popup(tray.app_handle(), None);
            }
        })
        .build(app)?;

    app.manage(TrayMenu { open, pause, autostart });
    sync(app, settings);
    Ok(())
}

/// 让菜单勾选状态、热键提示和图标提示与当前设置一致。
pub fn sync(app: &AppHandle, settings: &Settings) {
    let Some(menu) = app.try_state::<TrayMenu>() else {
        return;
    };
    let _ = menu.pause.set_checked(settings.paused);
    let _ = menu.open.set_accelerator(Some(settings.hotkey_open.as_str()));
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(if settings.paused { TOOLTIP_PAUSED } else { TOOLTIP }));
    }
}

pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    if let Some(menu) = app.try_state::<TrayMenu>() {
        let _ = menu.autostart.set_checked(enabled);
    }
}

fn toggle_pause(app: &AppHandle) {
    let mut next = prefs::current(app);
    next.paused = !next.paused;
    if let Err(e) = prefs::update(app, next) {
        log::warn!("切换暂停记录失败: {e:#}");
    }
}

fn toggle_autostart(app: &AppHandle) {
    let enabled = !prefs::autostart_enabled(app);
    if let Err(e) = prefs::set_autostart(app, enabled) {
        log::warn!("切换开机启动失败: {e:#}");
        sync_autostart(app, !enabled);
    }
}
