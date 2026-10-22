use std::collections::HashSet;

use rusqlite::{params, Params, Statement};

use super::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Rich,
    Image,
    Files,
}

impl ClipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Rich => "rich",
            Self::Image => "image",
            Self::Files => "files",
        }
    }
}

/// 一次采集得到、待入库的条目。
#[derive(Debug, Clone)]
pub struct NewClip {
    pub kind: ClipKind,
    pub content_hash: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub image_file: Option<String>,
    pub thumb: Option<Vec<u8>>,
    pub size_bytes: i64,
    pub source_app: Option<String>,
}

/// 批量删除的结果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Deleted {
    pub rows: usize,
    /// 被删条目引用的图片文件名，需要调用方一并删除文件。
    pub image_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upserted {
    pub id: i64,
    /// false 表示内容已存在，只刷新了时间并置顶。
    pub inserted: bool,
}

impl Store {
    /// 新内容插入为临时条目；已存在的内容刷新复制时间与过期时间，并以本次复制的格式为准。
    /// 永久条目保持永久；已删除（墓碑）的条目按新的临时条目复活。
    pub fn upsert_clip(&self, clip: &NewClip, now: i64, ttl_ms: i64) -> rusqlite::Result<Upserted> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let (id, returned_uuid): (i64, String) = self.conn().query_row(
            "INSERT INTO clips (uuid, kind, content_hash, text, html, rtf, image_file, thumb,
                                size_bytes, source_app, is_permanent, created_at, last_copied_at,
                                expires_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?11, ?12, ?11)
             ON CONFLICT (content_hash) DO UPDATE SET
               kind = excluded.kind,
               html = excluded.html,
               rtf = excluded.rtf,
               image_file = excluded.image_file,
               thumb = excluded.thumb,
               size_bytes = excluded.size_bytes,
               source_app = excluded.source_app,
               is_permanent = CASE WHEN clips.deleted_at IS NULL THEN clips.is_permanent ELSE 0 END,
               expires_at = CASE WHEN clips.deleted_at IS NULL AND clips.is_permanent = 1
                                 THEN NULL ELSE excluded.expires_at END,
               last_copied_at = excluded.last_copied_at,
               updated_at = excluded.updated_at,
               deleted_at = NULL
             RETURNING id, uuid",
            params![
                uuid,
                clip.kind.as_str(),
                clip.content_hash,
                clip.text,
                clip.html,
                clip.rtf,
                clip.image_file,
                clip.thumb,
                clip.size_bytes,
                clip.source_app,
                now,
                now.saturating_add(ttl_ms),
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(Upserted {
            id,
            inserted: returned_uuid == uuid,
        })
    }

    /// 删除已过期的临时条目。
    pub fn purge_expired(&self, now: i64) -> rusqlite::Result<Deleted> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "DELETE FROM clips WHERE is_permanent = 0 AND expires_at <= ?1 RETURNING image_file",
        )?;
        deleted(&mut stmt, params![now])
    }

    /// 只保留最近复制的 `keep` 条临时条目。
    pub fn trim_temp(&self, keep: usize) -> rusqlite::Result<Deleted> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "DELETE FROM clips WHERE id IN (
               SELECT id FROM clips WHERE is_permanent = 0
               ORDER BY last_copied_at DESC, id DESC LIMIT -1 OFFSET ?1
             ) RETURNING image_file",
        )?;
        deleted(&mut stmt, params![keep as i64])
    }

    /// 所有仍被引用的图片文件名，用于清理孤立文件。
    pub fn image_files(&self) -> rusqlite::Result<HashSet<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT image_file FROM clips WHERE image_file IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }
}

