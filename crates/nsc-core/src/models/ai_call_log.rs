use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 一次 AI 调用的业务归类。
/// - `TransformChapter` = `DefaultTransformer::transform` 路径,转换小说的某章节。
/// - `TestModel` = `commands::models::test_model` 路径,用户点模型"测试连通性"按钮。
///
/// 未来可加 Embedding / Summarize / ... —— enum 扩展点是这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCallBusiness {
    TransformChapter,
    TestModel,
    /// 重新生成单章节预览 —— DefaultTransformer::transform_with_business(req, RegeneratePreview) 路径;与 TransformChapter 共享 prompt 模板上下文,仅业务类型不同。
    RegeneratePreview,
}

/// AI 调用结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCallStatus {
    Success,
    Failed,
}

/// 一次 AI 调用的完整快照 —— 前端看板 / 详情页直接镜像。
/// - `system_preview` / `user_preview` 最多 10KB;完整内容看 `transformation_chapters.result_content` / 调用方上下文。
/// - `estimated_tokens_in` 来自 `chars / 2` 启发式,UI 标注"粗估"。
/// - `actual_tokens_*` 来自 provider `usage`;缺 usage 时为 NULL。
/// - `model_config_id` 可空 —— 历史调用对应的 model_config 可能已被 archive / 删,日志仍要能看到。
/// - `context_type` / `context_id` 软引用,无 FK —— transformation_chapter 删了不影响日志可见。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCallLog {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub business: AiCallBusiness,
    pub context_type: Option<String>,
    pub context_id: Option<i64>,
    pub model_config_id: Option<i64>,
    pub model_name: String,
    pub base_url: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub system_preview: Option<String>,
    pub user_preview: Option<String>,
    pub system_size: i64,
    pub user_size: i64,
    pub estimated_tokens_in: Option<i32>,
    pub actual_tokens_in: Option<i32>,
    pub actual_tokens_out: Option<i32>,
    pub status: AiCallStatus,
    pub response_preview: Option<String>,
    pub response_size: i64,
    pub latency_ms: i64,
    pub error: Option<String>,
}

/// 插入一行 AI 调用日志的入参 —— 命令层 / recorder 拼好后直接调 `AiCallLogRepo::insert`。
/// 字段语义与 `AiCallLog` 一致,但**不含 id / created_at** —— id 自增,created_at 由 repo 在 insert 时填当前 UTC。
#[derive(Debug, Clone)]
pub struct NewAiCallLog {
    pub business: AiCallBusiness,
    pub context_type: Option<String>,
    pub context_id: Option<i64>,
    pub model_config_id: Option<i64>,
    pub model_name: String,
    pub base_url: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub system_preview: Option<String>,
    pub user_preview: Option<String>,
    pub system_size: i64,
    pub user_size: i64,
    pub estimated_tokens_in: Option<i32>,
    pub actual_tokens_in: Option<i32>,
    pub actual_tokens_out: Option<i32>,
    pub status: AiCallStatus,
    pub response_preview: Option<String>,
    pub response_size: i64,
    pub latency_ms: i64,
    pub error: Option<String>,
}

/// 前端 / 命令层做 list 过滤用的查询参数。
/// - `business` 过滤 `'transform_chapter' | 'test_model' | None(=全部)`
/// - `model_config_id` 过滤某 model 的全部调用(`None` 不过滤)
/// - `status` 过滤 `'success' | 'failed' | None(=全部)`
/// - `limit` 上限行数,默认 200,UI 列表翻页时再扩;上限 1000。
/// - `offset` 跳过行数(>=0),用于传统 OFFSET 翻页(UI "第 N 页"导航)。
///   OFFSET 在新写入时会"漂移":插入到顶部会让后续页整体上移一格。
///   对显式页码导航是可接受的 —— 用户点 N 就是取第 N 页,UI 重新渲染。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AiCallLogFilter {
    pub business: Option<AiCallBusiness>,
    pub model_config_id: Option<i64>,
    pub status: Option<AiCallStatus>,
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}
