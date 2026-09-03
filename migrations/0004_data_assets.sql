-- 把 uploads.parsed_at 拆出来到独立 data_assets 表
-- 这步在 upgrade 路径上:旧 db 已经有 uploads.parsed_at;新 db 没这个字段

CREATE TABLE IF NOT EXISTS data_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    upload_id INTEGER NOT NULL UNIQUE REFERENCES uploads(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    parsed_at TEXT NOT NULL,
    locked_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_data_assets_upload ON data_assets(upload_id);

-- 把已解析的 upload 抓出来。新 db 没数据时 INSERT 不影响。
INSERT OR IGNORE INTO data_assets (upload_id, title, parsed_at)
SELECT id, filename, parsed_at FROM uploads WHERE parsed_at IS NOT NULL;