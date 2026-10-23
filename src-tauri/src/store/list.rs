use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params_from_iter, types::Value};
use serde::{Deserialize, Serialize, Serializer};

use super::Store;

/// 每页条数（不含置顶条目）。列表滚动到底部时按游标加载下一页。
pub const PAGE_SIZE: usize = 100;
/// 置顶条目随第一页一并返回的上限。
const MAX_PINNED: usize = 100;
/// 列表只传预览文字，避免大段文本拖慢 IPC 和渲染。
const PREVIEW_CHARS: i64 = 400;
/// trigram 分词需要至少 3 个字符，更短的查询退化为 LIKE。
const FTS_MIN_CHARS: usize = 3;

/// 分页游标：上一页最后一条（非置顶）的复制时间和 ID。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub last_copied_at: i64,
    pub id: i64,
}

/// 弹出列表中的一行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    /// 带 HTML/RTF，可选择以纯文本粘贴。
    pub rich: bool,
    #[serde(serialize_with = "thumb_data_url")]
    pub thumb: Option<Vec<u8>>,
    pub size_bytes: i64,
    pub source_app: Option<String>,
    pub permanent: bool,
    pub pinned: bool,
    pub last_copied_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub temp: usize,
    pub perm: usize,
}

fn thumb_data_url<S: Serializer>(thumb: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    match thumb {
        Some(bytes) => s.serialize_some(&format!("data:image/png;base64,{}", STANDARD.encode(bytes))),
        None => s.serialize_none(),
    }
}

/// 可见条目条件：未删除，且临时条目尚未过期（清理线程按分钟运行，这里不依赖它）。
/// `now` 为当前时间参数的占位符，如 `?2`。
fn visible(now: &str) -> String {
    format!("deleted_at IS NULL AND (is_permanent = 1 OR expires_at > {now})")
}

impl Store {
    /// 列出一页条目；`search` 为空时不过滤。第一页（`before` 为空）先返回全部置顶条目
    /// （按置顶时间倒序），再接按最近复制时间倒序的普通条目；后续页只含普通条目。
    pub fn list_clips(
        &self,
        permanent: bool,
        search: Option<&str>,
        before: Option<Cursor>,
        now: i64,
    ) -> rusqlite::Result<Vec<ClipItem>> {
        let mut items = if before.is_none() {
            self.query_items(permanent, search, None, now, true)?
        } else {
            Vec::new()
        };
        items.extend(self.query_items(permanent, search, before, now, false)?);
        Ok(items)
    }

