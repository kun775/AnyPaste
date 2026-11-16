-- 粘贴次数：用于按使用频率排序；只统计通过 AnyPaste 实际粘贴的次数，不参与同步
ALTER TABLE clips ADD COLUMN paste_count INTEGER NOT NULL DEFAULT 0;