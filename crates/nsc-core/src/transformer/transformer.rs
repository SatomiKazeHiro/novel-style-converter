use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::ai::{AiProvider, ChatMessage, ChatRequest, Role};
use crate::error::{Error, Result};
use crate::models::{AiCallBusiness, AiCallStatus, Chapter, ModelConfig, Prompt, TransformationNovel};
use crate::prompts::{render, PromptContext};
use crate::recorder::{AiCallEvent, AiCallRecorder};

pub struct TransformRequest {
    pub transformation_id: i64,
    pub chapter: Chapter,
    /// 章节正文切片(由 `queue.rs` 从 `chapters.body` 取出)。
    pub chapter_content: String,
    pub novel_context: TransformationNovelContext,
    pub prompt: Prompt,
    pub model_config: ModelConfig,
    /// 附加指令(可选,非空时拼到 system prompt 文末)。仅 preview 路径用 —— transform 路径任意传 None。
    pub custom_input: Option<String>,
    /// preview id(仅 RegeneratePreview 业务需要,recorder 写 ai_call_logs 时用)。TransformChapter 业务传 None。
    pub preview_id: Option<i64>,
}

pub struct TransformationNovelContext {
    pub transformation_novel: TransformationNovel,
    /// 邻章原文片段 —— 同样由 `queue.rs` 取出,Vec 元素是 `(title, content)` 对。
    pub prev_original: Vec<(String, String)>,
    /// 邻章已转换正文 —— 元素是 (title, content) 对;queue.rs 负责 join
    /// workflow_result_chapters 拿真内容。
    pub prev_transformed: Vec<(String, String)>,
    pub next_original: Vec<(String, String)>,
}

pub struct TransformOutcome {
    pub result_content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}

/// 把 prompt + 上下文渲染成 chat 请求并发给 `AiProvider` 的抽象。
/// `JobQueue` 通过 `Box<dyn Transformer>` 持有实例,所以 `Transformer` 要求 `Send + Sync`。
#[async_trait]
pub trait Transformer: Send + Sync {
    async fn transform(&self, req: TransformRequest) -> Result<TransformOutcome>;
}

/// `Transformer` 的默认实现:渲染 prompt → 调 `AiProvider::chat` → 透传结果。
/// **owns** `Arc<dyn AiProvider>` + `Arc<dyn AiCallRecorder>`,
/// 这样能装进 `Box<dyn Transformer>` 且与 worker 内部 `ProviderCache` 共享 provider 句柄。
///
/// ## Recorder 集成
/// - `transform()` 始终 record 一次,不管成功失败(失败路径也写 error + status=failed)。
/// - `record()` 不阻塞:channel 满 → drop new。
/// - **不**把 recorder 失败往上传 —— transform 业务成功了就算"账没记上",业务结果不受影响。
pub struct DefaultTransformer {
    pub ai: Arc<dyn AiProvider>,
    pub recorder: Arc<dyn AiCallRecorder>,
    /// 已知可关思考的 model_id 集合(由启动期 catalog 解析得到)。
    /// 构造 ChatRequest 时如果 model_config.model 在此集合内 —— 尽力塞
    /// `reasoning_effort:"none"`;否则让模型自决。
    pub close_thinking: Arc<HashSet<String>>,
}

impl DefaultTransformer {
    pub fn new(
        ai: Arc<dyn AiProvider>,
        recorder: Arc<dyn AiCallRecorder>,
        close_thinking: Arc<HashSet<String>>,
    ) -> Self {
        Self { ai, recorder, close_thinking }
    }

