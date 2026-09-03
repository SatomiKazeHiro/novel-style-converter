use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, OpenAiProvider, Role};
use nsc_core::db::Db;
use nsc_core::models::{AiCallBusiness, AiCallStatus, ModelConfig, NewModelConfig};
use nsc_core::recorder::{AiCallEvent, AiCallRecorder};
use serde::{Deserialize, Serialize};
use tauri::State;

/// 默认列表(仅 `archived = 0`)。各 dialog 拉模型下拉用。
#[tauri::command]
pub fn list_models(db: State<'_, Arc<Db>>) -> Result<Vec<ModelConfig>, String> {
    db.model_configs().list(false).map_err(|e| e.to_string())
}

/// 含归档的列表 —— Models.vue 顶部“显示已归档”开关用。
#[tauri::command]
pub fn list_models_including_archived(db: State<'_, Arc<Db>>) -> Result<Vec<ModelConfig>, String> {
    db.model_configs().list(true).map_err(|e| e.to_string())
}

/// 软删:`archived = 1` + `api_key = ''`。行保留以便历史 tc / tn 引用解析。
#[tauri::command]
pub fn delete_model(db: State<'_, Arc<Db>>, id: i64) -> Result<(), String> {
    db.model_configs().archive(id).map_err(|e| e.to_string())
}

/// 取消软删。注意:被抹掉的 `api_key` 不会自动恢复,用户需重新编辑保存。
#[tauri::command]
pub fn restore_model(db: State<'_, Arc<Db>>, id: i64) -> Result<(), String> {
    db.model_configs().restore(id).map_err(|e| e.to_string())
}

/// 前端 wrapper 透传 snake_case payload,DTO 字段保持 Rust 原名。
#[derive(Debug, Deserialize)]
pub struct ModelConfigDto {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub max_context: Option<i32>,
    pub temperature: Option<f32>,
    /// 用户主动关闭思考的开关(serde 默认 false)。仅对官方支持该能力的模型生效。
    pub disable_thinking: bool,
    pub concurrency: i32,
}

impl ModelConfigDto {
    fn into_new(self) -> NewModelConfig {
        NewModelConfig {
            name: self.name,
            base_url: self.base_url,
            api_key: self.api_key,
            model: self.model,
            max_tokens: self.max_tokens,
            max_context: self.max_context,
            temperature: self.temperature,
            disable_thinking: self.disable_thinking,
            concurrency: self.concurrency,
        }
    }

    fn into_full(self) -> ModelConfig {
        ModelConfig {
            id: self.id,
            name: self.name,
            base_url: self.base_url,
            api_key: self.api_key,
            model: self.model,
            max_tokens: self.max_tokens,
            max_context: self.max_context,
            temperature: self.temperature,
            disable_thinking: self.disable_thinking,
            concurrency: self.concurrency,
            // update 路径保留原 archived(0),不允许通过 upsert 改 archived。
            archived: 0,
        }
    }
}

/// 新建或更新 `ModelConfig`。`payload.id == 0` 走 insert(返回新 id);
/// 否则走 update(返回传入的 id)。`ModelConfigDto` 是手写 snake_case DTO
/// (后端 `#[serde(rename_all = "snake_case")]`),前端调用时内层字段保持 snake_case。
#[tauri::command]
pub fn upsert_model(db: State<'_, Arc<Db>>, payload: ModelConfigDto) -> Result<i64, String> {
    if payload.id == 0 {
        let new = ModelConfigDto::into_new(payload);
        db.model_configs().insert(&new).map_err(|e| e.to_string())
    } else {
        let full = ModelConfigDto::into_full(payload);
        db.model_configs().update(&full).map_err(|e| e.to_string())?;
        Ok(full.id)
    }
}