/// DELETE ... RETURNING image_file 每删除一行返回一行，据此同时得到行数和图片文件。
fn deleted(stmt: &mut Statement<'_>, params: impl Params) -> rusqlite::Result<Deleted> {
    let rows = stmt.query_map(params, |r| r.get::<_, Option<String>>(0))?;
    let mut out = Deleted::default();
    for row in rows {
        out.rows += 1;
        out.image_files.extend(row?);
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const TTL: i64 = 1000;

    pub fn text_clip(text: &str) -> NewClip {
        NewClip {
            kind: ClipKind::Text,
            content_hash: format!("hash-{text}"),
            text: Some(text.into()),
            html: None,
            rtf: None,
            image_file: None,
            thumb: None,
            size_bytes: text.len() as i64,
            source_app: Some("notepad.exe".into()),
        }
    }

    fn row(store: &Store, id: i64) -> (String, i64, i64, Option<i64>, Option<String>) {
        store
            .conn()
            .query_row(
                "SELECT kind, is_permanent, last_copied_at, expires_at, html FROM clips WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap()
    }

    fn count(store: &Store) -> i64 {
        store.conn().query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn insert_then_duplicate_bumps_and_refreshes() {
        let store = Store::open_in_memory().unwrap();
        let first = store.upsert_clip(&text_clip("a"), 100, TTL).unwrap();
        assert!(first.inserted);
        assert_eq!(row(&store, first.id), ("text".into(), 0, 100, Some(1100), None));

        let mut rich = text_clip("a");
        rich.kind = ClipKind::Rich;
        rich.html = Some("<b>a</b>".into());
        let again = store.upsert_clip(&rich, 500, TTL).unwrap();
        assert_eq!(again, Upserted { id: first.id, inserted: false });
        assert_eq!(
            row(&store, first.id),
            ("rich".into(), 0, 500, Some(1500), Some("<b>a</b>".into()))
        );
        assert_eq!(count(&store), 1);
    }

    #[test]
    fn duplicate_of_permanent_stays_permanent() {
        let store = Store::open_in_memory().unwrap();
        let id = store.upsert_clip(&text_clip("a"), 100, TTL).unwrap().id;
        store
            .conn()
            .execute("UPDATE clips SET is_permanent = 1, expires_at = NULL WHERE id = ?1", [id])
            .unwrap();
        store.upsert_clip(&text_clip("a"), 500, TTL).unwrap();
        let (_, perm, copied, expires, _) = row(&store, id);
        assert_eq!((perm, copied, expires), (1, 500, None));
    }

    #[test]
    fn tombstoned_clip_revives_as_temp() {
        let store = Store::open_in_memory().unwrap();
        let id = store.upsert_clip(&text_clip("a"), 100, TTL).unwrap().id;
        store
            .conn()
            .execute(
                "UPDATE clips SET is_permanent = 1, expires_at = NULL, deleted_at = 200 WHERE id = ?1",
                [id],
            )
            .unwrap();
        store.upsert_clip(&text_clip("a"), 500, TTL).unwrap();
        let (_, perm, _, expires, _) = row(&store, id);
        assert_eq!((perm, expires), (0, Some(1500)));
        let deleted: Option<i64> = store
            .conn()
            .query_row("SELECT deleted_at FROM clips WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(deleted, None);
    }

    #[test]
    fn purge_removes_only_expired_temp() {
        let store = Store::open_in_memory().unwrap();
        let mut img = text_clip("old");
        img.image_file = Some("old.png".into());
        store.upsert_clip(&img, 0, TTL).unwrap();
        store.upsert_clip(&text_clip("fresh"), 900, TTL).unwrap();
        let perm = store.upsert_clip(&text_clip("perm"), 0, TTL).unwrap().id;
        store
            .conn()
            .execute("UPDATE clips SET is_permanent = 1, expires_at = NULL WHERE id = ?1", [perm])
            .unwrap();

        assert_eq!(
            store.purge_expired(1000).unwrap(),
            Deleted { rows: 1, image_files: vec!["old.png".into()] }
        );
        assert_eq!(count(&store), 2);
        assert_eq!(store.purge_expired(1000).unwrap(), Deleted::default());
    }

    #[test]
    fn trim_keeps_most_recent_temp() {
        let store = Store::open_in_memory().unwrap();
        for (i, t) in ["a", "b", "c", "d"].iter().enumerate() {
            let mut c = text_clip(t);
            c.image_file = Some(format!("{t}.png"));
            store.upsert_clip(&c, i as i64, TTL).unwrap();
        }
        // 重新复制 a，使其变为最新
        store.upsert_clip(&text_clip("a"), 10, TTL).unwrap();

        let mut removed = store.trim_temp(2).unwrap();
        removed.image_files.sort();
        assert_eq!(removed, Deleted { rows: 2, image_files: vec!["b.png".into(), "c.png".into()] });
        assert_eq!(count(&store), 2);
        assert_eq!(store.trim_temp(2).unwrap(), Deleted::default());
    }

    #[test]
    fn image_files_lists_references() {
        let store = Store::open_in_memory().unwrap();
        let mut c = text_clip("img");
        c.image_file = Some("x.png".into());
        store.upsert_clip(&c, 0, TTL).unwrap();
        store.upsert_clip(&text_clip("plain"), 0, TTL).unwrap();
        assert_eq!(store.image_files().unwrap(), HashSet::from(["x.png".to_string()]));
    }
}
