//! 接管 Win+V：系统资源管理器占用了该热键，需要在注册表 `DisabledHotkeys` 中加入 `V`
//! 并重启资源管理器后，本程序才能注册它。

use std::{io, os::windows::process::CommandExt, process::Command};

use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const VALUE: &str = "DisabledHotkeys";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 注册表中是否已让出系统的 Win+V。
pub fn is_released() -> bool {
    read().is_ok_and(|v| v.chars().any(|c| c.eq_ignore_ascii_case(&'v')))
}

/// 在 `DisabledHotkeys` 中加入或移除 `V`，保留其他字符不变。
pub fn set_released(release: bool) -> io::Result<()> {
    let current = read().unwrap_or_default();
    let next = with_v(&current, release);
    if next == current {
        return Ok(());
    }
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(KEY)?;
    if next.is_empty() {
        match key.delete_value(VALUE) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    } else {
        key.set_value(VALUE, &next)?;
    }
    log::info!("已{}系统 Win+V（需重启资源管理器生效）", if release { "让出" } else { "恢复" });
    Ok(())
}

/// 重启资源管理器使注册表修改生效，期间任务栏会短暂消失。
pub fn restart_explorer() -> io::Result<()> {
    Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    Command::new("explorer.exe").spawn()?;
    log::info!("已重启资源管理器");
    Ok(())
}

fn read() -> io::Result<String> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEY)?.get_value(VALUE)
}

fn with_v(current: &str, release: bool) -> String {
    let without: String = current.chars().filter(|c| !c.eq_ignore_ascii_case(&'v')).collect();
    if release {
        without + "V"
    } else {
        without
    }
}

#[cfg(test)]
mod tests {
    use super::with_v;

    #[test]
    fn adds_and_removes_only_v() {
        assert_eq!(with_v("", true), "V");
        assert_eq!(with_v("QS", true), "QSV");
        assert_eq!(with_v("QvS", true), "QSV");
        assert_eq!(with_v("QVS", false), "QS");
        assert_eq!(with_v("V", false), "");
        assert_eq!(with_v("", false), "");
    }
}
