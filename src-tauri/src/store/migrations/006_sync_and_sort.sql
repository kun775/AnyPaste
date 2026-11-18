-- 永久转临时或墓碑复活为临时时保留脱离永久集合的时间戳，区分“主动取消永久”与“普通本机临时复制”
ALTER TABLE clips ADD COLUMN perm_removed_at INTEGER;

-- 常用片段统计实际粘贴次数，供频率排序使用
ALTER TABLE snippets ADD COLUMN paste_count INTEGER NOT NULL DEFAULT 0;

-- 用覆盖置顶与粘贴频率的局部索引替换早期列表索引，消除默认查询的临时排序
DROP INDEX IF EXISTS idx_clips_list;

CREATE INDEX idx_clips_pinned ON clips (is_permanent, pinned_at DESC, id DESC)
  WHERE deleted_at IS NULL AND pinned_at IS NOT NULL;

CREATE INDEX idx_clips_recent ON clips (is_permanent, last_copied_at DESC, id DESC)
  WHERE deleted_at IS NULL AND pinned_at IS NULL;

CREATE INDEX idx_clips_freq ON clips (is_permanent, paste_count DESC, last_copied_at DESC, id DESC)
  WHERE deleted_at IS NULL AND pinned_at IS NULL;

CREATE INDEX idx_snippets_freq ON snippets (paste_count DESC, updated_at DESC, id DESC);
