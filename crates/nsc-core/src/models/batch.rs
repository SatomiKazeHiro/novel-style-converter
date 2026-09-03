use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Pending,
    Running,
    /// 已停(spec §3.3) — 启动失败 / 用户手动停止 / 启动安全恢复统一收口。
    /// 生命周期不再回 Running;只有 retry_empty_slots 才能往这批里加新 tc 行。
    Stopped,
    Paused,
    Completed,
    Terminated,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailurePolicy {
    /// 章节失败 → batch 转 Paused 等用户决策(retry / skip / terminate 任选)。
    PauseAndReview,
    /// 章节失败 → 该章标 Skipped,继续 dispatch 下一章(batch 留 Running)。
    SkipFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
    pub status: BatchStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    // 新增(append_chapters spec §3.2):
    pub prompt_id: i64,
    pub model_config_id: i64,
    /// PromptKind 的字符串形式("compress" / "style")。
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub ctx_next_transformed: i32,
}

#[derive(Debug, Clone)]
pub struct NewBatch {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub on_failure_policy: OnFailurePolicy,
    // 新增:
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub ctx_next_transformed: i32,
}