/// `test_model` 返回结构化报告 —— 让 UI 完整展示 latency / token 计数 / 内容预览。
/// - `content_preview` 是响应前 200 字符(完整原文仍在后端 `provider.chat` 之后可扩展)。
/// - `error` 在失败时携带完整字符串(非 2xx / 空 choices / 缺 usage 都会写)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TestModelReport {
    pub model: String,
    pub base_url: String,
    pub latency_ms: i64,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub content_preview: Option<String>,
    pub error: Option<String>,
}

/// 实际发起一次 `chat` 调用(payload 的模型 + 用户消息 "ping")验证连通性。
/// **不抛错**:失败时把错误字符串塞进 `TestModelReport.error`,UI 可正常展示失败详情。
/// 成功时填 latency / tokens / content_preview。
///
/// **AI 调用日志**:不管成功失败,都通过 `recorder` 记一行 `AiCallEvent`,业务 `TestModel`。
/// recorder 在 hot path 不阻塞(channel 满 → drop new);记不上不影响 test_model 业务结果。
#[tauri::command]
pub async fn test_model(
    payload: ModelConfigDto,
    recorder: State<'_, Arc<dyn AiCallRecorder>>,
    close_thinking: State<'_, Arc<HashSet<String>>>,
) -> Result<TestModelReport, String> {
    let started = Instant::now();
    let report = match OpenAiProvider::new(payload.base_url.clone(), payload.api_key.clone()) {
        Err(e) => TestModelReport {
            model: payload.model,
            base_url: payload.base_url,
            latency_ms: started.elapsed().as_millis() as i64,
            tokens_in: None,
            tokens_out: None,
            content_preview: None,
            error: Some(format!("create provider failed: {e}")),
        },
        Ok(provider) => match provider
            .chat(ChatRequest {
                model: payload.model.clone(),
                messages: vec![ChatMessage { role: Role::User, content: "ping".into() }],
                temperature: payload.temperature,
                max_tokens: payload.max_tokens,
                reasoning_effort: if close_thinking.contains(&payload.model) { Some("none".to_string()) } else { None },
                thinking: if payload.model.starts_with("MiniMax-") {
                    Some("disabled".to_string())
                } else {
                    None
                },
            })
            .await
        {
            Ok(resp) => {
                let preview: String = resp.content.chars().take(200).collect();
                TestModelReport {
                    model: payload.model,
                    base_url: payload.base_url,
                    latency_ms: started.elapsed().as_millis() as i64,
                    tokens_in: Some(resp.tokens_in),
                    tokens_out: Some(resp.tokens_out),
                    content_preview: Some(preview),
                    error: None,
                }
            }
            Err(e) => TestModelReport {
                model: payload.model,
                base_url: payload.base_url,
                latency_ms: started.elapsed().as_millis() as i64,
                tokens_in: None,
                tokens_out: None,
                content_preview: None,
                error: Some(e.to_string()),
            },
        },
    };
    // Recorder 记账 —— 成功 / 失败两条路径分别 record。
    let latency_ms = started.elapsed().as_millis() as i64;
    let system_full = String::new(); // test_model 没 system message
    let user_full = "ping".to_string();
    let estimated_tokens_in = (user_full.chars().count() / 2) as i32;
    let (status, response_full, actual_in, actual_out, error_msg) = match &report {
        r if r.error.is_none() => (
            AiCallStatus::Success,
            r.content_preview.clone().unwrap_or_default(),
            r.tokens_in,
            r.tokens_out,
            None,
        ),
        r => (
            AiCallStatus::Failed,
            String::new(),
            None,
            None,
            r.error.clone(),
        ),
    };
    recorder.record(AiCallEvent {
        business: AiCallBusiness::TestModel,
        context_type: None,
        context_id: None,
        model_config_id: Some(payload.id),
        model_name: report.model.clone(),
        base_url: report.base_url.clone(),
        temperature: payload.temperature,
        max_tokens: payload.max_tokens,
        system_full,
        user_full,
        estimated_tokens_in: Some(estimated_tokens_in),
        actual_tokens_in: actual_in,
        actual_tokens_out: actual_out,
        status,
        response_full,
        latency_ms,
        error: error_msg,
    });

    Ok(report)
}
