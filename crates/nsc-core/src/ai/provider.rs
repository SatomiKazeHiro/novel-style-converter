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

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}

/// LLM 适配层抽象 —— 把 `ChatRequest` 转成对底座模型的实际 HTTP / RPC 调用,返回内容 + token 计数。
///
/// 实现方要求 `Send + Sync`,因为 `DefaultTransformer` 通过 `Box<dyn AiProvider>`
/// 跨 worker 线程持有。所有实现都应:
/// - 用 `req.model` / `req.max_tokens` / `req.temperature` 完整转发请求参数
/// - 把上游 `usage.prompt_tokens` / `completion_tokens` 落到 `ChatResponse.tokens_in/out`
/// - 非 2xx / 空响应 / JSON 解析失败 → 返回 `Error::Ai(String)`
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;
}