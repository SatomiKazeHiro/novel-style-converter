use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::PromptKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformStatus {
    Pending,
    Running,
    Done,
    Failed,
    /// `Skipped`：on_failure_policy=skip_failed 时失败章保留 error 但跳过；或
    /// 用户在 paused 时显式跳过（resume action=Skip）。
    /// `result_content` 通常为 NULL；`error` 字段保留失败原因。
    Skipped,
    Cancelled,
}

/// 一次转换的结果。挂在 (transformation_novel_id, chapter_id) 上,
/// 每次重跑生成新 row,历史全留(无 UNIQUE 约束)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationChapter {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: PromptKind,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub status: TransformStatus,
    pub result_content: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    /// 所属批号;存量散点行为 NULL。
    pub batch_id: Option<i64>,
    /// frontier 章节 id —— 同 tn 内、idx 严格小于本章节、status='done' 的最近一次 tc 行。
    /// 命名沿用 spec(spec §4.1 / §5.8);本片先用 NULL(scheduler 接力时填,Slice 4)。
    pub style_ref_chapter_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewTransformationChapter {
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub mode: PromptKind,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub batch_id: Option<i64>,
    pub style_ref_chapter_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Generating,
    Done,
    Failed,
}

fn parse_status(s: &str) -> Option<PreviewStatus> {
    match s {
        "generating" => Some(PreviewStatus::Generating),
        "done" => Some(PreviewStatus::Done),
        "failed" => Some(PreviewStatus::Failed),
        _ => None,
    }
}

impl FromStr for PreviewStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_status(s).ok_or_else(|| format!("unknown preview status: {}", s))
    }
}

/// 一次单章节预览尝试 —— 用户可以为一个 (batch_id, chapter_id) 多次生成,对比 / 编辑 / 提交。
/// 提交(transformation_chapters.content 被覆写)后由 repo.delete_by_chapter 清空全部预览行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPreviewRow {
    pub id: i64,
    pub batch_id: i64,
    pub chapter_id: i64,
    pub custom_input: Option<String>,
    pub preview_content: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub error: Option<String>,
    pub status: PreviewStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 「新建工作流」时，用户可选择为首章预置的内容（"种子"）。
/// 可不传（None），此时首章由 LLM 在 batch 内正常处理。
/// 重命名自 PreviewFirstChapter（spec 2026-09-01）；同步加 SeedSource 区分来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstChapterSeed {
    pub content: String,
    pub source: SeedSource,
}

/// 首章种子的来源 —— 区分 LLM 出 vs 手写,便于后端正确写 tokens 字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedSource {
    /// 用户调 previewFirstChapter + 从预览复制 → seed 来自 LLM。
    Llm { tokens_in: i32, tokens_out: i32 },
    /// 用户在 dialog 内手写 → 没有 LLM 调用,tokens 为 0。
    Manual,
}

/// 「新建工作流」试运行区预览首章的入参(spec §3.4 / §5.1)。
/// 后端按 `include_prev` / `include_next` 计算实际的前后文片段;`custom_input`
/// 是「附加指令」(本期 UI 不暴露,留 TODO 接入)。
/// `tn_id` 用于定位 data_asset 范围(章节在同一 da 下,不能跨 da 拿前文)。
#[derive(Debug, Clone)]
pub struct PreviewFirstChapterInput {
    pub tn_id: i64,
    pub chapter_id: i64,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub include_prev: bool,
    pub include_next: bool,
    pub custom_input: Option<String>,
}

/// 试运行结果(IPC 边界 DTO 由 commands 层另起,这里只承载后端内部结果)。
#[derive(Debug, Clone)]
pub struct PreviewFirstChapterOutcome {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}
