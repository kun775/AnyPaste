use rusqlite::{params, Row};
use serde::Serialize;

use super::{list::escape_like, Store};

/// 列表只返回预览，避免大段内容占用 IPC。
const PREVIEW_CHARS: i64 = 400;
/// 片段数量按日常使用规模设计，不额外分页。
const MAX_SNIPPETS: i64 = 200;

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
        let pattern = match search.map(str::trim).filter(|s| !s.is_empty()) {
            Some(q) => format!("%{}%", escape_like(q)),
            None => "%".into(),
        };
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, title, substr(content, 1, ?1), length(content), updated_at
             FROM snippets
             WHERE title LIKE ?2 ESCAPE '\\' OR content LIKE ?2 ESCAPE '\\'
             ORDER BY updated_at DESC, id DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![PREVIEW_CHARS, pattern, MAX_SNIPPETS], row_to_item)?;
        rows.collect()
    }

    pub fn snippet_count(&self) -> rusqlite::Result<usize> {
        let n: i64 = self.conn().query_row("SELECT COUNT(*) FROM snippets", [], |r| r.get(0))?;
        Ok(n as usize)
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

    /// 新建片段（`id` 为空）或更新已有片段。
    pub fn save_snippet(
        &self,
        id: Option<i64>,
        title: &str,
        content: &str,
        now: i64,
    ) -> rusqlite::Result<SnippetItem> {
        let conn = self.conn();
        let id = match id {
            Some(id) => {
                let n = conn.execute(
                    "UPDATE snippets SET title = ?2, content = ?3, updated_at = ?4 WHERE id = ?1",
                    params![id, title, content, now],
                )?;
                if n == 0 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
                id
            }
            None => {
                conn.execute(
                    "INSERT INTO snippets (title, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                    params![title, content, now],
                )?;
                conn.last_insert_rowid()
            }
        };
        conn.query_row(
            "SELECT id, title, substr(content, 1, ?2), length(content), updated_at FROM snippets WHERE id = ?1",
            params![id, PREVIEW_CHARS],
            row_to_item,
        )
    }

    pub fn delete_snippet(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self.conn().execute("DELETE FROM snippets WHERE id = ?1", [id])? > 0)
    }
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
        store.save_snippet(None, title, content, now).unwrap()
    }

    #[test]
    fn creates_searches_updates_and_deletes() {
        let store = Store::open_in_memory().unwrap();
        let mail = seed(&store, "工作邮件", "你好，附件是本周的进度报告。", 10);
        let addr = seed(&store, "公司地址", "上海市浦东新区示例路 88 号", 20);
        assert_eq!(store.snippet_count().unwrap(), 2);
        assert_eq!(store.list_snippets(None).unwrap()[0].id, addr.id);
        assert_eq!(store.list_snippets(Some("进度")).unwrap()[0].id, mail.id);
        assert_eq!(store.list_snippets(Some("公司地址")).unwrap()[0].id, addr.id);
        assert!(store.list_snippets(Some("不存在")).unwrap().is_empty());

        let updated = store.save_snippet(Some(mail.id), "周报邮件", "新的正文", 30).unwrap();
        assert_eq!((updated.title.as_str(), updated.preview.as_str()), ("周报邮件", "新的正文"));
        assert_eq!(store.snippet_content(mail.id).unwrap().as_deref(), Some("新的正文"));

        assert!(store.delete_snippet(addr.id).unwrap());
        assert!(!store.delete_snippet(addr.id).unwrap());
        assert_eq!(store.snippet_count().unwrap(), 1);
    }

    #[test]
    fn rejects_missing_update() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.save_snippet(Some(99), "不存在", "正文", 10).is_err());
        assert_eq!(store.snippet_content(99).unwrap(), None);
    }

    #[test]
    fn counts_include_snippets() {
        let store = Store::open_in_memory().unwrap();
        seed(&store, "一", "正文", 10);
        seed(&store, "二", "正文", 20);
        assert_eq!(store.counts(30).unwrap().snippets, 2);
    }
}
