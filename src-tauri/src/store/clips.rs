use std::collections::HashSet;

use rusqlite::{params, Connection, Params};

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

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "text" => Self::Text,
            "rich" => Self::Rich,
            "image" => Self::Image,
            "files" => Self::Files,
            _ => return None,
        })
    }
}

/// 写回剪贴板所需的完整内容。
#[derive(Debug, Clone)]
pub struct ClipContent {
    pub kind: ClipKind,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub image_file: Option<String>,
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
    /// 通过永久复制热键采集时为 true。
    pub permanent: bool,
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
    /// 新内容插入为临时条目（`clip.permanent` 时为永久）；已存在的内容刷新复制时间与过期时间，
    /// 并以本次复制的格式为准。永久条目保持永久；已删除（墓碑）的条目按新条目复活。
    pub fn upsert_clip(&self, clip: &NewClip, now: i64, ttl_ms: i64) -> rusqlite::Result<Upserted> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let expires = (!clip.permanent).then(|| now.saturating_add(ttl_ms));
        let (id, returned_uuid): (i64, String) = self.conn().query_row(
            "INSERT INTO clips (uuid, kind, content_hash, text, html, rtf, image_file, thumb,
                                size_bytes, source_app, is_permanent, created_at, last_copied_at,
                                expires_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?13, ?11, ?11, ?12, ?11)
             ON CONFLICT (content_hash) DO UPDATE SET
               kind = excluded.kind,
               text = excluded.text,
               html = excluded.html,
               rtf = excluded.rtf,
               image_file = excluded.image_file,
               thumb = excluded.thumb,
               size_bytes = excluded.size_bytes,
               source_app = excluded.source_app,
               is_permanent = CASE WHEN excluded.is_permanent = 1 THEN 1
                                   WHEN clips.deleted_at IS NULL THEN clips.is_permanent ELSE 0 END,
               expires_at = CASE WHEN excluded.is_permanent = 1
                                   OR (clips.deleted_at IS NULL AND clips.is_permanent = 1)
                                 THEN NULL ELSE excluded.expires_at END,
               perm_removed_at = CASE WHEN excluded.is_permanent = 1
                                        OR (clips.deleted_at IS NULL AND clips.is_permanent = 1)
                                      THEN NULL
                                      WHEN clips.deleted_at IS NOT NULL THEN clips.deleted_at
                                      ELSE clips.perm_removed_at END,
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
                expires,
                clip.permanent,
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
        let mut conn = self.conn();
        remove_clips(
            &mut conn,
            "SELECT id, image_file, perm_removed_at FROM clips
             WHERE is_permanent = 0 AND deleted_at IS NULL AND expires_at <= ?1",
            params![now],
        )
    }

    /// 只保留最近复制的 `keep` 条临时条目；已置顶的临时条目优先保留。
    pub fn trim_temp(&self, keep: usize) -> rusqlite::Result<Deleted> {
        let mut conn = self.conn();
        remove_clips(
            &mut conn,
            "SELECT id, image_file, perm_removed_at FROM clips WHERE id IN (
               SELECT id FROM clips WHERE is_permanent = 0 AND deleted_at IS NULL
               ORDER BY pinned_at IS NOT NULL DESC, last_copied_at DESC, id DESC LIMIT -1 OFFSET ?1
             )",
            params![keep as i64],
        )
    }

    pub fn clip_content(&self, id: i64) -> rusqlite::Result<Option<ClipContent>> {
        let row = self.conn().query_row(
            "SELECT kind, text, html, rtf, image_file FROM clips WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    ClipContent {
                        kind: ClipKind::Text,
                        text: r.get(1)?,
                        html: r.get(2)?,
                        rtf: r.get(3)?,
                        image_file: r.get(4)?,
                    },
                ))
            },
        );
        match row {
            Ok((kind, content)) => Ok(ClipKind::parse(&kind).map(|kind| ClipContent { kind, ..content })),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 设为永久（不再过期），或转为临时（从现在起重新计时，并记录脱离永久集合的时间供云同步传播删除）。
    pub fn set_permanent(&self, id: i64, permanent: bool, now: i64, ttl_ms: i64) -> rusqlite::Result<bool> {
        let expires = (!permanent).then(|| now.saturating_add(ttl_ms));
        let n = self.conn().execute(
            "UPDATE clips SET
               is_permanent = ?2,
               expires_at = ?3,
               updated_at = ?4,
               perm_removed_at = CASE WHEN ?2 = 1 THEN NULL
                                      WHEN is_permanent = 1 THEN ?4
                                      ELSE perm_removed_at END
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id, permanent, expires, now],
        )?;
        Ok(n > 0)
    }

    /// 置顶或取消置顶（只影响排序，不改变过期时间）。返回条目是否存在。
    pub fn set_pinned(&self, id: i64, pinned: bool, now: i64) -> rusqlite::Result<bool> {
        let n = self.conn().execute(
            "UPDATE clips SET pinned_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![id, pinned.then_some(now)],
        )?;
        Ok(n > 0)
    }

    /// 记录一次实际粘贴，用于按使用频率排序。本机统计，不参与同步，也不改变复制时间。
    pub fn record_paste(&self, id: i64) -> rusqlite::Result<()> {
        self.conn().execute(
            "UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        Ok(())
    }

    /// 最近复制且未过期的临时条目。
    pub fn latest_temp(&self, now: i64) -> rusqlite::Result<Option<i64>> {
        let row = self.conn().query_row(
            "SELECT id FROM clips WHERE is_permanent = 0 AND deleted_at IS NULL AND expires_at > ?1
             ORDER BY last_copied_at DESC, id DESC LIMIT 1",
            [now],
            |r| r.get(0),
        );
        match row {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete_clip(&self, id: i64) -> rusqlite::Result<Deleted> {
        let mut conn = self.conn();
        remove_clips(
            &mut conn,
            "SELECT id, image_file, perm_removed_at FROM clips WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
        )
    }

    pub fn clear_temp(&self) -> rusqlite::Result<Deleted> {
        let mut conn = self.conn();
        remove_clips(
            &mut conn,
            "SELECT id, image_file, perm_removed_at FROM clips WHERE is_permanent = 0 AND deleted_at IS NULL",
            [],
        )
    }

    /// 所有仍被引用的图片文件名，用于清理孤立文件。
    pub fn image_files(&self) -> rusqlite::Result<HashSet<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT image_file FROM clips WHERE image_file IS NOT NULL AND deleted_at IS NULL")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }
}

/// 删除匹配的条目：曾是永久条目的临时行（`perm_removed_at` 非空）转为墓碑以防止云端旧版本回灌，
/// 其余直接删除；最后过滤掉仍被其他存活条目共享引用的图片文件。
fn remove_clips(conn: &mut Connection, select_sql: &str, params: impl Params) -> rusqlite::Result<Deleted> {
    let tx = conn.transaction()?;
    let targets: Vec<(i64, Option<String>, Option<i64>)> = {
        let mut stmt = tx.prepare(select_sql)?;
        let rows = stmt.query_map(params, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut candidates = Vec::new();
    for (id, image, perm_removed_at) in &targets {
        if let Some(at) = perm_removed_at {
            tx.execute(
                "UPDATE clips SET is_permanent = 0, deleted_at = ?2, updated_at = max(updated_at, ?2),
                   perm_removed_at = NULL, text = NULL, html = NULL, rtf = NULL,
                   image_file = NULL, thumb = NULL, pinned_at = NULL
                 WHERE id = ?1",
                params![id, at],
            )?;
        } else {
            tx.execute("DELETE FROM clips WHERE id = ?1", [id])?;
        }
        if let Some(f) = image {
            candidates.push(f.clone());
        }
    }
    let image_files = unreferenced_images(&tx, candidates)?;
    tx.commit()?;
    Ok(Deleted { rows: targets.len(), image_files })
}

/// 只保留不再被任何未删除条目引用的图片文件名（图片条目与图片文件预览可共享同一像素哈希文件）。
pub(super) fn unreferenced_images(
    conn: &Connection,
    candidates: impl IntoIterator<Item = String>,
) -> rusqlite::Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM clips WHERE image_file = ?1 AND deleted_at IS NULL)")?;
    for file in candidates {
        if seen.insert(file.clone()) {
            let still_used: bool = stmt.query_row([&file], |r| r.get(0))?;
            if !still_used {
                out.push(file);
            }
        }
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
            permanent: false,
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
    fn permanent_capture_inserts_or_promotes() {
        let store = Store::open_in_memory().unwrap();
        let mut c = text_clip("new");
        c.permanent = true;
        let id = store.upsert_clip(&c, 100, TTL).unwrap().id;
        assert_eq!(row(&store, id), ("text".into(), 1, 100, None, None));

        // 已有的临时条目被永久复制后转为永久
        let temp = store.upsert_clip(&text_clip("old"), 0, TTL).unwrap().id;
        let mut again = text_clip("old");
        again.permanent = true;
        store.upsert_clip(&again, 200, TTL).unwrap();
        assert_eq!(row(&store, temp), ("text".into(), 1, 200, None, None));
    }

    #[test]
    fn latest_temp_skips_permanent_and_expired() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.latest_temp(0).unwrap(), None);
        let old = store.upsert_clip(&text_clip("old"), 0, TTL).unwrap().id;
        let perm = store.upsert_clip(&text_clip("perm"), 50, TTL).unwrap().id;
        store.set_permanent(perm, true, 50, TTL).unwrap();
        assert_eq!(store.latest_temp(500).unwrap(), Some(old));
        assert_eq!(store.latest_temp(1000).unwrap(), None);
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
    fn content_round_trips() {
        let store = Store::open_in_memory().unwrap();
        let mut c = text_clip("hi");
        c.kind = ClipKind::Rich;
        c.html = Some("<b>hi</b>".into());
        let id = store.upsert_clip(&c, 0, TTL).unwrap().id;
        let got = store.clip_content(id).unwrap().unwrap();
        assert_eq!(got.kind, ClipKind::Rich);
        assert_eq!(got.text.as_deref(), Some("hi"));
        assert_eq!(got.html.as_deref(), Some("<b>hi</b>"));
        assert!(store.clip_content(id + 100).unwrap().is_none());
    }

    #[test]
    fn toggle_permanent() {
        let store = Store::open_in_memory().unwrap();
        let id = store.upsert_clip(&text_clip("a"), 0, TTL).unwrap().id;
        store.set_permanent(id, true, 100, TTL).unwrap();
        assert_eq!(row(&store, id).1, 1);
        assert_eq!(row(&store, id).3, None);
        // 转回临时：从此刻起重新计时，复制时间不变
        store.set_permanent(id, false, 5000, TTL).unwrap();
        assert_eq!(row(&store, id), ("text".into(), 0, 0, Some(6000), None));
    }

    #[test]
    fn delete_and_clear_temp() {
        let store = Store::open_in_memory().unwrap();
        let mut img = text_clip("img");
        img.image_file = Some("img.png".into());
        let a = store.upsert_clip(&img, 0, TTL).unwrap().id;
        store.upsert_clip(&text_clip("b"), 0, TTL).unwrap();
        let perm = store.upsert_clip(&text_clip("p"), 0, TTL).unwrap().id;
        store.set_permanent(perm, true, 0, TTL).unwrap();

        assert_eq!(store.delete_clip(a).unwrap(), Deleted { rows: 1, image_files: vec!["img.png".into()] });
        assert_eq!(store.clear_temp().unwrap().rows, 1);
        assert_eq!(count(&store), 1);
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

    #[test]
    fn shared_image_file_stays_until_last_reference_removed() {
        let store = Store::open_in_memory().unwrap();
        let mut perm_img = text_clip("image-item");
        perm_img.kind = ClipKind::Image;
        perm_img.image_file = Some("shared.png".into());
        perm_img.permanent = true;
        let perm_id = store.upsert_clip(&perm_img, 10, TTL).unwrap().id;

        let mut file_img = text_clip("C:\\temp\\wechat.jpg");
        file_img.kind = ClipKind::Files;
        file_img.image_file = Some("shared.png".into());
        store.upsert_clip(&file_img, 20, TTL).unwrap();

        // 临时图片文件被清空时，永久图片条目仍在引用 shared.png，不能删除文件
        assert_eq!(store.clear_temp().unwrap(), Deleted { rows: 1, image_files: vec![] });
        assert!(store.image_files().unwrap().contains("shared.png"));

        // 最后一个引用被删除后才返回文件名以清理磁盘文件
        assert_eq!(
            store.delete_clip(perm_id).unwrap(),
            Deleted { rows: 1, image_files: vec!["shared.png".into()] }
        );
    }

    #[test]
    fn trim_preserves_pinned_temp_items() {
        let store = Store::open_in_memory().unwrap();
        let pinned_old = store.upsert_clip(&text_clip("pinned-old"), 1, TTL).unwrap().id;
        store.set_pinned(pinned_old, true, 2).unwrap();
        let unpinned_mid = store.upsert_clip(&text_clip("unpinned-mid"), 10, TTL).unwrap().id;
        let unpinned_new = store.upsert_clip(&text_clip("unpinned-new"), 20, TTL).unwrap().id;

        assert_eq!(store.trim_temp(2).unwrap().rows, 1);
        assert!(store.clip_content(pinned_old).unwrap().is_some());
        assert!(store.clip_content(unpinned_new).unwrap().is_some());
        assert!(store.clip_content(unpinned_mid).unwrap().is_none());
    }
}
