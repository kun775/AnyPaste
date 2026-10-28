use std::time::Duration;

use tauri::{
    webview::PageLoadEvent, AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::{
    events,
    settings::{PopupPosition, SettingsState},
    state::AppState,
};

pub const POPUP: &str = "popup";
pub const SETTINGS: &str = "settings";

/// 弹窗与光标之间的间距（逻辑像素）。
const CURSOR_GAP: f64 = 4.0;
/// 弹窗的逻辑尺寸。
const POPUP_SIZE: (f64, f64) = (380.0, 520.0);
/// 失焦或移动后等待窗口状态稳定的时间。
const FOCUS_SETTLE: Duration = Duration::from_millis(150);

/// 弹窗是否仍是前台窗口（拖动过程中保持为前台）。
fn popup_is_foreground(app: &AppHandle) -> bool {
    #[cfg(windows)]
    if let Some(Ok(hwnd)) = app.get_webview_window(POPUP).map(|p| p.hwnd()) {
        return crate::input::foreground_hwnd() == hwnd.0 as isize;
    }
    #[cfg(not(windows))]
    let _ = app;
    false
}

/// 热键触发：弹窗可见时隐藏，否则记录当前前台窗口后弹出。
#[cfg(windows)]
pub fn toggle_popup(app: &AppHandle) {
    let visible = app
        .get_webview_window(POPUP)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_popup(app);
    } else {
        show_popup(app, crate::input::foreground_window());
    }
}

/// 在光标附近显示弹窗。`target` 为粘贴目标窗口，为空时选中条目只复制不粘贴。
/// 弹窗不存在（首次打开或空闲后已销毁）时先创建，页面加载完成后再显示。
pub fn show_popup(app: &AppHandle, target: Option<isize>) {
    let state = app.state::<AppState>();
    state.set_paste_target(target);
    // 取消尚未执行的空闲销毁
    state.bump_popup_generation();
    match app.get_webview_window(POPUP) {
        Some(popup) => {
            events::popup_shown(app);
            present(app, &popup);
        }
        None => create_popup(app, true),
    }
}

/// 设置为常驻时在后台预先创建弹窗（隐藏），首次打开也无需等待 WebView 启动。
pub fn preload_popup(app: &AppHandle) {
    if app.get_webview_window(POPUP).is_none() {
        create_popup(app, false);
    }
}

/// “空闲后释放”设置变化：改为常驻时预先创建；改为释放时，若弹窗正隐藏则按新时长计时。
pub fn on_release_setting_changed(app: &AppHandle, release_secs: u64) {
    if release_secs == 0 {
        preload_popup(app);
    } else if app
        .get_webview_window(POPUP)
        .is_some_and(|p| !p.is_visible().unwrap_or(true))
    {
        schedule_release(app);
    }
}

fn present(app: &AppHandle, popup: &WebviewWindow) {
    let position = app.state::<SettingsState>().get().popup_position;
    let placed = match position {
        PopupPosition::Cursor => place_near_cursor(app, popup),
        PopupPosition::Center => popup.center(),
    };
    if let Err(e) = placed {
        log::warn!("定位弹出列表失败: {e}");
        let _ = popup.center();
    }
    if let Err(e) = popup.show().and_then(|_| popup.set_focus()) {
        log::warn!("显示弹出列表失败: {e}");
    }
}

