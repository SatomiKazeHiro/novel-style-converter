use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::error::Error;
use nsc_core::models::{Batch, BatchStatus, OnFailurePolicy, PromptKind, TransformStatus};
use nsc_core::transformer::{BatchScheduler, WorkflowCreate};

const CONTENT_PREVIEW_CHARS: usize = 80;

/// IPC 边界的首章种子 DTO(spec 2026-09-01)—— 内嵌 source 字段区分来源。
/// 后端 nsc_core::models::FirstChapterSeed + SeedSource。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FirstChapterSeedDto {
    pub content: String,
    /// snake_case: `kind: "llm"` / `kind: "manual"`
    pub source: SeedSourceDto,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeedSourceDto {
    Llm {
        tokens_in: i32,
        tokens_out: i32,
    },
    Manual,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkflowPayload {
    pub tn_id: i64,
    pub label: Option<String>,
    pub chapter_ids: Vec<i64>,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub on_failure_policy: String,
    /// 试运行首章种子(spec §3.2)。None → 全部 tc pending。
    pub preview_first_chapter: Option<FirstChapterSeedDto>,
}

impl CreateWorkflowPayload {
    fn into_core(self) -> Result<WorkflowCreate, Error> {
        let mode = match self.mode.as_str() {
            "compress" => PromptKind::Compress,
            "style" => PromptKind::Style,
            other => return Err(Error::Validation(format!("未知 mode: {other}"))),
        };
        let on_failure_policy = match self.on_failure_policy.as_str() {
            "pause_and_review" => OnFailurePolicy::PauseAndReview,
            "skip_failed" => OnFailurePolicy::SkipFailed,
            // 前端已移除 'terminate'(0.2),保留解析仅为兼容历史 IPC 请求。
            "terminate" => OnFailurePolicy::PauseAndReview,
            other => return Err(Error::Validation(format!("未知 on_failure_policy: {other}"))),
        };
        Ok(WorkflowCreate {
            transformation_novel_id: self.tn_id,
            label: self.label,
            chapter_ids: self.chapter_ids,
            prompt_id: self.prompt_id,
            model_config_id: self.model_config_id,
            mode,
            ctx_prev_original: self.ctx_prev_original,
            ctx_prev_transformed: self.ctx_prev_transformed,
            ctx_next_original: self.ctx_next_original,
            // 前端 CreateWorkflowInput 暂无 ctx_next_transformed 字段 —— 默认 0(无后文),
            // 与 transformation_chapters 列在 schema 0028 后保持一致(详见 batch.rs 同质配置迁移)。
            ctx_next_transformed: 0,
            on_failure_policy,
            first_chapter_seed: self.preview_first_chapter.map(|p| {
                let source = match p.source {
                    SeedSourceDto::Llm { tokens_in, tokens_out } =>
                        nsc_core::models::transformation::SeedSource::Llm { tokens_in, tokens_out },
                    SeedSourceDto::Manual =>
                        nsc_core::models::transformation::SeedSource::Manual,
                };
                nsc_core::models::transformation::FirstChapterSeed {
                    content: p.content,
                    source,
                }
            }),
        })
    }
}

/// 单章节预览提交入参(spec §4.2) —— 必传 batch_id / chapter_id / draft_content;可选 source_preview_id 用于透传 tokens。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommitPreviewInput {
    pub batch_id: i64,
    pub chapter_id: i64,
    pub draft_content: String,
    pub source_preview_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowSummary {
    pub id: i64,
    pub tn_id: i64,
    pub label: Option<String>,
    pub status: BatchStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub done_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
    pub total_count: i64,
    pub promoted_count: i64,
    // append_chapters spec §3.2 / Task 8:
    // - `prompt_name` / `model_display_name` 来自 batch 同质配置字段的 JOIN,
    //   缺失时(archived=1 或已物理删除的 prompt/model)fallback 为空串;
    //   UI 的 AppendChaptersDialog 上下文展示位只用显示语义,不要求强非空。
    // - `mode` 是 batch 字段("compress" / "style"),直接 clone,不再做归一化。
    // - 三个 ctx_* 是 batch 字段,stopped batch append 时 dialog 拿来确认"沿用旧配置"。
    pub prompt_name: String,
    pub model_display_name: String,
    pub mode: String,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}

#[derive(Debug, Serialize)]
pub struct WorkflowChapterRow {
    pub tc_id: i64,
    pub chapter_id: i64,
    pub chapter_idx: i32,
    pub chapter_title: String,
    pub status: TransformStatus,
    pub error: Option<String>,
    pub content_preview: Option<String>,
    pub is_empty_slot: bool,
}

#[derive(Debug, Serialize)]
pub struct SourceChapterRow {
    pub chapter_id: i64,
    pub idx: i32,
    pub title: String,
    pub word_count: i32,
    pub non_empty_result_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ChapterWorkflowResultRow {
    pub batch_id: i64,
    pub batch_label: Option<String>,
    pub batch_status: BatchStatus,
    pub batch_ended_at: Option<String>,
    pub content: Option<String>,
    pub status: TransformStatus,
}

fn parse_batch_status(s: &str) -> Result<BatchStatus, String> {
    match s {
        "pending" => Ok(BatchStatus::Pending),
        "running" => Ok(BatchStatus::Running),
        "stopped" => Ok(BatchStatus::Stopped),
        "paused" => Ok(BatchStatus::Paused),
        "completed" => Ok(BatchStatus::Completed),
        "terminated" => Ok(BatchStatus::Terminated),
        "cancelled" => Ok(BatchStatus::Cancelled),
        other => Err(format!("unknown batch status: {other}")),
    }
}

fn parse_transform_status(s: &str) -> Result<TransformStatus, String> {
    match s {
        "pending" => Ok(TransformStatus::Pending),
        "running" => Ok(TransformStatus::Running),
        "done" => Ok(TransformStatus::Done),
        "failed" => Ok(TransformStatus::Failed),
        "skipped" => Ok(TransformStatus::Skipped),
        "cancelled" => Ok(TransformStatus::Cancelled),
        other => Err(format!("unknown transform status: {other}")),
    }
}

fn to_summary(db: &Db, b: &Batch) -> WorkflowSummary {
    // 单 SQL 一次拿完 5 个聚合 + 2 个 JOIN 的名称:tc status 计数、promoted da 计数、
    // prompt / model name(spec §3.2 同质配置 join)。
    // list_workflows 调 to_summary N 次,保持 N=1 次 db.lock(),不引入额外锁往返;
    // prompt_name / model_display_name 用 SELECT-list 子查询(常量输入),SQLite 按主键
    // lookup 单行,O(1) 开销可忽略。
    //
    // prompt / model 名字用 COALESCE 兜底:archived=1 的行仍然存在可 join,只有物理
    // 删除才会 NULL;UI 上下文展示位不要求强非空,空串显示更直白。
    let lk = db.lock();
    let (done, failed, skipped, total, promoted_count, prompt_name, model_display_name): (
        i64, i64, i64, i64, i64, String, String,
    ) = lk.query_row(
        "SELECT \
            COALESCE(SUM(CASE WHEN tc.status='done' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN tc.status='failed' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN tc.status='skipped' THEN 1 ELSE 0 END), 0), \
            COUNT(tc.id), \
            COALESCE((SELECT COUNT(*) FROM data_assets WHERE source_workflow_id = ?1), 0), \
            COALESCE((SELECT name FROM prompts WHERE id = ?2), ''), \
            COALESCE((SELECT name FROM model_configs WHERE id = ?3), '') \
         FROM transformation_chapters tc WHERE tc.batch_id = ?1",
        rusqlite::params![b.id, b.prompt_id, b.model_config_id],
        |row| Ok((
            row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
            row.get(5)?, row.get(6)?,
        )),
    ).unwrap_or((0, 0, 0, 0, 0, String::new(), String::new()));
    drop(lk);
    WorkflowSummary {
        id: b.id,
        tn_id: b.transformation_novel_id,
        label: b.label.clone(),
        status: b.status,
        created_at: b.created_at.to_rfc3339(),
        started_at: b.started_at.map(|t| t.to_rfc3339()),
        ended_at: b.ended_at.map(|t| t.to_rfc3339()),
        done_count: done,
        failed_count: failed,
        skipped_count: skipped,
        total_count: total,
        promoted_count,
        prompt_name,
        model_display_name,
        mode: b.mode.clone(),
        ctx_prev_original: b.ctx_prev_original,
        ctx_prev_transformed: b.ctx_prev_transformed,
        ctx_next_original: b.ctx_next_original,
    }
}

#[tauri::command]
pub async fn create_workflow(
    db: State<'_, Arc<Db>>,
    payload: CreateWorkflowPayload,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let spec = payload.into_core().map_err(|e| e.to_string())?;
    let res = tokio::task::spawn_blocking(move || sched.create_workflow(spec))
        .await
        .map_err(|e| format!("create_workflow join: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub fn list_workflows(
    db: State<'_, Arc<Db>>,
    tn_id: i64,
) -> Result<Vec<WorkflowSummary>, String> {
    let batches = db.batches().list_by_tn(tn_id).map_err(|e| e.to_string())?;
    Ok(batches.iter().map(|b| to_summary(&db, b)).collect())
}

#[tauri::command]
pub fn get_workflow(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
) -> Result<WorkflowSummary, String> {
    let b = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;
    Ok(to_summary(&db, &b))
}

#[tauri::command]
pub fn list_workflow_chapters(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
) -> Result<Vec<WorkflowChapterRow>, String> {
    let _batch = db.batches().get(batch_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch {batch_id} 不存在"))?;

    let _dbg = db.lock();
    let mut stmt = _dbg.prepare(
        "SELECT tc.id, tc.chapter_id, c.idx, c.title, tc.status, tc.error, wrc.content \
         FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         LEFT JOIN workflow_result_chapters wrc \
            ON wrc.chapter_id = tc.chapter_id \
            AND wrc.workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id = ?1) \
         WHERE tc.batch_id = ?1 \
         ORDER BY c.idx ASC, tc.id ASC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![batch_id], |row| {
        let status_s: String = row.get(4)?;
        let status = parse_transform_status(&status_s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, e.into())
        })?;
        let content: Option<String> = row.get(6)?;
        let preview = content.as_deref().map(preview_first_chars);
        let is_empty = content.is_none();
        Ok(WorkflowChapterRow {
            tc_id: row.get(0)?,
            chapter_id: row.get(1)?,
            chapter_idx: row.get(2)?,
            chapter_title: row.get(3)?,
            status,
            error: row.get(5)?,
            content_preview: preview,
            is_empty_slot: is_empty,
        })
    }).map_err(|e| e.to_string())?;
    let collected: Vec<WorkflowChapterRow> = rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(collected)
}

fn preview_first_chars(s: &str) -> String {
    s.chars().take(CONTENT_PREVIEW_CHARS).collect()
}

#[tauri::command]
pub async fn stop_workflow(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let res = tokio::task::spawn_blocking(move || sched.stop_workflow(batch_id))
        .await
        .map_err(|e| format!("stop_workflow join: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub async fn retry_workflow_chapters(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
    chapter_ids: Vec<i64>,
    scheduler: State<'_, Arc<BatchScheduler>>,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let res = tokio::task::spawn_blocking(move || sched.retry_empty_slots(batch_id, &chapter_ids))
        .await
        .map_err(|e| format!("retry_workflow_chapters join: {e}"))?
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub fn list_transformation_source_chapters(
    db: State<'_, Arc<Db>>,
    tn_id: i64,
) -> Result<Vec<SourceChapterRow>, String> {
    let tn = db.transformation_novels().get(tn_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("tn {tn_id} 不存在"))?;
    let _dbg = db.lock();
    let mut stmt = _dbg.prepare(
        "SELECT c.id, c.idx, c.title, c.word_count, \
                COALESCE((SELECT COUNT(*) FROM workflow_result_chapters wrc \
                    JOIN workflow_results wr ON wr.id = wrc.workflow_result_id \
                    JOIN batches b ON b.id = wr.batch_id \
                    WHERE b.transformation_novel_id = ?1 \
                      AND wrc.chapter_id = c.id \
                      AND wrc.content IS NOT NULL), 0) \
         FROM chapters c \
         WHERE c.data_asset_id = ?2 \
         ORDER BY c.idx ASC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(
        rusqlite::params![tn_id, tn.data_asset_id],
        |row| Ok(SourceChapterRow {
            chapter_id: row.get(0)?,
            idx: row.get(1)?,
            title: row.get(2)?,
            word_count: row.get(3)?,
            non_empty_result_count: row.get(4)?,
        }),
    ).map_err(|e| e.to_string())?;
    let collected: Vec<SourceChapterRow> = rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(collected)
}

#[tauri::command]
pub fn list_chapter_workflow_results(
    db: State<'_, Arc<Db>>,
    tn_id: i64,
    chapter_id: i64,
) -> Result<Vec<ChapterWorkflowResultRow>, String> {
    let _dbg = db.lock();
    let mut stmt = _dbg.prepare(
        "SELECT b.id, b.label, b.status, b.ended_at, wrc.content, tc.status \
         FROM batches b \
         JOIN workflow_results wr ON wr.batch_id = b.id \
         JOIN workflow_result_chapters wrc ON wrc.workflow_result_id = wr.id \
         LEFT JOIN transformation_chapters tc \
            ON tc.batch_id = b.id AND tc.chapter_id = wrc.chapter_id \
         WHERE b.transformation_novel_id = ?1 AND wrc.chapter_id = ?2 \
         ORDER BY b.id DESC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(
        rusqlite::params![tn_id, chapter_id],
        |row| {
            let batch_status_s: String = row.get(2)?;
            let batch_status = parse_batch_status(&batch_status_s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
            })?;
            let tc_status_s: Option<String> = row.get(5)?;
            let tc_status = match tc_status_s.as_deref() {
                None => TransformStatus::Pending,
                Some(s) => parse_transform_status(s).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, e.into())
                })?,
            };
            Ok(ChapterWorkflowResultRow {
                batch_id: row.get(0)?,
                batch_label: row.get(1)?,
                batch_status,
                batch_ended_at: row.get(3)?,
                content: row.get(4)?,
                status: tc_status,
            })
        },
    ).map_err(|e| e.to_string())?;
    let collected: Vec<ChapterWorkflowResultRow> = rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(collected)
}

#[tauri::command]
pub async fn regenerate_chapter_preview(
    scheduler: State<'_, Arc<BatchScheduler>>,
    batch_id: i64,
    chapter_id: i64,
    custom_input: Option<String>,
) -> Result<i64, String> {
    let sched = scheduler.inner().clone();
    let res = tokio::task::spawn_blocking(move || {
        sched.regenerate_preview(batch_id, chapter_id, custom_input)
    })
    .await
    .map_err(|e| format!("regenerate_chapter_preview join: {e}"))?
    .map_err(|e| e.to_string())?;
    Ok(res)
}

#[tauri::command]
pub fn commit_chapter_preview(
    db: State<'_, Arc<Db>>,
    scheduler: State<'_, Arc<BatchScheduler>>,
    input: CommitPreviewInput,
) -> Result<WorkflowSummary, String> {
    let sched = scheduler.inner().clone();
    let res = sched.commit_preview(
        input.batch_id,
        input.chapter_id,
        input.draft_content,
        input.source_preview_id,
    ).map_err(|e| e.to_string())?;
    Ok(to_summary(&db, &res))
}

#[tauri::command]
pub fn list_chapter_previews(
    scheduler: State<'_, Arc<BatchScheduler>>,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Vec<nsc_core::models::ChapterPreviewRow>, String> {
    let sched = scheduler.inner().clone();
    sched.list_chapter_previews(batch_id, chapter_id)
        .map_err(|e| e.to_string())
}

/// 工作流删除结果 —— 报告被影响的对象,给 UI 提示用户"哪些数据资产来源工作流没了"。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeleteWorkflowResult {
    pub deleted_batch_id: i64,
    /// 删除时已派生自此工作流的 promoted data_asset 数量(删除前快照)。
    /// UI 提示用:"已有 N 份数据资产从此工作流派生,删除后它们的来源工作流将被抹掉"。
    pub promoted_data_asset_count: i64,
}

