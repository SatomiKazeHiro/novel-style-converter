-- Step 17: prompts 软删
-- - 加 archived 列(默认 0 = 正常)。
-- - list_prompts 默认过滤 archived=0;按 id 直接 get() 仍返回归档行(保留历史 tc 引用)。
-- - delete_prompt 是 UPDATE 软删。
-- - builtin 行(id=1 / 2)用户可软删,行保留;seed_builtin_if_empty 看到 count >= 1 永远不再种。
-- - 与 model 软删逻辑对齐(api_key 那条不适用 —— prompt 没有密钥)。

ALTER TABLE prompts ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_prompts_archived ON prompts(archived);