/// 创建弹窗；`show` 为 true 时在页面首次加载完成后显示，避免先出现空白窗口。
///
/// 在主线程上创建 WebView2 时会嵌套处理消息，期间再次按下热键或点击托盘会重入到这里，
/// 而同名窗口要等创建完成才登记，重复创建会留下游离的弹窗。所以创建进行中只记下显示请求。
fn create_popup(app: &AppHandle, show: bool) {
    let state = app.state::<AppState>();
    if show {
        state.request_popup_show();
    }
    if !state.begin_popup_creation() {
        return;
    }
    let handle = app.clone();
    let built = WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App("popup.html".into()))
        .title("AnyPaste")
        .inner_size(POPUP_SIZE.0, POPUP_SIZE.1)
        .resizable(false)
        .decorations(false)
        .visible(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .shadow(true)
        // 只响应创建期间的显示请求；开发模式热更新会再次触发加载事件，此时请求已被取走
        .on_page_load(move |popup, payload| {
            if payload.event() == PageLoadEvent::Finished && handle.state::<AppState>().take_popup_show() {
                present(&handle, &popup);
            }
        })
        .build();
    state.end_popup_creation();
    match built {
        Ok(popup) => {
            let handle = app.clone();
            popup.on_window_event(move |event| match event {
                // 失去焦点即隐藏，与系统 Win+V 的行为一致。但开始拖动窗口时也会短暂失焦，
                // 所以稍后确认前台确实换成了其他窗口才隐藏
                WindowEvent::Focused(false) => {
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(FOCUS_SETTLE).await;
                        if !popup_is_foreground(&handle) {
                            hide_popup(&handle);
                        }
                    });
                }
                // 拖动结束后把焦点还给弹窗，之后点击其他窗口时才能再次触发失焦隐藏
                WindowEvent::Moved(_) => {
                    let generation = handle.state::<AppState>().bump_move_generation();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(FOCUS_SETTLE).await;
                        if handle.state::<AppState>().move_generation() == generation && popup_is_foreground(&handle) {
                            if let Some(p) = handle.get_webview_window(POPUP) {
                                let _ = p.set_focus();
                            }
                        }
                    });
                }
                _ => {}
            });
        }
        Err(e) => {
            // 不留下显示请求，否则下次后台预创建会意外弹出
            state.take_popup_show();
            log::error!("创建弹出列表失败: {e}");
        }
    }
}

pub fn hide_popup(app: &AppHandle) {
    let Some(popup) = app.get_webview_window(POPUP) else {
        return;
    };
    let _ = popup.hide();
    schedule_release(app);
}

/// 隐藏后若一直没有再次打开，到期销毁弹窗，WebView2 进程随之退出并释放内存。
/// 设置为常驻（0）时不释放。
fn schedule_release(app: &AppHandle) {
    let secs = app.state::<SettingsState>().get().popup_release_secs;
    if secs == 0 {
        return;
    }
    let generation = app.state::<AppState>().popup_generation();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(secs)).await;
        // 期间重新打开过，或已改为常驻
        if app.state::<AppState>().popup_generation() != generation
            || app.state::<SettingsState>().get().popup_release_secs == 0
        {
            return;
        }
        if let Some(popup) = app.get_webview_window(POPUP) {
            if !popup.is_visible().unwrap_or(true) {
                match popup.destroy() {
                    Ok(()) => log::info!("弹出列表空闲，已释放"),
                    Err(e) => log::warn!("释放弹出列表失败: {e}"),
                }
            }
        }
    });
}

/// 优先放在光标右下方，放不下时翻到另一侧，最后限制在光标所在显示器的工作区内。
fn place_near_cursor(app: &AppHandle, popup: &WebviewWindow) -> tauri::Result<()> {
    let cursor = app.cursor_position()?;
    let Some(monitor) = app.monitor_from_point(cursor.x, cursor.y)? else {
        return popup.center();
    };
    // 目标显示器的缩放比例可能与当前不同，按逻辑尺寸换算
    let scale = monitor.scale_factor();
    let (w, h) = ((POPUP_SIZE.0 * scale).round() as i32, (POPUP_SIZE.1 * scale).round() as i32);
    let gap = (CURSOR_GAP * scale).round() as i32;
    let area = monitor.work_area();
    let (left, top) = (area.position.x, area.position.y);
    let (right, bottom) = (left + area.size.width as i32, top + area.size.height as i32);
    let (cx, cy) = (cursor.x.round() as i32, cursor.y.round() as i32);

    let mut x = cx + gap;
    if x + w > right {
        x = cx - gap - w;
    }
    let mut y = cy + gap;
    if y + h > bottom {
        y = cy - gap - h;
    }
    let x = x.clamp(left, (right - w).max(left));
    let y = y.clamp(top, (bottom - h).max(top));
    popup.set_position(PhysicalPosition::new(x, y))
}

/// 设置窗口按需创建，关闭即销毁以释放 WebView 内存，应用仍在托盘中运行；
/// 若关闭了“关闭窗口时最小化到托盘”，关闭设置窗口即退出程序。
pub fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("AnyPaste 设置")
        .inner_size(880.0, 620.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .build();
    match built {
        Ok(win) => {
            let handle = app.clone();
            win.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { .. } = event {
                    if !handle.state::<SettingsState>().get().close_to_tray {
                        log::info!("关闭设置窗口，按设置退出程序");
                        handle.exit(0);
                    }
                }
            });
        }
        Err(e) => log::error!("创建设置窗口失败: {e}"),
    }
}
