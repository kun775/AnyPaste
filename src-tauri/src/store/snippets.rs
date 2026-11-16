use rusqlite::{params, Row};
use serde::Serialize;

use super::{list::escape_like, Store};

/// 列表只返回预览，避免大段内容占用 IPC。
const PREVIEW_CHARS: i64 = 400;
/// 搜索命中靠后时，预览从命中位置之前多少个字符开始截取。
const EXCERPT_LEAD: i64 = 24;
/// 命中位置超过该字符数才截取，避免开头命中时预览跳动。
const EXCERPT_THRESHOLD: i64 = 48;
/// 片段数量按日常使用规模设计，不额外分页；创建与列表共用同一上限。
pub const MAX_SNIPPETS: usize = 200;

/// 新建片段的结果：达到数量上限时单独提示，其余保留数据库错误。
#[derive(Debug)]
pub enum CreateSnippetError {
    LimitReached,
    Db(rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetItem {
    pub id: i64,
    pub title: String,
    pub preview: String,
    /// 内容的字符数。
    pub chars: usize,
    pub updated_at: i64,
}

impl Store {
    pub fn list_snippets(&self, search: Option<&str>) -> rusqlite::Result<Vec<SnippetItem>> {
        let needle = search.map(str::trim).filter(|s| !s.is_empty()).unwrap_or("");
        let pattern = if needle.is_empty() {
            "%".into()
        } else {
            format!("%{}%", escape_like(needle))
        };
        // 命中在正文靠后位置时从命中处附近截取预览，否则列表行里看不到搜索词。
        // 空查询时 instr 返回 1，走“开头预览”分支。
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, title,
                    CASE WHEN pos > {EXCERPT_THRESHOLD}
                         THEN '…' || substr(content, max(1, pos - {EXCERPT_LEAD}), {PREVIEW_CHARS})
                         ELSE substr(content, 1, {PREVIEW_CHARS})
                    END,
                    length(content), updated_at
             FROM (
               SELECT id, title, content, updated_at,
                      instr(lower(content), lower(?1)) AS pos
               FROM snippets
               WHERE title LIKE ?2 ESCAPE '\\' OR content LIKE ?2 ESCAPE '\\'
             )
             ORDER BY updated_at DESC, id DESC
             LIMIT {MAX_SNIPPETS}"
        ))?;
        let rows = stmt.query_map(params![needle, pattern], row_to_item)?;
        rows.collect()
    }

    pub fn snippet_content(&self, id: i64) -> rusqlite::Result<Option<String>> {
        let row = self.conn().query_row("SELECT content FROM snippets WHERE id = ?1", [id], |r| r.get(0));
        match row {
            Ok(text) => Ok(Some(text)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 编辑片段时读取完整标题与正文，不做预览截断。
    pub fn snippet_draft(&self, id: i64) -> rusqlite::Result<Option<(String, String)>> {
        let row = self.conn().query_row("SELECT title, content FROM snippets WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        });
        match row {
            Ok(draft) => Ok(Some(draft)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 新建片段；数量检查与插入在同一把连接锁内完成，避免并发创建越过上限。
    pub fn create_snippet(&self, title: &str, content: &str, now: i64) -> Result<SnippetItem, CreateSnippetError> {
        let conn = self.conn();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM snippets", [], |r| r.get(0))?;
        if count >= MAX_SNIPPETS as i64 {
            return Err(CreateSnippetError::LimitReached);
        }
        conn.execute(
            "INSERT INTO snippets (title, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![title, content, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(item_by_id(&conn, id)?)
    }

    /// 更新已有片段；返回 `None` 表示片段已被删除。
    pub fn update_snippet(
        &self,
        id: i64,
        title: &str,
        content: &str,
        now: i64,
    ) -> rusqlite::Result<Option<SnippetItem>> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE snippets SET title = ?2, content = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, title, content, now],
        )?;
        if n == 0 {
            return Ok(None);
        }
        item_by_id(&conn, id).map(Some)
    }

    pub fn delete_snippet(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self.conn().execute("DELETE FROM snippets WHERE id = ?1", [id])? > 0)
    }
}

impl From<rusqlite::Error> for CreateSnippetError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}

fn item_by_id(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<SnippetItem> {
    conn.query_row(
        "SELECT id, title, substr(content, 1, ?2), length(content), updated_at FROM snippets WHERE id = ?1",
        params![id, PREVIEW_CHARS],
        row_to_item,
    )
}

fn row_to_item(r: &Row<'_>) -> rusqlite::Result<SnippetItem> {
    Ok(SnippetItem {
        id: r.get(0)?,
        title: r.get(1)?,
        preview: r.get(2)?,
        chars: r.get::<_, i64>(3)? as usize,
        updated_at: r.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(store: &Store, title: &str, content: &str, now: i64) -> SnippetItem {
        store.create_snippet(title, content, now).unwrap()
    }

    #[test]
    fn creates_searches_updates_and_deletes() {
        let store = Store::open_in_memory().unwrap();
        let mail = seed(&store, "工作邮件", "你好，附件是本周的进度报告。", 10);
        let addr = seed(&store, "公司地址", "上海市浦东新区示例路 88 号", 20);
        assert_eq!(store.counts(0).unwrap().snippets, 2);
        assert_eq!(store.list_snippets(None).unwrap()[0].id, addr.id);
        assert_eq!(store.list_snippets(Some("进度")).unwrap()[0].id, mail.id);
        assert_eq!(store.list_snippets(Some("公司地址")).unwrap()[0].id, addr.id);
        assert!(store.list_snippets(Some("不存在")).unwrap().is_empty());

        let updated = store.update_snippet(mail.id, "周报邮件", "新的正文", 30).unwrap().unwrap();
        assert_eq!((updated.title.as_str(), updated.preview.as_str()), ("周报邮件", "新的正文"));
        assert_eq!(store.snippet_content(mail.id).unwrap().as_deref(), Some("新的正文"));

        assert!(store.delete_snippet(addr.id).unwrap());
        assert!(!store.delete_snippet(addr.id).unwrap());
        assert_eq!(store.counts(0).unwrap().snippets, 1);
    }

    #[test]
    fn rejects_missing_update() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.update_snippet(99, "不存在", "正文", 10).unwrap().is_none());
        assert_eq!(store.snippet_content(99).unwrap(), None);
    }

    #[test]
    fn preview_follows_search_hit() {
        let store = Store::open_in_memory().unwrap();
        let content = format!("{}发票{}", "前".repeat(300), "后".repeat(300));
        seed(&store, "账单", &content, 10);
        let item = &store.list_snippets(Some("发票")).unwrap()[0];
        assert!(item.preview.starts_with('…'));
        assert!(item.preview.contains("发票"));
        assert_eq!(item.chars, content.chars().count());
    }

    #[test]
    fn enforces_snippet_limit() {
        let store = Store::open_in_memory().unwrap();
        for i in 0..MAX_SNIPPETS {
            store.create_snippet(&format!("片段 {i}"), "正文", i as i64).unwrap();
        }
        assert!(matches!(
            store.create_snippet("超出", "正文", 0),
            Err(CreateSnippetError::LimitReached)
        ));
    }

    #[test]
    fn counts_include_snippets() {
        let store = Store::open_in_memory().unwrap();
        seed(&store, "一", "正文", 10);
        seed(&store, "二", "正文", 20);
        assert_eq!(store.counts(30).unwrap().snippets, 2);
    }
}
