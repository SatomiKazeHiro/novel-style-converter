use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde::Deserialize;

use crate::ai::{AiProvider, ChatRequest, ChatResponse, Role};
use crate::error::{Error, Result};

#[derive(Debug, Deserialize)]
struct WireResponse {
    choices: Vec<WireChoice>,
    usage: Option<WireUsage>,
}
#[derive(Debug, Deserialize)]
struct WireChoice { message: WireMessage }
#[derive(Debug, Deserialize)]
struct WireMessage { content: String }
#[derive(Debug, Deserialize)]
struct WireUsage {
    #[serde(default)] prompt_tokens: i32,
    #[serde(default)] completion_tokens: i32,
}

/// wire 格式:`{"type": "..."}` —— Anthropic Messages API 风格思考控制字段。
/// OpenAI Chat Completions 协议服务端会忽略未知字段,MiniMax 等 Anthropic 风格
/// provider 会接受并据此开关 thinking 输出。
#[derive(Debug, serde::Serialize)]
struct ThinkingWire<'a> {
    #[serde(rename = "type")]
    t: &'a str,
}

#[derive(Debug, serde::Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireOutMsg<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingWire<'a>>,
}
#[derive(Debug, serde::Serialize)]
struct WireOutMsg<'a> { role: &'a str, content: &'a str }

#[derive(Clone)]
pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl OpenAiProvider {
    /// 构造一个 OpenAI 兼容 provider。`base_url` 是根地址,不带路径
    /// (内部会自动拼 `/chat/completions`)。`api_key` 透传到 `Authorization: Bearer ...`。
    /// 失败时(当前主要是 `reqwest::Client` 构造失败)返回 `Error::Http`。
    pub fn new(base_url: String, api_key: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { client, base_url, api_key })
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let out: Vec<WireOutMsg> = req.messages.iter().map(|m| WireOutMsg {
            role: match m.role { Role::System => "system", Role::User => "user", Role::Assistant => "assistant" },
            content: &m.content,
        }).collect();
        let body = WireRequest {
            model: &req.model,
            messages: out,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            reasoning_effort: req.reasoning_effort.as_deref(),
            thinking: req.thinking.as_deref().map(|t| ThinkingWire { t }),
        };
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|e| Error::Ai(format!("bad api key header: {e}")))?);

        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await?;
            return Err(Error::Ai(format!("http {status}: {text}")));
        }
        let wire: WireResponse = resp.json().await?;
        let content = wire.choices.into_iter().next()
            .ok_or_else(|| Error::Ai("empty choices".into()))?
            .message.content;
        let (in_t, out_t) = wire.usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .ok_or_else(|| Error::Ai("usage field missing in response".into()))?;
        Ok(ChatResponse {
            content,
            tokens_in: in_t,
            tokens_out: out_t,
        })
    }
}