mod clips;
mod list;
mod settings;
mod sync;

use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context};
use rusqlite::Connection;

pub use clips::{ClipContent, ClipKind, NewClip};
pub use list::{ClipItem, Counts, Cursor};
pub use sync::{ClipRecord, Fingerprint, LocalState};
#[cfg(test)]
pub(crate) use sync::tests as sync_tests;
#[cfg(test)]
pub(crate) use clips::tests as clips_tests;

/// 按顺序执行的迁移脚本，版本号记录在 `PRAGMA user_version`。
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_fts.sql"),
    include_str!("migrations/003_pin.sql"),
];

/// SQLite 存储。连接用互斥锁串行化，剪贴板监听、清理线程和前端命令共用。
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("无法打开数据库 {}", path.display()))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> anyhow::Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // 持锁线程 panic 不会破坏 SQLite 自身的一致性，继续使用连接
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn migrate(conn: &mut Connection) -> anyhow::Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let current = usize::try_from(current).unwrap_or(usize::MAX);
    if current > MIGRATIONS.len() {
        bail!(
            "数据库版本 {current} 高于程序支持的 {}，请升级 AnyPaste",
            MIGRATIONS.len()
        );
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = idx + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("执行数据库迁移 {version} 失败"))?;
        tx.pragma_update(None, "user_version", version as i64)?;
        tx.commit()?;
        log::info!("数据库已迁移到版本 {version}");
    }
    Ok(())
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        drop(Store::open(&path).unwrap());
        let store = Store::open(&path).unwrap();
        let version: i64 = store
            .conn()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn rejects_newer_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        assert!(Store::open(&path).is_err());
    }
}
