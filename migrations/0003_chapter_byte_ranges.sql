-- chapters 加 byte_start / byte_end,持久化章节在原文中的位置。
-- 解析页重入时不再重跑 splitter,直接读 DB 还原编辑现场。
-- 老数据这两列为 NULL → 前端 store 检测到 NULL 时回退 splitter 重定位。

ALTER TABLE chapters ADD COLUMN byte_start INTEGER;
ALTER TABLE chapters ADD COLUMN byte_end INTEGER;
CREATE INDEX IF NOT EXISTS idx_chapters_upload_byte ON chapters(upload_id, byte_start);