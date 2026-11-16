-- 常用片段：用户主动保存的文本，可随时从弹出列表粘贴；只保存在本机
CREATE TABLE snippets (
  id         INTEGER PRIMARY KEY,
  title      TEXT NOT NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX idx_snippets_updated ON snippets (updated_at DESC, id DESC);