    /// ä¸ `transform` ç­ä»·,ä½åè®¸æå®ä¸å¡æ è¯ ââ recorder å `ai_call_logs` æ¶æ `business` åºåä¸ä¸æ / context_type / context_idã
    /// `custom_input` éç©ºæ¶æ¼å° system prompt ææ«(spec Â§3.3)ââ ä¸ºç©ºæ¶ä¸å transform è·¯å¾ byte-equalã
    pub async fn transform_with_business(
        &self,
        req: TransformRequest,
        business: AiCallBusiness,
    ) -> Result<TransformOutcome> {
        let ctx = PromptContext {
            transformation_novel: &req.novel_context.transformation_novel,
            chapter: &req.chapter,
            chapter_content: &req.chapter_content,
            prev_original: &req.novel_context.prev_original,
            prev_transformed: &req.novel_context.prev_transformed,
            next_original: &req.novel_context.next_original,
            kind: req.prompt.kind,
        };
        let rendered = render(&req.prompt.template, &ctx);
        let mut system_full = rendered.system.clone().unwrap_or_default();
        let user_full = rendered.user.clone();
        // close_thinking 模型:catalog 标了可关思考的,在 system prompt 末尾追加
        // "禁止输出思考过程"指令。协议层 `reasoning_effort:"none"` 是首选
        // (OpenAI 风格),但某些 OpenAI 兼容路径(MiniMax /v1 等)不一定认;
        // prompt 层兜底 —— 让模型明确知道不要输出思考块。
        if self.close_thinking.contains(&req.model_config.model) {
            system_full.push_str(
                "\n\n---\n\n直接输出最终转换后的章节正文,不要包含任何思考/推理/分析过程(如 <think>...</think>、<thinking>...</thinking>、以 \"Reasoning:\" / \"分析:\" 等开头的段落)。\n",
            );
        }

        if let Some(extra) = req.custom_input.as_deref() {
            if !extra.trim().is_empty() {
                system_full.push_str("\n\n---\n\n附加指令：\n");
                system_full.push_str(extra);
            }
        }
        let estimated_tokens_in =
            ((system_full.chars().count() + user_full.chars().count()) / 2) as i32;
        // max_context 护栏 —— 估算的输入 tokens 超模型配置上限则直接拒发,
        // 由前端把错误原样展示给用户(见 `Error::Validation` 在 ai_call_logs 透出)。
        // None = 不强制校验,保留历史行为。
        if let Some(max_ctx) = req.model_config.max_context {
            if estimated_tokens_in > max_ctx {
                return Err(Error::Validation(format!(
                    "超出模型 max_context: 估算 {} tokens > 配置上限 {} tokens(model={}, name={})。请调小章节正文 / 上下文邻章数 / system prompt,或换用更大上下文的模型。",
                    estimated_tokens_in, max_ctx,
                    req.model_config.model, req.model_config.name,
                )));
            }
        }
        let mut messages = Vec::with_capacity(if !system_full.is_empty() { 2 } else { 1 });
        if !system_full.is_empty() {
            messages.push(ChatMessage { role: Role::System, content: system_full.clone() });
        }
        messages.push(ChatMessage { role: Role::User, content: user_full.clone() });
        // 尽力关闭思考 = 请求体塞 reasoning_effort:"none"(OpenAI 官方关闭思考的标准方式)。
        // 仅在 catalog 标了 toggle / effort 含 "none" 的模型上做,其他让模型自决。
        // (以前的 model_config.disable_thinking 字段已废弃 —— UI 不再暴露,这里改用 close_thinking 集合。)
        let reasoning_effort = if self.close_thinking.contains(&req.model_config.model) {
            Some("none".to_string())
        } else {
            None
        };
        let chat_req = ChatRequest {
            model: req.model_config.model.clone(),
            messages,
            temperature: req.model_config.temperature,
            max_tokens: req.model_config.max_tokens,
            reasoning_effort,
            thinking: if req.model_config.model.starts_with("MiniMax-") {
                Some("disabled".to_string())
            } else {
                None
            },
        };

        let started = Instant::now();
        let ai_result = self.ai.chat(chat_req).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        // Recorder event æ¼è£ ââ ä¸ç®¡ ai_result æåå¤±è´¥,æ¸å§ç» record ä¸æ¬¡ã
        // æ³¨æ:è¿éä¸è½ `?` æåè¿å(å¦åå¤±è´¥è·¯å¾ä¸ä¼ record)ã
        let (context_type, context_id): (Option<String>, Option<i64>) = match business {
            AiCallBusiness::TransformChapter => {
                (Some("transformation_chapter".into()), Some(req.transformation_id))
            }
            AiCallBusiness::RegeneratePreview => {
                (Some("chapter_preview".into()), req.preview_id)
            }
            AiCallBusiness::TestModel => (None, None),
        };
        let (status, response_full, actual_in, actual_out, error_msg, outcome) = match &ai_result {
            Ok(r) => (
                AiCallStatus::Success,
                r.content.clone(),
                Some(r.tokens_in),
                Some(r.tokens_out),
                None,
                Ok(TransformOutcome {
                    result_content: r.content.clone(),
                    tokens_in: r.tokens_in,
                    tokens_out: r.tokens_out,
                }),
            ),
            Err(e) => (
                AiCallStatus::Failed,
                String::new(),
                None,
                None,
                Some(e.to_string()),
                Err(Error::Ai(e.to_string())),
            ),
        };
        self.recorder.record(AiCallEvent {
            business,
            context_type,
            context_id,
            model_config_id: Some(req.model_config.id),
            model_name: req.model_config.model.clone(),
            base_url: req.model_config.base_url.clone(),
            temperature: req.model_config.temperature,
            max_tokens: req.model_config.max_tokens,
            system_full: system_full.clone(),
            user_full: user_full.clone(),
            estimated_tokens_in: Some(estimated_tokens_in),
            actual_tokens_in: actual_in,
            actual_tokens_out: actual_out,
            status,
            response_full,
            latency_ms,
            error: error_msg,
        });

        outcome
    }

    /// Wrapper:ç­ä»·äº `transform_with_business(req, TransformChapter)` ââ `queue.rs` éè¿ `Box<dyn Transformer>` è°ç¨æ¶ä½¿ç¨ã
    pub async fn transform(&self, req: TransformRequest) -> Result<TransformOutcome> {
        self.transform_with_business(req, AiCallBusiness::TransformChapter).await
    }
}

#[async_trait]
impl Transformer for DefaultTransformer {
    async fn transform(&self, req: TransformRequest) -> Result<TransformOutcome> {
        self.transform_with_business(req, AiCallBusiness::TransformChapter).await
    }
}
