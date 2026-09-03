-- 拆分 upload 与 transformation_novel 概念。
-- chapters 挂在 upload 下(同一文件被多本 novel 共享),
-- transformation_novels 引用 upload,transformation_chapters 关联两者。
-- Phase 1 没真实数据,直接 drop 旧 novels/chapters/transformations 表重建。
-- model_configs / prompts 保持 0001 不动。

PRAGMA foreign_keys = OFF;

DROP TABLE IF EXISTS transformation_chapters;
DROP TABLE IF EXISTS transformation_novels;
DROP TABLE IF EXISTS chapters;
DROP TABLE IF EXISTS novels;

CREATE TABLE uploads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sha256 TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    uploaded_at TEXT NOT NULL,
    file_path TEXT NOT NULL,
    parsed_at TEXT
);
CREATE INDEX idx_uploads_sha256 ON uploads(sha256);

CREATE TABLE chapters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    upload_id INTEGER NOT NULL REFERENCES uploads(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    title TEXT NOT NULL,
    original_content TEXT NOT NULL,
    word_count INTEGER NOT NULL
);
CREATE INDEX idx_chapters_upload ON chapters(upload_id, idx);

CREATE TABLE transformation_novels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    upload_id INTEGER NOT NULL REFERENCES uploads(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_transformation_novels_upload ON transformation_novels(upload_id);

CREATE TABLE transformation_chapters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    mode TEXT NOT NULL,
    prompt_id INTEGER NOT NULL,
    model_config_id INTEGER NOT NULL,
    ctx_prev_original INTEGER NOT NULL,
    ctx_prev_transformed INTEGER NOT NULL,
    ctx_next_original INTEGER NOT NULL,
    status TEXT NOT NULL,
    result_content TEXT,
    tokens_in INTEGER,
    tokens_out INTEGER,
    error TEXT,
    started_at TEXT,
    completed_at TEXT
);
CREATE INDEX idx_transformation_chapters_novel ON transformation_chapters(transformation_novel_id);
CREATE INDEX idx_transformation_chapters_chapter ON transformation_chapters(chapter_id);
CREATE INDEX idx_transformation_chapters_status ON transformation_chapters(status);

PRAGMA foreign_keys = ON;