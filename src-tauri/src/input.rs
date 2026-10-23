//! 前台窗口记录与模拟按键（Windows）。

use std::{
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::HWND,
    System::Threading::GetCurrentProcessId,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU,
            VK_RWIN, VK_SHIFT, VK_V,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow, SetForegroundWindow, ShowWindow,
            SW_RESTORE,
        },
    },
};

const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN];

/// 当前前台窗口句柄（不区分进程）。
pub fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

/// 当前前台窗口句柄；属于 AnyPaste 自身的窗口不作为粘贴目标。
pub fn foreground_window() -> Option<isize> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (pid != GetCurrentProcessId()).then_some(hwnd.0 as isize)
    }
}

/// 把焦点还给目标窗口并发送 Ctrl+V。目标以管理员权限运行时按键会被系统拦截，内容仍留在剪贴板中。
pub fn paste_into(target: isize) -> anyhow::Result<()> {
    let hwnd = HWND(target as *mut _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            anyhow::bail!("目标窗口已关闭");
        }
        release_modifiers();
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        if !wait_until(Duration::from_millis(300), || GetForegroundWindow() == hwnd) {
            log::warn!("目标窗口未能获得焦点，仍尝试粘贴");
        }
    }
    send(&[
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ])
}

/// 向前台窗口模拟一次复制。`insert` 为 true 时发送 Ctrl+Insert：终端里没有选中内容时
/// Ctrl+C 会中断正在运行的命令，而 Ctrl+Insert 在终端、编辑器和浏览器中都只表示复制。
pub fn send_copy(insert: bool) -> anyhow::Result<()> {
    // 热键本身的 Shift 若仍按着，Ctrl+C 会变成 Ctrl+Shift+C 并再次触发热键
    release_modifiers();
    let vk = if insert { VK_INSERT } else { VK_C };
    send(&[key(VK_CONTROL, false), key(vk, false), key(vk, true), key(VK_CONTROL, true)])
}

/// 等待用户松开修饰键（如 Shift+Enter 中的 Shift），超时后补发抬起事件，避免 Ctrl+V 变成其他组合键。
fn release_modifiers() {
    let held = || MODIFIERS.iter().any(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0);
    if wait_until(Duration::from_millis(400), || !held()) {
        return;
    }
    let ups: Vec<INPUT> = MODIFIERS
        .iter()
        .filter(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0)
        .map(|vk| key(*vk, true))
        .collect();
    if let Err(e) = send(&ups) {
        log::warn!("释放修饰键失败: {e}");
    }
}

/// 每 10ms 检查一次条件，满足时返回 true，超时返回 false。
pub fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if done() {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    // Insert 是扩展键，缺少标志时可能被识别为小键盘的 0/Ins
    if vk == VK_INSERT {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> anyhow::Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        anyhow::bail!("模拟按键失败（{sent}/{}）", inputs.len());
    }
    Ok(())
}
