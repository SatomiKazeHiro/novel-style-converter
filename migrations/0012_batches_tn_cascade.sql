-- batches.transformation_novel_id 加 ON DELETE CASCADE。
-- 原 FK 只 REFERENCES,不带 cascade。删除 TN 时被反向引用拦下 → FOREIGN KEY constraint failed。
-- SQLite 不支持 ALTER TABLE 改 FK 约束,必须重建表。

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS batches_new (
    id                      INTEGER PRIMARY KEY,
    transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id) ON DELETE CASCADE,
    label                   TEXT,
    on_failure_policy       TEXT NOT NULL DEFAULT 'pause_and_review',
    status                  TEXT NOT NULL DEFAULT 'pending',
    created_at              TEXT NOT NULL,
    started_at              TEXT,
    ended_at                TEXT
);

INSERT INTO batches_new (id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at)
SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at FROM batches;

DROP TABLE batches;
ALTER TABLE batches_new RENAME TO batches;
CREATE INDEX IF NOT EXISTS idx_batches_tn      ON batches(transformation_novel_id);
CREATE INDEX IF NOT EXISTS idx_batches_status  ON batches(status);

PRAGMA foreign_keys = ON;