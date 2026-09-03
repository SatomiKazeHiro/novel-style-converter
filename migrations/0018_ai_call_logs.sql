-- AI 调用日志:每次发起 LLM chat 调用,无论成功失败,落一行供 UI 看板 / 排查 / 计量。
--
-- 设计取舍:
-- - prompt / response 不存全文 —— 一章 50KB+,1000 章就让 DB 爆炸;只存前 10KB 预览 + 总字符数。
--   test_model 一次小调用预览即全文;transform 的全文留在 transformation_chapters.result_content。
-- - estimated_tokens_in 用 chars/2 粗估(zh-aware 经验值),UI 标注粗估让用户理解非精确。
-- - 实际 tokens 来自 provider usage(tokens_in / tokens_out 两列);缺 usage 时为 NULL,UI 显示无。
-- - status = success | failed;失败的 error 字段保留完整错误字符串(provider 错误 / 网络 / 反序列化)。
-- - business = transform_chapter | test_model 两类;未来可加 embedding / summarize 等。
-- - context_type / context_id 是软引用(无 FK)—— transformation_chapters 可能被删,日志仍要保留。
--   transformation_chapter 删了不影响本行可见,UI 详情页会显示业务对象已删除。

CREATE TABLE IF NOT EXISTS ai_call_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,                       -- ISO 8601 UTC
    business TEXT NOT NULL,                         -- transform_chapter | test_model
    -- 业务上下文(软引用,无 FK)
    context_type TEXT,                              -- transformation_chapter | NULL
    context_id INTEGER,                             -- transformation_chapter.id / NULL
    -- 模型配置(denormalized: 配置行可能 archive / 删了,日志仍要能看到当时调的是哪个)
    model_config_id INTEGER,
    model_name TEXT NOT NULL,                       -- 实际请求的 model 名
    base_url TEXT NOT NULL,                         -- provider endpoint
    -- 请求参数
    temperature REAL,
    max_tokens INTEGER,
    -- prompt 预览(完整 system / user 内容看 transformation_chapters / 原始调用上下文)
    system_preview TEXT,                            -- system message 前 10KB
    user_preview TEXT,                              -- user message 前 10KB
    system_size INTEGER NOT NULL DEFAULT 0,         -- 完整 system 字符数
    user_size INTEGER NOT NULL DEFAULT 0,           -- 完整 user 字符数
    -- Token
    estimated_tokens_in INTEGER,                    -- 预估值 (chars/2 启发式)
    actual_tokens_in INTEGER,                       -- API usage.prompt_tokens
    actual_tokens_out INTEGER,                      -- API usage.completion_tokens
    -- 结果
    status TEXT NOT NULL,                           -- success | failed
    response_preview TEXT,                          -- response 前 10KB
    response_size INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL,                    -- 从发起到收到结果(含失败路径)
    error TEXT                                      -- 失败时的错误字符串
);

-- 查询热点:
-- 1) 时间倒序(看板默认)
CREATE INDEX IF NOT EXISTS idx_ai_call_logs_created_at ON ai_call_logs(created_at DESC);
-- 2) 按业务过滤(transform_chapter / test_model)
CREATE INDEX IF NOT EXISTS idx_ai_call_logs_business ON ai_call_logs(business, created_at DESC);
-- 3) 按模型配置过滤(看某个 model 的所有调用)
CREATE INDEX IF NOT EXISTS idx_ai_call_logs_model_config ON ai_call_logs(model_config_id, created_at DESC);
-- 4) 按业务上下文反查(从 transformation_chapter 找历史 AI 调用)
CREATE INDEX IF NOT EXISTS idx_ai_call_logs_context ON ai_call_logs(context_type, context_id);
