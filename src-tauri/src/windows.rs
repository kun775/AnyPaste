use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const POPUP: &str = "popup";
pub const SETTINGS: &str = "settings";

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

pub fn show_popup(app: &AppHandle) {
    let Some(popup) = app.get_webview_window(POPUP) else {
        return;
    };
    // TODO(M2): 改为在光标附近弹出
    let result = popup
        .center()
        .and_then(|_| popup.show())
        .and_then(|_| popup.set_focus());
    if let Err(e) = result {
        log::warn!("显示弹出列表失败: {e}");
    }
}

pub fn hide_popup(app: &AppHandle) {
    if let Some(popup) = app.get_webview_window(POPUP) {
        let _ = popup.hide();
    }
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
