use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::{state::AppState, windows};

const TOOLTIP: &str = "AnyPaste";
const TOOLTIP_PAUSED: &str = "AnyPaste（已暂停记录）";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开剪贴板历史", true, Some("Alt+V"))?;
    let pause = CheckMenuItem::with_id(app, "pause", "暂停记录", true, false, None::<&str>)?;
    // 同步（M5）与开机启动（M4）尚未实现，先占位并禁用
    let sync = MenuItem::with_id(app, "sync", "立即同步", false, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "开机启动", false, false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 AnyPaste", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &pause,
            &sync,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &settings,
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
        .on_menu_event(move |app, event| match event.id().as_ref() {
            // 从托盘打开时前台是任务栏，没有可粘贴的目标窗口，选中条目只复制
            "open" => windows::show_popup(app, None),
            "pause" => set_paused(app, pause.is_checked().unwrap_or(false)),
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
    Ok(())
}

fn set_paused(app: &AppHandle, paused: bool) {
    app.state::<AppState>().set_paused(paused);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(if paused { TOOLTIP_PAUSED } else { TOOLTIP }));
    }
    log::info!("记录状态: {}", if paused { "已暂停" } else { "记录中" });
}
