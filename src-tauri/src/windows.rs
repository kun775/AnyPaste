use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::{events, state::AppState};

pub const POPUP: &str = "popup";
pub const SETTINGS: &str = "settings";

/// 弹窗与光标之间的间距（逻辑像素）。
const CURSOR_GAP: f64 = 4.0;

/// 弹出窗口在配置中预先创建且常驻隐藏，保证热键唤出时足够快。
pub fn setup_popup(app: &AppHandle) -> tauri::Result<()> {
    let popup = app
        .get_webview_window(POPUP)
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let handle = popup.clone();
    popup.on_window_event(move |event| {
        // 失去焦点即隐藏，与系统 Win+V 的行为一致
        if let WindowEvent::Focused(false) = event {
            let _ = handle.hide();
        }
    });
    Ok(())
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
pub fn show_popup(app: &AppHandle, target: Option<isize>) {
    let Some(popup) = app.get_webview_window(POPUP) else {
        return;
    };
    app.state::<AppState>().set_paste_target(target);
    if let Err(e) = place_near_cursor(app, &popup) {
        log::warn!("定位弹出列表失败: {e}");
        let _ = popup.center();
    }
    events::popup_shown(app);
    if let Err(e) = popup.show().and_then(|_| popup.set_focus()) {
        log::warn!("显示弹出列表失败: {e}");
    }
}

pub fn hide_popup(app: &AppHandle) {
    if let Some(popup) = app.get_webview_window(POPUP) {
        let _ = popup.hide();
    }
}

/// 优先放在光标右下方，放不下时翻到另一侧，最后限制在光标所在显示器的工作区内。
fn place_near_cursor(app: &AppHandle, popup: &WebviewWindow) -> tauri::Result<()> {
    let cursor = app.cursor_position()?;
    let Some(monitor) = app.monitor_from_point(cursor.x, cursor.y)? else {
        return popup.center();
    };
    // 目标显示器的缩放比例可能与当前不同，按逻辑尺寸换算
    let size = popup.outer_size()?.to_logical::<f64>(popup.scale_factor()?);
    let scale = monitor.scale_factor();
    let (w, h) = ((size.width * scale).round() as i32, (size.height * scale).round() as i32);
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

/// 设置窗口按需创建，关闭即销毁以释放 WebView 内存；应用仍在托盘中运行。
pub fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("AnyPaste 设置")
        .inner_size(860.0, 600.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        log::error!("创建设置窗口失败: {e}");
    }
}
