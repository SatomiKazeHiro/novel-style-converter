-- migration 0008: tn 默认配置
-- 已知 SQLite ALTER TABLE 不支持 IF NOT EXISTS; 二次执行靠 schema_versions 阻拦
ALTER TABLE transformation_novels
  ADD COLUMN default_model_config_id INTEGER REFERENCES model_configs(id);
ALTER TABLE transformation_novels
  ADD COLUMN default_prompt_id       INTEGER REFERENCES prompts(id);
ALTER TABLE transformation_novels
  ADD COLUMN default_mode            TEXT;
