//! 云同步所需的本地数据访问。只有永久条目参与同步；删除以墓碑（deleted_at）记录。
//! 同步时以内容哈希标识条目：不同设备复制了相同内容时自然合并为同一条。

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{clips::Deleted, Store};

/// 云端保存的条目内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipRecord {
    pub kind: String,
    pub content_hash: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub image_file: Option<String>,
    /// PNG 缩略图的 base64。
    pub thumb: Option<String>,
    pub size_bytes: i64,
    pub source_app: Option<String>,
    pub created_at: i64,
    pub last_copied_at: i64,
    pub updated_at: i64,
}

/// 本地条目在同步中的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalState {
    /// 永久条目，参与同步。
    Live,
    /// 已删除的永久条目（墓碑）。
    Deleted,
    /// 临时条目，不参与同步；云端存在同内容的条目时，说明它已被转为临时。
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    pub content_hash: String,
    /// 版本号：墓碑取删除时间，其余取更新时间。
    pub version: i64,
    pub state: LocalState,
}

/// 用于判断自上次同步后是否有需要上传的变化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fingerprint {
    pub count: i64,
    pub max_updated: i64,
}

fn to_record(r: &Row<'_>) -> rusqlite::Result<ClipRecord> {
    Ok(ClipRecord {
        kind: r.get(0)?,
        content_hash: r.get(1)?,
        text: r.get(2)?,
        html: r.get(3)?,
        rtf: r.get(4)?,
        image_file: r.get(5)?,
        thumb: r.get::<_, Option<Vec<u8>>>(6)?.map(|b| STANDARD.encode(b)),
        size_bytes: r.get(7)?,
        source_app: r.get(8)?,
        created_at: r.get(9)?,
        last_copied_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

impl Store {
    pub fn sync_entries(&self) -> rusqlite::Result<Vec<LocalEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT content_hash, is_permanent, deleted_at, updated_at FROM clips")?;
        let rows = stmt.query_map([], |r| {
            let permanent: bool = r.get(1)?;
            let deleted: Option<i64> = r.get(2)?;
            let updated: i64 = r.get(3)?;
            let (state, version) = match deleted {
                Some(at) => (LocalState::Deleted, at),
                None if permanent => (LocalState::Live, updated),
                None => (LocalState::Local, updated),
            };
            Ok(LocalEntry { content_hash: r.get(0)?, version, state })
        })?;
        rows.collect()
    }

    pub fn sync_fingerprint(&self) -> rusqlite::Result<Fingerprint> {
        self.conn().query_row(
            "SELECT COUNT(*), COALESCE(MAX(updated_at), 0) FROM clips
             WHERE is_permanent = 1 OR deleted_at IS NOT NULL",
            [],
            |r| Ok(Fingerprint { count: r.get(0)?, max_updated: r.get(1)? }),
        )
    }

    /// 读取用于上传的永久条目。
    pub fn clip_record(&self, content_hash: &str) -> rusqlite::Result<Option<ClipRecord>> {
        self.conn()
            .query_row(
                "SELECT kind, content_hash, text, html, rtf, image_file, thumb, size_bytes,
                        source_app, created_at, last_copied_at, updated_at
                 FROM clips WHERE content_hash = ?1 AND is_permanent = 1 AND deleted_at IS NULL",
                [content_hash],
                to_record,
            )
            .optional()
    }

    /// 写入云端下载的条目，作为永久条目；本地已有同内容的条目（临时、永久或墓碑）时直接覆盖。
    pub fn apply_remote_record(&self, rec: &ClipRecord) -> rusqlite::Result<()> {
        let thumb = rec.thumb.as_deref().and_then(|t| STANDARD.decode(t).ok());
        self.conn().execute(
            "INSERT INTO clips (uuid, kind, content_hash, text, html, rtf, image_file, thumb, size_bytes,
               source_app, is_permanent, created_at, last_copied_at, expires_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, NULL, ?13)
             ON CONFLICT (content_hash) DO UPDATE SET
               kind = excluded.kind, text = excluded.text, html = excluded.html, rtf = excluded.rtf,
               image_file = excluded.image_file, thumb = excluded.thumb, size_bytes = excluded.size_bytes,
               source_app = excluded.source_app, is_permanent = 1, created_at = excluded.created_at,
               last_copied_at = excluded.last_copied_at, expires_at = NULL,
               updated_at = excluded.updated_at, deleted_at = NULL",
            params![
                uuid::Uuid::new_v4().to_string(),
                rec.kind,
                rec.content_hash,
                rec.text,
                rec.html,
                rec.rtf,
                rec.image_file,
                thumb,
                rec.size_bytes,
                rec.source_app,
                rec.created_at,
                rec.last_copied_at,
                rec.updated_at,
            ],
        )?;
        Ok(())
    }

    /// 云端删除了该条目：删除本地的永久条目或墓碑；同内容的临时条目属于本机，保留。
    pub fn apply_remote_delete(&self, content_hash: &str) -> rusqlite::Result<Deleted> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "DELETE FROM clips WHERE content_hash = ?1 AND (is_permanent = 1 OR deleted_at IS NOT NULL)
             RETURNING image_file",
        )?;
        let rows = stmt.query_map([content_hash], |r| r.get::<_, Option<String>>(0))?;
        let mut out = Deleted::default();
        for row in rows {
            out.rows += 1;
            out.image_files.extend(row?);
        }
        Ok(out)
    }

    /// 同步开启时删除永久条目：清空内容只保留墓碑，等待同步到其他设备。返回需删除的图片。
    pub fn tombstone_clip(&self, id: i64, now: i64) -> rusqlite::Result<Deleted> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let image: Option<Option<String>> = tx
            .query_row("SELECT image_file FROM clips WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        let rows = tx.execute(
            "UPDATE clips SET deleted_at = ?2, updated_at = ?2, text = NULL, html = NULL, rtf = NULL,
               image_file = NULL, thumb = NULL
             WHERE id = ?1",
            params![id, now],
        )?;
        tx.commit()?;
        Ok(Deleted { rows, image_files: image.flatten().into_iter().collect() })
    }

    /// 清理早于 `before` 的墓碑。
    pub fn purge_tombstones(&self, before: i64) -> rusqlite::Result<usize> {
        self.conn().execute("DELETE FROM clips WHERE deleted_at IS NOT NULL AND deleted_at < ?1", [before])
    }

    pub fn is_permanent(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self
            .conn()
            .query_row("SELECT is_permanent FROM clips WHERE id = ?1", [id], |r| r.get::<_, bool>(0))
            .optional()?
            .unwrap_or(false))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::clips_tests::text_clip;

    const TTL: i64 = 1000;

    pub fn perm(store: &Store, text: &str, at: i64) -> i64 {
        let mut c = text_clip(text);
        c.permanent = true;
        store.upsert_clip(&c, at, TTL).unwrap().id
    }

    #[test]
    fn entries_reflect_state() {
        let store = Store::open_in_memory().unwrap();
        perm(&store, "live", 10);
        let dead = perm(&store, "dead", 20);
        store.tombstone_clip(dead, 30).unwrap();
        store.upsert_clip(&text_clip("temp"), 40, TTL).unwrap();

        let mut e = store.sync_entries().unwrap();
        e.sort_by_key(|x| x.version);
        assert_eq!(e[0], LocalEntry { content_hash: "hash-live".into(), version: 10, state: LocalState::Live });
        assert_eq!(e[1], LocalEntry { content_hash: "hash-dead".into(), version: 30, state: LocalState::Deleted });
        assert_eq!(e[2].state, LocalState::Local);
        assert_eq!(store.sync_fingerprint().unwrap(), Fingerprint { count: 2, max_updated: 30 });
    }

    #[test]
    fn record_round_trip_between_stores() {
        let a = Store::open_in_memory().unwrap();
        perm(&a, "shared", 10);
        let rec = a.clip_record("hash-shared").unwrap().unwrap();

        let b = Store::open_in_memory().unwrap();
        b.apply_remote_record(&rec).unwrap();
        assert_eq!(b.clip_record("hash-shared").unwrap().unwrap(), rec);
        b.apply_remote_record(&rec).unwrap();
        assert_eq!(b.sync_entries().unwrap().len(), 1);
    }

    #[test]
    fn remote_record_promotes_local_temp_with_same_content() {
        let a = Store::open_in_memory().unwrap();
        perm(&a, "same", 10);
        let rec = a.clip_record("hash-same").unwrap().unwrap();

        let b = Store::open_in_memory().unwrap();
        b.upsert_clip(&text_clip("same"), 5, TTL).unwrap();
        b.apply_remote_record(&rec).unwrap();
        let entries = b.sync_entries().unwrap();
        assert_eq!(entries, vec![LocalEntry { content_hash: "hash-same".into(), version: 10, state: LocalState::Live }]);
    }

    #[test]
    fn tombstone_hides_and_revives_with_text() {
        let store = Store::open_in_memory().unwrap();
        let id = perm(&store, "bye", 10);
        assert_eq!(store.tombstone_clip(id, 20).unwrap().rows, 1);
        assert!(store.list_clips(true, None, None, 0).unwrap().is_empty());
        assert!(store.list_clips(true, Some("bye"), None, 0).unwrap().is_empty());

        // 再次复制同样内容：按新的临时条目复活，文字恢复
        store.upsert_clip(&text_clip("bye"), 30, TTL).unwrap();
        let items = store.list_clips(false, Some("bye"), None, 0).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].preview, "bye");
    }

    #[test]
    fn remote_delete_keeps_local_temp() {
        let store = Store::open_in_memory().unwrap();
        perm(&store, "p", 10);
        assert_eq!(store.apply_remote_delete("hash-p").unwrap().rows, 1);
        store.upsert_clip(&text_clip("t"), 10, TTL).unwrap();
        assert_eq!(store.apply_remote_delete("hash-t").unwrap().rows, 0);
    }

    #[test]
    fn purge_old_tombstones() {
        let store = Store::open_in_memory().unwrap();
        let a = perm(&store, "a", 10);
        let b = perm(&store, "b", 10);
        store.tombstone_clip(a, 100).unwrap();
        store.tombstone_clip(b, 300).unwrap();
        assert_eq!(store.purge_tombstones(200).unwrap(), 1);
        assert_eq!(store.sync_entries().unwrap().len(), 1);
    }
}
