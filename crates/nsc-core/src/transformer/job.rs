use std::sync::Arc;
use tokio::sync::Mutex;

use crate::models::{Chapter, ModelConfig, Prompt, PromptKind};

#[derive(Debug, Clone)]
pub struct JobSpec {
    /// tc.id —— worker 用这个 id 跳到 transformation_chapters 表的具体一行，
    /// 以便 mark_running / mark_failed / on_chapter_done 都函数能识别。
    pub tc_id: i64,
    /// transformation_novels.id —— read_context 用这个 id 跻 tn 行。
    pub tn_id: i64,
    pub mode: PromptKind,
    pub chapter: Chapter,
    pub prompt: Prompt,
    pub model_config: ModelConfig,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum JobStatus { Pending, Running, Done, Failed, Cancelled }

#[derive(Debug, Clone, serde::Serialize)]
pub struct JobInfo {
    pub tc_id: i64,
    pub tn_id: i64,
    pub chapter_title: String,
    pub chapter_idx: i32,
    pub status: JobStatus,
    pub error: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct QueueSnapshot {
    pub pending:  Vec<JobInfo>,
    pub running:  Vec<JobInfo>,
    pub done:     Vec<JobInfo>,
    pub failed:   Vec<JobInfo>,
}

#[derive(Default)]
pub(crate) struct SharedQueue {
    pub inner: Mutex<QueueSnapshot>,
}

pub(crate) type Shared = Arc<SharedQueue>;