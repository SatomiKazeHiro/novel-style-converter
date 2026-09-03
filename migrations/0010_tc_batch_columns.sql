-- transformation_chapters 增 2 列(NULL 兼容存量历史散点)
-- 文件名 0010(非 0008):避免与 transformation_novels 默认列 migration 同版本冲突。
ALTER TABLE transformation_chapters
  ADD COLUMN batch_id             INTEGER REFERENCES batches(id);
ALTER TABLE transformation_chapters
  ADD COLUMN style_ref_chapter_id INTEGER REFERENCES chapters(id);

CREATE INDEX IF NOT EXISTS idx_tc_batch ON transformation_chapters(batch_id);
