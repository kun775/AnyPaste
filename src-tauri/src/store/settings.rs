use rusqlite::params;

use super::Store;

const TEMP_TTL_SECS: &str = "temp_ttl_secs";
const TEMP_MAX_ITEMS: &str = "temp_max_items";
const MAX_ITEM_BYTES: &str = "max_item_bytes";

/// 采集与清理相关的设置，每次使用时从数据库读取，修改后无需重启。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSettings {
    /// 临时条目从最后一次复制起的保留时长。
    pub ttl_ms: i64,
    /// 临时条目数量上限，0 表示不限制。
    pub max_temp_items: usize,
    /// 单条内容大小上限（文本类按 UTF-8 字节，图片按 PNG 文件大小）。
    pub max_item_bytes: usize,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            ttl_ms: 24 * 3600 * 1000,
            max_temp_items: 500,
            max_item_bytes: 10 * 1024 * 1024,
        }
    }
}

impl Store {
    pub fn capture_settings(&self) -> rusqlite::Result<CaptureSettings> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE key IN (?1, ?2, ?3)")?;
        let rows = stmt.query_map(params![TEMP_TTL_SECS, TEMP_MAX_ITEMS, MAX_ITEM_BYTES], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;

        let mut s = CaptureSettings::default();
        for row in rows {
            let (key, value) = row?;
            // 非法值回退到默认值，避免一条坏配置让采集整体失效
            let Ok(n) = value.trim().parse::<u64>() else {
                log::warn!("忽略非法设置 {key}={value}");
                continue;
            };
            match key.as_str() {
                TEMP_TTL_SECS if n > 0 => s.ttl_ms = (n as i64).saturating_mul(1000),
                TEMP_MAX_ITEMS => s.max_temp_items = n as usize,
                MAX_ITEM_BYTES if n > 0 => s.max_item_bytes = n as usize,
                _ => log::warn!("忽略非法设置 {key}={value}"),
            }
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(store: &Store, key: &str, value: &str) {
        store
            .conn()
            .execute("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)", params![key, value])
            .unwrap();
    }

    #[test]
    fn defaults_when_unset() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.capture_settings().unwrap(), CaptureSettings::default());
    }

    #[test]
    fn reads_overrides_and_ignores_invalid() {
        let store = Store::open_in_memory().unwrap();
        set(&store, TEMP_TTL_SECS, "60");
        set(&store, TEMP_MAX_ITEMS, "0");
        set(&store, MAX_ITEM_BYTES, "abc");
        let s = store.capture_settings().unwrap();
        assert_eq!(s.ttl_ms, 60_000);
        assert_eq!(s.max_temp_items, 0);
        assert_eq!(s.max_item_bytes, CaptureSettings::default().max_item_bytes);
    }

    #[test]
    fn zero_ttl_falls_back_to_default() {
        let store = Store::open_in_memory().unwrap();
        set(&store, TEMP_TTL_SECS, "0");
        assert_eq!(store.capture_settings().unwrap().ttl_ms, CaptureSettings::default().ttl_ms);
    }
}
