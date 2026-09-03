-- Migration 0022: chapters 加 edited_at 标记
-- 背景:
-- - v15 起 chapter.body 是自包含 TEXT(data_asset / workflow / upload 互相独立)
-- - 用户在 DataAsset.vue 编辑章节正文(任意 kind)后,改的是当前 da 的 chapter.body,
--   不影响 source chapter / workflow_result_chapters / workflow 状态。
-- - source_kind 标记内容来源(transformed / original),与"是否被用户编辑"是两个维度。
-- - edited_at = NULL 表示从未被用户编辑;非 NULL = 上次编辑时间(RFC3339)。
-- - 测试阶段允许破坏性改动,不写回滚脚本。
ALTER TABLE chapters ADD COLUMN edited_at TEXT;

CREATE INDEX IF NOT EXISTS idx_chapters_edited_at ON chapters(data_asset_id, edited_at);
