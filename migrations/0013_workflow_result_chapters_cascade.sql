-- workflow_result_chapters.chapter_id 加 ON DELETE CASCADE。
-- 删 data_asset 时 chapters 被级联(0005),如果 workflow_result_chapters
-- 没接 cascade,FK 约束会拦下整个删除链。
-- SQLite 不支持 ALTER TABLE 改 FK 约束,必须重建表。

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS workflow_result_chapters_new (
    id                 INTEGER PRIMARY KEY,
    workflow_result_id INTEGER NOT NULL REFERENCES workflow_results(id) ON DELETE CASCADE,
    chapter_id         INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    content            TEXT,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    UNIQUE(workflow_result_id, chapter_id)
);

INSERT INTO workflow_result_chapters_new (id, workflow_result_id, chapter_id, content, created_at, updated_at)
SELECT id, workflow_result_id, chapter_id, content, created_at, updated_at FROM workflow_result_chapters;

DROP TABLE workflow_result_chapters;
ALTER TABLE workflow_result_chapters_new RENAME TO workflow_result_chapters;

PRAGMA foreign_keys = ON;
