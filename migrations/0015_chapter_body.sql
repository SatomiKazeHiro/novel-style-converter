-- Step 15: chapter 重设计 — 存正文 body、丢弃 byte 偏移；data_assets 弱化 upload 引用
-- 单一方向：chapter.body 是自包含的；data_asset 仅审计式引用 upload_id（无 FK、无 UNIQUE）

ALTER TABLE chapters ADD COLUMN body TEXT NOT NULL DEFAULT '';

ALTER TABLE chapters DROP COLUMN byte_start;
ALTER TABLE chapters DROP COLUMN byte_end;

PRAGMA foreign_keys = OFF;

CREATE TABLE data_assets_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    upload_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    parsed_at TEXT NOT NULL,
    source_filename TEXT NOT NULL DEFAULT ''
);
INSERT INTO data_assets_new (id, upload_id, title, parsed_at, source_filename)
SELECT id, upload_id, title, parsed_at, '' FROM data_assets;
DROP TABLE data_assets;
ALTER TABLE data_assets_new RENAME TO data_assets;
CREATE INDEX IF NOT EXISTS idx_data_assets_upload ON data_assets(upload_id);

PRAGMA foreign_keys = ON;
