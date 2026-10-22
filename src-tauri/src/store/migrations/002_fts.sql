-- 全文搜索：trigram 分词支持中文子串匹配（查询至少 3 个字符）
CREATE VIRTUAL TABLE clips_fts USING fts5(text, content = 'clips', content_rowid = 'id', tokenize = 'trigram');

INSERT INTO clips_fts (rowid, text) SELECT id, text FROM clips;

CREATE TRIGGER clips_fts_ai AFTER INSERT ON clips BEGIN
  INSERT INTO clips_fts (rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER clips_fts_ad AFTER DELETE ON clips BEGIN
  INSERT INTO clips_fts (clips_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER clips_fts_au AFTER UPDATE OF text ON clips BEGIN
  INSERT INTO clips_fts (clips_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO clips_fts (rowid, text) VALUES (new.id, new.text);
END;
