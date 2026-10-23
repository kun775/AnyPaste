-- 置顶：非空表示已置顶，按置顶时间倒序排在列表最前；仅本机有效，不参与同步
ALTER TABLE clips ADD COLUMN pinned_at INTEGER;
