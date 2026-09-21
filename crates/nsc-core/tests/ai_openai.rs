//! `OpenAiProvider` 的回归测试 —— 重点覆盖"provider 不返回 usage"这条路径。
//!
//! 背景:`ChatResponse.tokens_in/out` 曾经是 `i32`,而解析侧把缺 `usage` 当**致命错误**
//! (`ok_or_else(|| Error::Ai("usage field missing in response"))`)。结果是:凡不返回
//! usage 的 OpenAI 兼容实现(流式代理、部分自建网关),每次调用都直接失败,整本书的
//! 章节全部转换不了 —— 而 usage 只是记账信息,内容本身完全可用。
//!
//! 现在 `tokens_*` 是 `Option<i32>`:缺 usage → 落 NULL,调用照常成功。
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, OpenAiProvider, Role};

fn req() -> ChatRequest {
    ChatRequest {
        model: "test-model".into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "把这一章压缩一下".into(),
        }],
        temperature: None,
        max_tokens: None,
        reasoning_effort: None,
        thinking: None,
    }
}

fn provider_for(server: &MockServer) -> OpenAiProvider {
    OpenAiProvider::new(server.uri(), "test-key".into()).expect("provider")
}

/// 主回归:响应里**没有** `usage` 字段时,调用必须成功,且 tokens 为 None。
#[tokio::test]
async fn missing_usage_still_succeeds_with_none_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{ "message": { "content": "压缩后的正文" } }]
            // 故意不提供 usage —— 这正是修复前会整章失败的情形
        })))
        .mount(&server)
        .await;

    let out = provider_for(&server)
        .chat(req())
        .await
        .expect("缺 usage 不应导致调用失败");

    assert_eq!(out.content, "压缩后的正文");
    assert_eq!(
        out.tokens_in, None,
        "缺 usage 时 tokens_in 应为 None(落库为 NULL)"
    );
    assert_eq!(out.tokens_out, None, "缺 usage 时 tokens_out 应为 None");
}

/// 正常情形:有 usage 时如实解析。
#[tokio::test]
async fn usage_is_parsed_when_present() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{ "message": { "content": "压缩后的正文" } }],
            "usage": { "prompt_tokens": 123, "completion_tokens": 45 }
        })))
        .mount(&server)
        .await;

    let out = provider_for(&server).chat(req()).await.expect("应成功");
    assert_eq!(out.tokens_in, Some(123));
    assert_eq!(out.tokens_out, Some(45));
}

/// usage 存在但只回一个字段(部分网关如此)—— 缺的按 serde default 落 0,不整体失败。
#[tokio::test]
async fn partial_usage_defaults_missing_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{ "message": { "content": "正文" } }],
            "usage": { "prompt_tokens": 10 }
        })))
        .mount(&server)
        .await;

    let out = provider_for(&server).chat(req()).await.expect("应成功");
    assert_eq!(out.tokens_in, Some(10));
    assert_eq!(out.tokens_out, Some(0), "缺失的字段按 serde default 落 0");
}

/// 空 choices 仍然是**致命错误** —— 那是模型真没产出内容,与"缺记账"性质不同。
#[tokio::test]
async fn empty_choices_is_still_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [],
            "usage": { "prompt_tokens": 1, "completion_tokens": 0 }
        })))
        .mount(&server)
        .await;

    let err = provider_for(&server)
        .chat(req())
        .await
        .expect_err("空 choices 应失败");
    assert!(
        err.to_string().contains("empty choices"),
        "错误应说明是空 choices,实际: {err}"
    );
}

/// 非 2xx 仍是致命错误 —— 鉴权/网络问题不能被"usage 宽松处理"顺带掩盖。
#[tokio::test]
async fn non_2xx_is_still_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let err = provider_for(&server)
        .chat(req())
        .await
        .expect_err("401 应失败");
    assert!(
        err.to_string().contains("401"),
        "错误应带状态码,实际: {err}"
    );
}
