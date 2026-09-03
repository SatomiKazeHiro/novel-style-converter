-- 把 transformation_novels.upload_id 改成 data_asset_id
-- INSERT 用 JOIN data_assets 把 upload_id → data_asset_id 一一对应
-- (现有数据保证每个 upload_id 在 data_assets 里都有一行,因为 0004 已经把已解析的 upload 抓出来了)

CREATE TABLE IF NOT EXISTS transformation_novels_new (
    id INTEGER PRIMARY KEY,
    data_asset_id INTEGER NOT NULL REFERENCES data_assets(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL
);

INSERT INTO transformation_novels_new (id, data_asset_id, title, created_at)
SELECT tn.id, da.id, tn.title, tn.created_at
FROM transformation_novels tn
JOIN data_assets da ON da.upload_id = tn.upload_id;

DROP TABLE transformation_novels;
ALTER TABLE transformation_novels_new RENAME TO transformation_novels;
CREATE INDEX IF NOT EXISTS idx_transformation_novels_data_asset ON transformation_novels(data_asset_id);
