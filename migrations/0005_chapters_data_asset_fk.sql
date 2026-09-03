-- 把 chapters.upload_id 改成 data_asset_id(并加 FK 约束)
-- 关键:id 保持不变(INSERT 直接搬 id),chapter_id 引用 chapters.id 的外键仍有效
--
-- byte_start / byte_end 保留为 nullable(老数据可能 NULL),应用层 ChapterSegmentRow
-- 已有 Option<i64> 处理。
--
-- 同时把 uploads.parsed_at 拆掉(语义迁到 data_assets.parsed_at),并把原文整篇
-- 落到 uploads.original_text —— 这是「State 1 = 原始文件」的核心:后续的章节
-- 切片都从这一字段 byte offset 取,不再重复存正文。

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS uploads_new (
    id INTEGER PRIMARY KEY,
    sha256 TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    uploaded_at TEXT NOT NULL,
    file_path TEXT NOT NULL,
    original_text TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_uploads_sha256 ON uploads(sha256);

INSERT INTO uploads_new (id, sha256, filename, byte_size, uploaded_at, file_path, original_text)
SELECT id, sha256, filename, byte_size, uploaded_at, file_path, '' FROM uploads;

DROP TABLE uploads;
ALTER TABLE uploads_new RENAME TO uploads;

CREATE TABLE IF NOT EXISTS chapters_new (
    id INTEGER PRIMARY KEY,
    data_asset_id INTEGER NOT NULL REFERENCES data_assets(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    title TEXT NOT NULL,
    byte_start INTEGER,
    byte_end INTEGER,
    word_count INTEGER NOT NULL,
    UNIQUE(data_asset_id, idx)
);

INSERT INTO chapters_new (id, data_asset_id, idx, title, byte_start, byte_end, word_count)
SELECT c.id, da.id, c.idx, c.title, c.byte_start, c.byte_end, c.word_count
FROM chapters c
JOIN data_assets da ON da.upload_id = c.upload_id;

DROP TABLE chapters;
ALTER TABLE chapters_new RENAME TO chapters;
CREATE INDEX IF NOT EXISTS idx_chapters_data_asset ON chapters(data_asset_id, idx);

PRAGMA foreign_keys = ON;