-- Migration 0021: data_assets 加 kind + 溯源字段 + chapters 加 source_kind/source_chapter_id
-- 背景:
-- - v15 已经把 data_assets.upload_id 改成软引用,本章在此基础上把"派生数据资产"作为同表的不同 kind
-- - 现有 data_assets 自动继承 kind='source'(default)
-- - 测试阶段允许破坏性改动,不写回滚脚本
ALTER TABLE data_assets ADD COLUMN kind TEXT NOT NULL DEFAULT 'source';
ALTER TABLE data_assets ADD COLUMN source_workflow_id INTEGER REFERENCES batches(id) ON DELETE SET NULL;
ALTER TABLE data_assets ADD COLUMN source_data_asset_id INTEGER REFERENCES data_assets(id) ON DELETE SET NULL;
ALTER TABLE data_assets ADD COLUMN note TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_data_assets_kind ON data_assets(kind);
CREATE INDEX IF NOT EXISTS idx_data_assets_source_workflow ON data_assets(source_workflow_id);

ALTER TABLE chapters ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'original';
ALTER TABLE chapters ADD COLUMN source_chapter_id INTEGER REFERENCES chapters(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_chapters_source_kind ON chapters(data_asset_id, source_kind);