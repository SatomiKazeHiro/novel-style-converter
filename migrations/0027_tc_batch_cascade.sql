-- Migration 0027: transformation_chapters.batch_id 加 ON DELETE CASCADE。
-- 背景:
-- - 0010 给 transformation_chapters.batch_id 加的是裸 REFERENCES,删除 batch 时被反向引用拦下
--   → FOREIGN KEY constraint failed。
-- - SQLite 不支持 ALTER TABLE 改 FK 约束,只能重建表。
-- - 工作流删除的语义:batch 一并删 → 其 transformation_chapters 也跟着删。
--   workflow_results / chapter_previews 已经在 0011 / 0024 里挂好 CASCADE,无需再动。
-- - 测试阶段允许破坏性改动,不写回滚脚本。

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS transformation_chapters_new (
    id                    INTEGER PRIMARY KEY,
    transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id) ON DELETE CASCADE,
    chapter_id            INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    mode                  TEXT NOT NULL,
    prompt_id             INTEGER NOT NULL,
    model_config_id       INTEGER NOT NULL,
    ctx_prev_original     INTEGER NOT NULL,
    ctx_prev_transformed  INTEGER NOT NULL,
    ctx_next_original     INTEGER NOT NULL,
    status                TEXT NOT NULL,
    result_content        TEXT,
    tokens_in             INTEGER,
    tokens_out            INTEGER,
    error                 TEXT,
    started_at            TEXT,
    completed_at          TEXT,
    batch_id              INTEGER REFERENCES batches(id) ON DELETE CASCADE,
    style_ref_chapter_id  INTEGER REFERENCES chapters(id)
);

INSERT INTO transformation_chapters_new
    (id, transformation_novel_id, chapter_id, mode, prompt_id, model_config_id,
     ctx_prev_original, ctx_prev_transformed, ctx_next_original,
     status, result_content, tokens_in, tokens_out, error, started_at, completed_at,
     batch_id, style_ref_chapter_id)
SELECT
    id, transformation_novel_id, chapter_id, mode, prompt_id, model_config_id,
     ctx_prev_original, ctx_prev_transformed, ctx_next_original,
     status, result_content, tokens_in, tokens_out, error, started_at, completed_at,
     batch_id, style_ref_chapter_id
FROM transformation_chapters;

DROP TABLE transformation_chapters;
ALTER TABLE transformation_chapters_new RENAME TO transformation_chapters;

CREATE INDEX IF NOT EXISTS idx_transformation_chapters_novel   ON transformation_chapters(transformation_novel_id);
CREATE INDEX IF NOT EXISTS idx_transformation_chapters_chapter ON transformation_chapters(chapter_id);
CREATE INDEX IF NOT EXISTS idx_transformation_chapters_status  ON transformation_chapters(status);
CREATE INDEX IF NOT EXISTS idx_tc_batch                       ON transformation_chapters(batch_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tc_batch_chapter
  ON transformation_chapters(batch_id, chapter_id)
  WHERE batch_id IS NOT NULL;

PRAGMA foreign_keys = ON;
