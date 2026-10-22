CREATE TABLE clips (
  id             INTEGER PRIMARY KEY,
  uuid           TEXT    NOT NULL UNIQUE,              -- 同步用全局 ID
  kind           TEXT    NOT NULL CHECK (kind IN ('text', 'rich', 'image', 'files')),
  content_hash   TEXT    NOT NULL UNIQUE,              -- blake3，去重依据
  text           TEXT,                                 -- 纯文本；文件列表为换行分隔的路径
  html           TEXT,                                 -- HTML 片段
  rtf            TEXT,
  image_file     TEXT,                                 -- 图片文件名，位于数据目录 images/ 下
  thumb          BLOB,                                 -- 列表用 PNG 缩略图
  size_bytes     INTEGER NOT NULL,
  source_app     TEXT,                                 -- 复制时前台进程名
  is_permanent   INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,                     -- 以下时间均为 Unix 毫秒
  last_copied_at INTEGER NOT NULL,
  expires_at     INTEGER,                              -- 永久条目为 NULL
  updated_at     INTEGER NOT NULL,                     -- 同步冲突判定
  deleted_at     INTEGER                               -- 墓碑，仅已同步的永久条目使用
);

CREATE INDEX idx_clips_list ON clips (is_permanent, deleted_at, last_copied_at DESC);
CREATE INDEX idx_clips_expire ON clips (expires_at) WHERE is_permanent = 0;

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
