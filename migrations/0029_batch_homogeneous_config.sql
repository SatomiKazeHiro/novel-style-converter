-- Migration key: 0028_batch_homogeneous_config (version 注册用)
-- Migration filename: 0029_batch_homogeneous_config.sql (task 编号)
-- 两者错位是 plan 的有意设计:key 名保持与已注册 SCHEMAS 单调递增顺序,
-- 文件名按 task 编号递进。不要"修正"成一致 — schema_versions 表存的是 key。
-- batches 补「同质配置」字段:batch 创建时统一采用同一套 prompt/model/ctx/mode。
-- stopped batch append 章节时,从 batch 字段直接读,无需反查 tc 行。
ALTER TABLE batches ADD COLUMN prompt_id INTEGER;
ALTER TABLE batches ADD COLUMN model_config_id INTEGER;
ALTER TABLE batches ADD COLUMN mode TEXT;
ALTER TABLE batches ADD COLUMN ctx_prev_original INTEGER;
ALTER TABLE batches ADD COLUMN ctx_prev_transformed INTEGER;
ALTER TABLE batches ADD COLUMN ctx_next_original INTEGER;
ALTER TABLE batches ADD COLUMN ctx_next_transformed INTEGER;

-- 旧数据 backfill:从该 batch 下任意一个 tc 行取(业务上同质)。
-- 不存在 tc 行的 batch 留 NULL(理论上不该有;防御性)。
UPDATE batches SET
  prompt_id = (SELECT prompt_id FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  model_config_id = (SELECT model_config_id FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  mode = (SELECT mode FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_prev_original = (SELECT ctx_prev_original FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_prev_transformed = (SELECT ctx_prev_transformed FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_next_original = (SELECT ctx_next_original FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1),
  ctx_next_transformed = (SELECT ctx_next_transformed FROM transformation_chapters WHERE batch_id = batches.id LIMIT 1);