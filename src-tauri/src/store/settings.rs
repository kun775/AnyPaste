use rusqlite::{params, OptionalExtension};

use super::Store;

impl Store {
    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
    }

    pub fn put_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// 保留时长修改后，按最后一次复制时间重新计算所有临时条目的过期时间。
    pub fn recompute_expiry(&self, ttl_ms: i64) -> rusqlite::Result<usize> {
        self.conn().execute(
            "UPDATE clips SET expires_at = last_copied_at + ?1 WHERE is_permanent = 0 AND deleted_at IS NULL",
            [ttl_ms],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::clips_tests::text_clip;

    #[test]
    fn put_and_get() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting("k").unwrap(), None);
        store.put_setting("k", "1").unwrap();
        store.put_setting("k", "2").unwrap();
        assert_eq!(store.get_setting("k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn recompute_only_touches_temp() {
        let store = Store::open_in_memory().unwrap();
        let temp = store.upsert_clip(&text_clip("t"), 100, 1000).unwrap().id;
        let perm = store.upsert_clip(&text_clip("p"), 100, 1000).unwrap().id;
        store.set_permanent(perm, true, 100, 1000).unwrap();

        assert_eq!(store.recompute_expiry(60_000).unwrap(), 1);
        let expires = |id: i64| -> Option<i64> {
            store
                .conn()
                .query_row("SELECT expires_at FROM clips WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(expires(temp), Some(60_100));
        assert_eq!(expires(perm), None);
    }
}
