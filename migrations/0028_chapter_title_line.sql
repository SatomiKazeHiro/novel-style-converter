-- chapter 加 title_line:标题文本在 upload.original_text 里的 0-based 行号。
-- NULL = 无原文坐标(仅 promote_workflow 转正的 AI 结果章节)。
-- 原始章节(fresh/committed)永远非 NULL。数据可清除,无 backfill。
ALTER TABLE chapters ADD COLUMN title_line INTEGER;
