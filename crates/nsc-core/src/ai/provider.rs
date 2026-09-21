use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role { System, User, Assistant }

#[derive(Debug, Clone)]
pub struct ChatMessage { pub role: Role, pub content: String }

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    /// OpenAI o1/o3/o4-mini 风格的努力等级 —— None = 不传该字段。
    /// 由 OpenAiProvider 在请求体里塞 `reasoning_effort`: <value>。
    /// 非 OpenAI 兼容协议的模型(如 Anthropic 原生)忽略该字段。
    pub reasoning_effort: Option<String>,
    /// Anthropic Messages API 风格的思考控制字段 —— 发到 wire 时变成 `thinking: {\"type\": ...}`。
    /// 当前只为 MiniMax 模型自动填 \"disabled\"(其协议上正确的字段名),
    /// OpenAI Chat Completions 协议的服务端会忽略未知字段。
    pub thinking: Option<String>,
}

/// 把 provider 的非 2xx 响应体转成**可操作**的错误信息。
///
/// ## 为什么需要
/// 原先直接把原始响应体塞进错误串,用户在 UI 上看到的是
/// `unprocessable_entity_error (1026)` + 一段 JSON —— 既不知道是**哪一类**失败,
/// 也不知道该怎么办(实测:某章被 MiniMax 输入审核拦下,连试 5 次全失败,
/// 用户只能看到原始 JSON)。
///
/// ## 覆盖范围(有意收窄)
/// 只识别一种**明确且需要不同处置**的情形:输入内容被 provider 审核拦截。
/// 其余错误(401 / 429 / 5xx / 网络)保持原样透传 —— 不去解析各家厂商的全部
/// 错误码文案,避免把自己绑死在某个 provider 的措辞上(换一家就失效)。
///
/// 判据取"状态码 + 关键词"而不是只匹配文案:`422` 是输入不可处理的通用码,
/// 关键词覆盖 MiniMax(`new_sensitive`)、OpenAI(`content_policy`)、
/// Anthropic(`invalid_request` 侧的内容策略)、以及通用 moderation 措辞。
pub fn describe_provider_error(status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    let looks_like_moderation = lower.contains("new_sensitive")
        || lower.contains("sensitive")
        || lower.contains("content_policy")
        || lower.contains("content policy")
        || lower.contains("moderation")
        || lower.contains("safety");
    if status == 422 && looks_like_moderation {
        return format!(
            "内容被 provider 审核拦截(输入侧)。这是 provider 服务端的内容审核,不是请求格式或上下文长度问题;\
             同一段正文重试通常仍会被拦下(deterministic),建议:\
             ① 给这一章换用其它模型 / provider;\
             ② 或调小该章的上下文邻章数后再试;\
             ③ 若确认正文无敏感内容,可拿原始响应里的 request_id 向 provider 反馈误判。\
             原始响应: {body}"
        );
    }
    format!("http {status}: {body}")
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    /// 上游 `usage.prompt_tokens`。**None = provider 没返回 usage**。
    ///
    /// 有意做成 Option:并非所有 OpenAI 兼容实现都返回 usage(流式代理、部分
    /// 自建网关会省略)。缺 usage 只是"记账缺一项",不影响对话内容本身 ——
    /// 因此不得据此判定调用失败,落库时 `actual_tokens_*` 记 NULL。
    pub tokens_in: Option<i32>,
    /// 上游 `usage.completion_tokens`;语义同 `tokens_in`。
    pub tokens_out: Option<i32>,
}

/// LLM 适配层抽象 —— 把 `ChatRequest` 转成对底座模型的实际 HTTP / RPC 调用,返回内容 + token 计数。
///
/// 实现方要求 `Send + Sync`,因为 `DefaultTransformer` 通过 `Box<dyn AiProvider>`
/// 跨 worker 线程持有。所有实现都应:
/// - 用 `req.model` / `req.max_tokens` / `req.temperature` 完整转发请求参数
/// - 把上游 `usage.prompt_tokens` / `completion_tokens` 落到 `ChatResponse.tokens_in/out`;
///   provider 不返回 usage 时落 `None`(**不要**因此报错 —— 见 `ChatResponse` 字段注释)
/// - 非 2xx / 空响应 / JSON 解析失败 → 返回 `Error::Ai(String)`
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;
}