#[tauri::command]
pub fn delete_workflow(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
) -> Result<DeleteWorkflowResult, String> {
    // 先在删除前快照:已派生 da 数量(data_assets.source_workflow_id 删后被 SET NULL)。
    let promoted = db.promotion().count_by_workflow(batch_id)
        .map_err(|e| e.to_string())?;
    db.batches().delete(batch_id).map_err(|e| e.to_string())?;
    Ok(DeleteWorkflowResult {
        deleted_batch_id: batch_id,
        promoted_data_asset_count: promoted,
    })
}

#[tauri::command]
pub fn discard_chapter_preview(
    scheduler: State<'_, Arc<BatchScheduler>>,
    preview_id: i64,
) -> Result<(), String> {
    let sched = scheduler.inner().clone();
    sched.discard_preview(preview_id).map_err(|e| e.to_string())
}

/// 「新建工作流」试运行区 IPC 入参(spec §5.1)。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewFirstChapterInput {
    pub tn_id: i64,
    pub chapter_id: i64,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub include_prev: bool,
    pub include_next: bool,
    pub custom_input: Option<String>,
}

/// 试运行结果(IPC 出参)。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewFirstChapterOutput {
    pub content: String,
    pub tokens_in: i32,
    pub tokens_out: i32,
}

