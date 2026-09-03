-- model_configs 加 max_context INTEGER —— 模型最大上下文窗口(输入 tokens 上限)。
-- 默认 NULL 表示不强制校验(transformer 行为与历史一致)。
-- 设置后:transformer.rs 估算 estimated_tokens_in,超过则 Error::Validation 拒发,
-- 由前端显示给用户,不静默截断(避免"调 AI 没反应/截了一半"这种难以排查的隐式行为)。
ALTER TABLE model_configs ADD COLUMN max_context INTEGER;
