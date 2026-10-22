//! 用户设置：整体以 JSON 存在 settings 表的一行中，缺失字段使用默认值，便于后续增加设置项。

use std::sync::{RwLock, RwLockReadGuard};

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::Shortcut;

use crate::store::Store;

const KEY: &str = "app";

const MIN_TTL_SECS: u64 = 60;
const MAX_TTL_SECS: u64 = 30 * 24 * 3600;
const MAX_TEMP_ITEMS: usize = 100_000;
const MIN_ITEM_BYTES: usize = 64 * 1024;
const MAX_ITEM_BYTES: usize = 100 * 1024 * 1024;
const MAX_LIST_LEN: usize = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PopupPosition {
    #[default]
    Cursor,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 关闭设置窗口时只隐藏到托盘；关闭此项则直接退出程序。
    pub close_to_tray: bool,
    pub paused: bool,
    pub theme: Theme,
    pub popup_position: PopupPosition,
    /// 回车默认以纯文本粘贴（Shift+Enter 反之）。
    pub plain_paste_default: bool,
    /// 热键，使用 global-shortcut 的写法，如 `alt+v`。
    pub hotkey_open: String,
    pub hotkey_permanent: String,
    /// 接管 Win+V：在注册表中让出系统热键，并注册为打开剪贴板历史。
    pub take_over_win_v: bool,
    pub notify_permanent: bool,
    /// 永久复制时改用 Ctrl+Insert 的程序。
    pub terminal_apps: Vec<String>,
    /// 来自这些程序的复制不记录（永久复制除外）。
    pub ignored_apps: Vec<String>,
    pub temp_ttl_secs: u64,
    /// 0 表示不限制。
    pub temp_max_items: usize,
    pub max_item_bytes: usize,
    pub capture_text: bool,
    pub capture_rich: bool,
    pub capture_image: bool,
    pub capture_files: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            paused: false,
            theme: Theme::System,
            popup_position: PopupPosition::Cursor,
            plain_paste_default: false,
            hotkey_open: "alt+v".into(),
            hotkey_permanent: "ctrl+shift+c".into(),
            take_over_win_v: false,
            notify_permanent: true,
            terminal_apps: [
                "WindowsTerminal.exe",
                "OpenConsole.exe",
                "conhost.exe",
                "cmd.exe",
                "powershell.exe",
                "pwsh.exe",
                "wezterm-gui.exe",
                "alacritty.exe",
                "mintty.exe",
            ]
            .map(String::from)
            .to_vec(),
            ignored_apps: ["KeePass.exe", "KeePassXC.exe", "1Password.exe", "Bitwarden.exe"]
                .map(String::from)
                .to_vec(),
            temp_ttl_secs: 24 * 3600,
            temp_max_items: 500,
            max_item_bytes: 10 * 1024 * 1024,
            capture_text: true,
            capture_rich: true,
            capture_image: true,
            capture_files: true,
        }
    }
}

/// 采集与清理用到的数值设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSettings {
    pub ttl_ms: i64,
    pub max_temp_items: usize,
    pub max_item_bytes: usize,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Settings::default().capture()
    }
}

impl Settings {
    pub fn capture(&self) -> CaptureSettings {
        CaptureSettings {
            ttl_ms: (self.temp_ttl_secs as i64).saturating_mul(1000),
            max_temp_items: self.temp_max_items,
            max_item_bytes: self.max_item_bytes,
        }
    }

    pub fn is_ignored(&self, process: &str) -> bool {
        contains(&self.ignored_apps, process)
    }

    pub fn is_terminal(&self, process: &str) -> bool {
        contains(&self.terminal_apps, process)
    }

    /// 规范化列表与热键写法，并校验取值范围。错误信息直接展示给用户。
    pub fn validated(mut self) -> Result<Self, String> {
        if !(MIN_TTL_SECS..=MAX_TTL_SECS).contains(&self.temp_ttl_secs) {
            return Err("保留时长需在 1 分钟到 30 天之间".into());
        }
        if self.temp_max_items > MAX_TEMP_ITEMS {
            return Err(format!("临时条目上限不能超过 {MAX_TEMP_ITEMS}"));
        }
        if !(MIN_ITEM_BYTES..=MAX_ITEM_BYTES).contains(&self.max_item_bytes) {
            return Err("单条内容上限需在 64 KB 到 100 MB 之间".into());
        }
        self.hotkey_open = normalize_hotkey(&self.hotkey_open, "打开剪贴板历史")?;
        self.hotkey_permanent = normalize_hotkey(&self.hotkey_permanent, "永久复制")?;
        if self.hotkey_open == self.hotkey_permanent {
            return Err("两个热键不能相同".into());
        }
        self.terminal_apps = normalize_list(self.terminal_apps)?;
        self.ignored_apps = normalize_list(self.ignored_apps)?;
        Ok(self)
    }
}