/// 调一次 AI 跑 idx 最小那个章节,返回 preview 结果(spec §3.4 / §5.1)。
/// 不写 batch / tc / wrc 行;仅写一条 ai_call_logs(business=RegeneratePreview,
/// context_type=transformation_chapter + context_id=tn_id)。
/// 用户满意后通过 `create_workflow` 的 `preview_first_chapter` 入参传入此结果。
#[tauri::command]
pub async fn preview_first_chapter(
    scheduler: State<'_, Arc<BatchScheduler>>,
    input: PreviewFirstChapterInput,
) -> Result<PreviewFirstChapterOutput, String> {
    let sched = scheduler.inner().clone();
    let core_input = nsc_core::models::transformation::PreviewFirstChapterInput {
        tn_id: input.tn_id,
        chapter_id: input.chapter_id,
        prompt_id: input.prompt_id,
        model_config_id: input.model_config_id,
        include_prev: input.include_prev,
        include_next: input.include_next,
        custom_input: input.custom_input,
    };
    let outcome = sched.preview_first_chapter(core_input).await
        .map_err(|e| e.to_string())?;
    Ok(PreviewFirstChapterOutput {
        content: outcome.content,
        tokens_in: outcome.tokens_in,
        tokens_out: outcome.tokens_out,
    })
}
