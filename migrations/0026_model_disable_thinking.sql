-- model_configs 加 disable_thinking INTEGER —— 用户主动关闭模型思考的开关。
-- 0 = 模型自决(default);1 = 主动禁用思考。
-- 仅对官方支持禁用思考的模型生效:
--   - OpenAI 新一代(gpt-5 / o4-mini 等)支持 reasoning_effort:"none"
--   - toggle 类型的 reasoning_options 模型也走 None 语义
-- 不支持禁用思考的模型(纯努力等级 / 内置思考型),该字段即便置 1 也不生效 —— UI 上压根不让选。
-- 由 DefaultTransformer 在构造 ChatRequest 时把 disable_thinking 映射成
-- reasoning_effort: Some("none"),再交给 OpenAiProvider 写入请求体。
ALTER TABLE model_configs ADD COLUMN disable_thinking INTEGER NOT NULL DEFAULT 0;
