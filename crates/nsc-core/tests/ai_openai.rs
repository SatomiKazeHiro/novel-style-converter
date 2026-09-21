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

/// 输入被 provider 内容审核拦截(实测现象:MiniMax 对某一章返回 422 new_sensitive,
/// 同一段正文连试 5 次全失败)→ 错误串必须说明"这是审核拦截"并给出可操作建议,
/// 而不是把原始 JSON 直接抛给用户。
#[tokio::test]
async fn moderation_block_explains_cause_and_next_steps() {
    let server = MockServer::start().await;
    // 与线上实际响应体一致(取自 ai_call_logs.error)
    let body = r#"{"type":"error","error":{"type":"unprocessable_entity_error","message":"input new_sensitive (1026)","http_code":"422"},"request_id":"06ffc0ff7cf3633450746124132ad06a"}"#;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(422).set_body_string(body))
        .mount(&server)
        .await;

    let err = provider_for(&server)
        .chat(req())
        .await
        .expect_err("422 应失败");
    let msg = err.to_string();

    assert!(
        msg.contains("内容被 provider 审核拦截"),
        "应点明是审核拦截: {msg}"
    );
    assert!(msg.contains("换用其它模型"), "应给出可操作建议: {msg}");
    assert!(msg.contains("重试通常仍会被拦下"), "应说明重试无效: {msg}");
    assert!(
        msg.contains("request_id"),
        "应提示可凭 request_id 反馈: {msg}"
    );
    assert!(msg.contains("06ffc0ff"), "应保留原始响应供排查: {msg}");
}

/// 422 但不是审核问题(其它输入不可处理错误)→ 保持原样,不误报成审核拦截。
#[tokio::test]
async fn non_moderation_422_is_passed_through() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_string(r#"{"error":{"message":"invalid temperature"}}"#),
        )
        .mount(&server)
        .await;

    let err = provider_for(&server)
        .chat(req())
        .await
        .expect_err("422 应失败");
    let msg = err.to_string();
    assert!(!msg.contains("审核拦截"), "非审核类 422 不应被误报: {msg}");
    assert!(
        msg.contains("invalid temperature"),
        "应原样保留响应体: {msg}"
    );
}

/// 审核关键词出现在非 422 状态码上时不套用该提示(避免扩大解释)。
#[tokio::test]
async fn moderation_keyword_on_non_422_is_not_special_cased() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(500).set_body_string("internal: sensitive check crashed"),
        )
        .mount(&server)
        .await;

    let err = provider_for(&server)
        .chat(req())
        .await
        .expect_err("500 应失败");
    let msg = err.to_string();
    assert!(!msg.contains("审核拦截"), "仅 422 才套用审核提示: {msg}");
    assert!(msg.contains("500"), "应保留状态码: {msg}");
}

/// 错误描述函数的厂商覆盖与边界:各厂商措辞都要能识别,普通错误不误伤。
#[test]
fn describe_provider_error_covers_vendors() {
    use nsc_core::ai::describe_provider_error;

    for body in [
        r#"{"error":{"message":"input new_sensitive (1026)"}}"#,
        r#"{"error":{"code":"content_policy_violation"}}"#,
        r#"{"error":{"message":"blocked by moderation"}}"#,
        r#"{"error":{"message":"flagged by safety system"}}"#,
    ] {
        let msg = describe_provider_error(422, body);
        assert!(
            msg.contains("审核拦截"),
            "应识别为审核拦截: {body} -> {msg}"
        );
    }
    // 普通错误原样透传
    assert_eq!(
        describe_provider_error(401, "unauthorized"),
        "http 401: unauthorized"
    );
    assert_eq!(
        describe_provider_error(429, "rate limited"),
        "http 429: rate limited"
    );
}
