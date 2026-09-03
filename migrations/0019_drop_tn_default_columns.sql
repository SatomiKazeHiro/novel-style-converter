-- migration 0019: 删 `transformation_novels.default_*` 三列。
--
-- 背景:
-- - migration 0008 加 default_model_config_id / default_prompt_id / default_mode 三列,
--   意图是给 tn 一个"默认配置"方便回填。
-- - 现状:唯一在用的 entry path `create_workflow` 不读这三个值(必填 prompt/model/mode),
--   `create_batch` + `BatchOverrides` 那条 fallback 路径已被删除;
-- - UI 上 `TransformationNovelDialog` 收集的三字段提交后也是死数据。
-- - 留着只会增加语义 / 复杂度 / 排查路径,跟"不 silent 兜底、不增加冗余"原则冲突。
--
-- 实施:跟 0006 / 0012 / 0013 一样,关 FK → 重建表 → 开 FK。
-- SQLite 不支持 ALTER TABLE DROP COLUMN 在 FK 启用时直接删带外键的列,
-- 重建表是稳妥做法。
--
-- 跟其他 destructive migration 一样:运行后原 default_* 值永久丢失;
-- 用户接受"重新测,删了不好吗"原则,这里不写回填逻辑。

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS transformation_novels_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    data_asset_id INTEGER NOT NULL REFERENCES data_assets(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL
);

INSERT INTO transformation_novels_new (id, data_asset_id, title, created_at)
SELECT id, data_asset_id, title, created_at FROM transformation_novels;

DROP TABLE transformation_novels;
ALTER TABLE transformation_novels_new RENAME TO transformation_novels;
CREATE INDEX IF NOT EXISTS idx_transformation_novels_data_asset ON transformation_novels(data_asset_id);

PRAGMA foreign_keys = ON;