fn contains(list: &[String], process: &str) -> bool {
    list.iter().any(|p| p.eq_ignore_ascii_case(process))
}

fn normalize_hotkey(raw: &str, name: &str) -> Result<String, String> {
    let s = raw.trim().to_ascii_lowercase().replace(' ', "");
    let shortcut: Shortcut = s.parse().map_err(|_| format!("{name}的热键“{raw}”无效"))?;
    if shortcut.mods.is_empty() {
        return Err(format!("{name}的热键需要包含 Ctrl、Alt、Shift 或 Win"));
    }
    Ok(s)
}

/// 去掉首尾空白与空项，按不区分大小写去重。
fn normalize_list(list: Vec<String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for item in list {
        let item = item.trim().to_string();
        if !item.is_empty() && !contains(&out, &item) {
            out.push(item);
        }
    }
    if out.len() > MAX_LIST_LEN {
        return Err(format!("程序列表最多 {MAX_LIST_LEN} 项"));
    }
    Ok(out)
}

/// 读取设置；内容损坏时记录日志并使用默认值，不阻止启动。
pub fn load(store: &Store) -> Settings {
    let raw = match store.get_setting(KEY) {
        Ok(Some(raw)) => raw,
        Ok(None) => return Settings::default(),
        Err(e) => {
            log::warn!("读取设置失败，使用默认值: {e}");
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&raw).map_err(|e| e.to_string()).and_then(Settings::validated) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("设置内容无效，使用默认值: {e}");
            Settings::default()
        }
    }
}

pub fn save(store: &Store, settings: &Settings) -> anyhow::Result<()> {
    store.put_setting(KEY, &serde_json::to_string(settings)?)?;
    Ok(())
}

/// 内存中的当前设置，修改时整体替换。
pub struct SettingsState(RwLock<Settings>);

impl SettingsState {
    pub fn new(settings: Settings) -> Self {
        Self(RwLock::new(settings))
    }

    pub fn get(&self) -> RwLockReadGuard<'_, Settings> {
        self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, settings: Settings) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = settings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert_eq!(Settings::default().validated().unwrap(), Settings::default());
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{"paused":true,"theme":"dark"}"#).unwrap();
        assert!(s.paused);
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.hotkey_open, "alt+v");
    }

    #[test]
    fn validation_normalizes_and_rejects() {
        let s = Settings {
            hotkey_open: " Ctrl + Alt + V ".into(),
            ignored_apps: vec![" a.exe ".into(), "A.EXE".into(), "".into(), "b.exe".into()],
            ..Default::default()
        }
        .validated()
        .unwrap();
        assert_eq!(s.hotkey_open, "ctrl+alt+v");
        assert_eq!(s.ignored_apps, vec!["a.exe".to_string(), "b.exe".to_string()]);

        let bad = |f: fn(&mut Settings)| {
            let mut s = Settings::default();
            f(&mut s);
            s.validated().is_err()
        };
        assert!(bad(|s| s.temp_ttl_secs = 10));
        assert!(bad(|s| s.max_item_bytes = 1));
        assert!(bad(|s| s.hotkey_open = "v".into()));
        assert!(bad(|s| s.hotkey_open = "ctrl+nonsense".into()));
        assert!(bad(|s| s.hotkey_permanent = "ALT+V".into()));
    }

    #[test]
    fn load_and_save_round_trip() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(load(&store), Settings::default());
        let s = Settings { temp_ttl_secs: 3600, theme: Theme::Light, ..Default::default() };
        save(&store, &s).unwrap();
        assert_eq!(load(&store), s);

        store.put_setting(KEY, "not json").unwrap();
        assert_eq!(load(&store), Settings::default());
    }

    #[test]
    fn process_lists_ignore_case() {
        let s = Settings::default();
        assert!(s.is_terminal("windowsterminal.exe"));
        assert!(s.is_ignored("keepassxc.exe"));
        assert!(!s.is_ignored("Code.exe"));
    }
}