    fn query_items(
        &self,
        permanent: bool,
        search: Option<&str>,
        before: Option<Cursor>,
        now: i64,
        pinned: bool,
    ) -> rusqlite::Result<Vec<ClipItem>> {
        let mut sql = format!(
            "SELECT id, kind, substr(text, 1, {PREVIEW_CHARS}), html IS NOT NULL OR rtf IS NOT NULL,
                    thumb, size_bytes, source_app, is_permanent, last_copied_at, expires_at,
                    pinned_at IS NOT NULL
             FROM clips
             WHERE is_permanent = ?1 AND {} AND pinned_at IS {}",
            visible("?2"),
            if pinned { "NOT NULL" } else { "NULL" }
        );
        let mut args: Vec<Value> = vec![(permanent as i64).into(), now.into()];

        if let Some(c) = before {
            sql.push_str(" AND (last_copied_at < ?3 OR (last_copied_at = ?3 AND id < ?4))");
            args.push(c.last_copied_at.into());
            args.push(c.id.into());
        }

        let search = search.map(str::trim).filter(|s| !s.is_empty());
        if let Some(q) = search {
            let n = args.len() + 1;
            if q.chars().count() >= FTS_MIN_CHARS {
                sql.push_str(&format!(" AND id IN (SELECT rowid FROM clips_fts WHERE clips_fts MATCH ?{n})"));
                // 整体作为短语匹配，避免用户输入被解析成 FTS 语法
                args.push(format!("\"{}\"", q.replace('"', "\"\"")).into());
            } else {
                sql.push_str(&format!(" AND text LIKE ?{n} ESCAPE '\\'"));
                args.push(format!("%{}%", escape_like(q)).into());
            }
        }

        sql.push_str(&if pinned {
            format!(" ORDER BY pinned_at DESC, id DESC LIMIT {MAX_PINNED}")
        } else {
            format!(" ORDER BY last_copied_at DESC, id DESC LIMIT {PAGE_SIZE}")
        });

        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| {
            Ok(ClipItem {
                id: r.get(0)?,
                kind: r.get(1)?,
                preview: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                rich: r.get(3)?,
                thumb: r.get(4)?,
                size_bytes: r.get(5)?,
                source_app: r.get(6)?,
                permanent: r.get(7)?,
                last_copied_at: r.get(8)?,
                expires_at: r.get(9)?,
                pinned: r.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn counts(&self, now: i64) -> rusqlite::Result<Counts> {
        let sql = format!(
            "SELECT COALESCE(SUM(is_permanent = 0), 0), COALESCE(SUM(is_permanent = 1), 0)
             FROM clips WHERE {}",
            visible("?1")
        );
        self.conn().query_row(&sql, [now], |r| {
            Ok(Counts {
                temp: r.get::<_, i64>(0)? as usize,
                perm: r.get::<_, i64>(1)? as usize,
            })
        })
    }
}

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::clips_tests::text_clip;

    const TTL: i64 = 1000;

    fn ids(items: &[ClipItem]) -> Vec<i64> {
        items.iter().map(|i| i.id).collect()
    }

    fn seed(store: &Store, texts: &[&str]) -> Vec<i64> {
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| store.upsert_clip(&text_clip(t), i as i64 * 10, TTL).unwrap().id)
            .collect()
    }

    #[test]
    fn lists_newest_first_and_hides_expired() {
        let store = Store::open_in_memory().unwrap();
        let id = seed(&store, &["a", "b", "c"]);
        // a 在 1000 过期，b 在 1010，c 在 1020
        let items = store.list_clips(false, None, None, 1005).unwrap();
        assert_eq!(ids(&items), vec![id[2], id[1]]);
        assert_eq!(store.counts(1005).unwrap(), Counts { temp: 2, perm: 0 });
    }

    #[test]
    fn separates_tabs() {
        let store = Store::open_in_memory().unwrap();
        let id = seed(&store, &["temp", "perm"]);
        store.set_permanent(id[1], true, 50, TTL).unwrap();
        assert_eq!(ids(&store.list_clips(false, None, None, 60).unwrap()), vec![id[0]]);
        let perm = store.list_clips(true, None, None, 60).unwrap();
        assert_eq!(ids(&perm), vec![id[1]]);
        assert!(perm[0].permanent && perm[0].expires_at.is_none());
        // 永久条目不会过期
        assert_eq!(store.counts(1_000_000).unwrap(), Counts { temp: 0, perm: 1 });
    }

    #[test]
    fn paginates_with_cursor() {
        let store = Store::open_in_memory().unwrap();
        let texts: Vec<String> = (0..PAGE_SIZE + 5).map(|i| format!("item {i}")).collect();
        for (i, t) in texts.iter().enumerate() {
            store.upsert_clip(&text_clip(t), i as i64, 1_000_000).unwrap();
        }
        let first = store.list_clips(false, None, None, 0).unwrap();
        assert_eq!(first.len(), PAGE_SIZE);
        let last = first.last().unwrap();
        let cursor = Cursor { last_copied_at: last.last_copied_at, id: last.id };
        let second = store.list_clips(false, None, Some(cursor), 0).unwrap();
        assert_eq!(second.len(), 5);
        assert!(second.iter().all(|i| i.last_copied_at < last.last_copied_at));
    }

    #[test]
    fn searches_with_fts_and_short_like() {
        let store = Store::open_in_memory().unwrap();
        let id = seed(&store, &["会议改到周四下午", "订单号 100%", "Hello World"]);
        let find = |q: &str| ids(&store.list_clips(false, Some(q), None, 0).unwrap());

        assert_eq!(find("周四下"), vec![id[0]]);
        assert_eq!(find("hello world"), vec![id[2]]);
        assert_eq!(find("会议"), vec![id[0]]);
        assert_eq!(find("0%"), vec![id[1]]);
        assert_eq!(find("%"), vec![id[1]]);
        assert_eq!(find("\"quoted"), Vec::<i64>::new());
        assert_eq!(find("   ").len(), 3);
    }

    #[test]
    fn pinned_items_come_first_on_first_page_only() {
        let store = Store::open_in_memory().unwrap();
        let texts: Vec<String> = (0..PAGE_SIZE + 3).map(|i| format!("item {i}")).collect();
        let ids: Vec<i64> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| store.upsert_clip(&text_clip(t), i as i64, 1_000_000).unwrap().id)
            .collect();
        // 置顶两条较早的条目，后置顶的排在前面
        store.set_pinned(ids[0], true, 10).unwrap();
        store.set_pinned(ids[1], true, 20).unwrap();

        let first = store.list_clips(false, None, None, 0).unwrap();
        assert_eq!(first[..2].iter().map(|i| i.id).collect::<Vec<_>>(), vec![ids[1], ids[0]]);
        assert!(first[0].pinned && !first[2].pinned);
        assert_eq!(first.len(), 2 + PAGE_SIZE);

        let last = first.last().unwrap();
        let second = store
            .list_clips(false, None, Some(Cursor { last_copied_at: last.last_copied_at, id: last.id }), 0)
            .unwrap();
        // 剩余 1 条普通条目，置顶条目不会重复出现
        assert_eq!(second.len(), 1);
        assert!(second.iter().all(|i| !i.pinned));

        // 搜索同样作用于置顶条目；取消置顶后回到时间顺序
        assert_eq!(ids_of(&store.list_clips(false, Some("item 1"), None, 0).unwrap())[0], ids[1]);
        store.set_pinned(ids[1], false, 30).unwrap();
        let after = store.list_clips(false, None, None, 0).unwrap();
        assert_eq!(after[0].id, ids[0]);
        assert_eq!(after.iter().filter(|i| i.pinned).count(), 1);
    }

    fn ids_of(items: &[ClipItem]) -> Vec<i64> {
        items.iter().map(|i| i.id).collect()
    }

    #[test]
    fn search_index_follows_deletes() {
        let store = Store::open_in_memory().unwrap();
        let id = seed(&store, &["searchable text"]);
        store.delete_clip(id[0]).unwrap();
        assert!(store.list_clips(false, Some("searchable"), None, 0).unwrap().is_empty());
    }

    #[test]
    fn thumb_serializes_as_data_url() {
        let item = ClipItem {
            id: 1,
            kind: "image".into(),
            preview: String::new(),
            rich: false,
            thumb: Some(vec![1, 2, 3]),
            size_bytes: 3,
            source_app: None,
            permanent: false,
            pinned: false,
            last_copied_at: 0,
            expires_at: None,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["thumb"], "data:image/png;base64,AQID");
        assert_eq!(json["lastCopiedAt"], 0);
    }
}
