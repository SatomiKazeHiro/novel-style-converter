-- 批号表(独立 entity)。每次批量转换一条。
CREATE TABLE IF NOT EXISTS batches (
  id                      INTEGER PRIMARY KEY,
  transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id),
  label                   TEXT,
  on_failure_policy       TEXT NOT NULL DEFAULT 'pause_and_review',
  status                  TEXT NOT NULL DEFAULT 'pending',
  created_at              TEXT NOT NULL,
  started_at              TEXT,
  ended_at                TEXT
);
CREATE INDEX IF NOT EXISTS idx_batches_tn      ON batches(transformation_novel_id);
CREATE INDEX IF NOT EXISTS idx_batches_status  ON batches(status);
