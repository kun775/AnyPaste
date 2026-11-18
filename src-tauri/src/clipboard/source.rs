use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{CloseHandle, HWND},
        System::{
            DataExchange::GetClipboardOwner,
            Threading::{
                GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
};

/// 当前前台窗口所属进程的文件名，如 `chrome.exe`；取不到时返回 `None`。
pub fn foreground_app() -> Option<String> {
    unsafe { process_name_of_window(GetForegroundWindow(), false) }
}

/// 当前剪贴板内容所属源程序：优先读取剪贴板拥有者窗口（后台复制或快速切窗时仍准确），
/// 取不到或属于本程序自身时回退到通知到达时的前台程序。
pub fn clipboard_source_app(fallback_foreground: Option<String>) -> Option<String> {
    unsafe {
        if let Ok(owner) = GetClipboardOwner() {
            if let Some(name) = process_name_of_window(owner, true) {
                return Some(name);
            }
        }
    }
    fallback_foreground.or_else(foreground_app)
}

unsafe fn process_name_of_window(hwnd: HWND, exclude_self: bool) -> Option<String> {
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || (exclude_self && pid == GetCurrentProcessId()) {
        return None;
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(handle);
    result.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_owned)
}
