-- Step 16: model_configs 软删
-- - 加 archived 列(默认 0 = 正常)。
-- - list_models 默认过滤 archived=0;按 id 直接 get() 仍返回归档行(保留历史 tc 引用)。
-- - delete_model 是 UPDATE 软删,顺手把 api_key 抹掉(避免密钥随归档条目泄露)。
-- - 保留 base_url / model / concurrency 等纯元数据,旧 tc 仍可读出归档 model 用于显示。

ALTER TABLE model_configs ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_model_configs_archived ON model_configs(archived);
