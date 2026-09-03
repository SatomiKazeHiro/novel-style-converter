use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::{NewTransformationChapter, PromptKind, TransformStatus};
use nsc_core::transformer::{BatchScheduler, JobQueue, JobSpec, QueueSnapshot};

#[derive(Debug, Serialize)]
pub struct TransformationChapterRow {
    pub id: i64,
    pub transformation_novel_id: i64,
    pub chapter_id: i64,
    pub chapter_idx: i32,
    pub chapter_title: String,
    pub mode: PromptKind,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub status: TransformStatus,
    pub result_content: Option<String>,
    pub tokens_in: Option<i32>,
    pub tokens_out: Option<i32>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub batch_id: Option<i64>,
    pub style_ref_chapter_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct EnqueuePayload {
    pub transformation_novel_id: i64,
    pub chapter_ids: Vec<i64>,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}

#[derive(Debug, Deserialize)]
pub struct EnqueueAllPayload {
    pub transformation_novel_id: i64,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
}

/// 通过 data_asset 反查 chapters 表(Chapter 不再带 upload_id 字段)。
fn chapter_lookup(db: &Db, data_asset_id: i64) -> std::collections::HashMap<i64, (i32, String)> {
    db.chapters()
        .list_by_data_asset(data_asset_id)
        .map(|v| v.into_iter().map(|c| (c.id, (c.idx, c.title))).collect())
        .unwrap_or_default()
}

pub(crate) fn join_chapter_info(
    db: &Db,
    data_asset_id: i64,
    rows: Vec<nsc_core::models::TransformationChapter>,
) -> Vec<TransformationChapterRow> {
    let lookup = chapter_lookup(db, data_asset_id);
    rows.into_iter()
        .map(|r| {
            let (idx, title) = lookup.get(&r.chapter_id).cloned().unwrap_or((0, String::new()));
            TransformationChapterRow {
                id: r.id,
                transformation_novel_id: r.transformation_novel_id,
                chapter_id: r.chapter_id,
                chapter_idx: idx,
                chapter_title: title,
                mode: r.mode,
                prompt_id: r.prompt_id,
                model_config_id: r.model_config_id,
                status: r.status,
                result_content: r.result_content,
                tokens_in: r.tokens_in,
                tokens_out: r.tokens_out,
                error: r.error,
                started_at: r.started_at.map(|t| t.to_rfc3339()),
                completed_at: r.completed_at.map(|t| t.to_rfc3339()),
                batch_id: r.batch_id,
                style_ref_chapter_id: r.style_ref_chapter_id,
            }
        })
        .collect()
}

#[tauri::command]
pub fn list_transformation_chapters(
    db: State<'_, Arc<Db>>,
    transformation_novel_id: i64,
) -> Result<Vec<TransformationChapterRow>, String> {
    let tn = db
        .transformation_novels()
        .get(transformation_novel_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("transformation_novel {transformation_novel_id} 不存在"))?;
    let rows = db
        .transformation_chapters()
        .list_by_transformation_novel(transformation_novel_id)
        .map_err(|e| e.to_string())?;
    Ok(join_chapter_info(&db, tn.data_asset_id, rows))
}

#[tauri::command]
pub fn list_transformation_chapters_for_chapter(
    db: State<'_, Arc<Db>>,
    chapter_id: i64,
) -> Result<Vec<TransformationChapterRow>, String> {
    let ch = db
        .chapters()
        .get(chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("chapter {chapter_id} 不存在"))?;
    let rows = db
        .transformation_chapters()
        .list_by_chapter(chapter_id)
        .map_err(|e| e.to_string())?;
    Ok(join_chapter_info(&db, ch.data_asset_id, rows))
}

/// 对 `chapter_ids` 逐章插入 `transformation_chapters(pending)` 并立即
/// `JobQueue.enqueue(JobSpec)`。每个 chapter 校验:
/// 1. `chapter.data_asset_id == transformation_novel.data_asset_id`(共享坐标系)
/// 2. `chapter.id` 存在
/// `chapter_ids` 为空时直接 `Ok(vec![])` 跳过。
/// 返回所有新 `transformation_chapter.id` 的顺序列表。
#[tauri::command]
pub fn enqueue_transformation_chapters(
    db: State<'_, Arc<Db>>,
    queue: State<'_, Arc<JobQueue>>,
    payload: EnqueuePayload,
) -> Result<Vec<i64>, String> {
    if payload.chapter_ids.is_empty() {
        return Ok(vec![]);
    }
    let (tn, prompt, model_cfg) = {

        let tn = db
            .transformation_novels()
            .get(payload.transformation_novel_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!("transformation_novel {} 不存在", payload.transformation_novel_id)
            })?;
        let prompt = db
            .prompts()
            .get(payload.prompt_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("prompt {} 不存在", payload.prompt_id))?;
        let cfg = db
            .model_configs()
            .get(payload.model_config_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("model_config {} 不存在", payload.model_config_id))?;
        (tn, prompt, cfg)
    };

    let mut ids = Vec::with_capacity(payload.chapter_ids.len());
    let mut jobs: Vec<JobSpec> = Vec::with_capacity(payload.chapter_ids.len());
    {

        for chapter_id in &payload.chapter_ids {
            let chapter = db
                .chapters()
                .get(*chapter_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("chapter {chapter_id} 不存在"))?;
            // 校验:同一 transformation_novel 共享同一 data_asset。
            if chapter.data_asset_id != tn.data_asset_id {
                return Err(format!(
                    "chapter {chapter_id} 不属于 transformation_novel 的 data_asset {}",
                    tn.data_asset_id
                ));
            }
            let mode = prompt.kind;
            let id = db
                .transformation_chapters()
                .insert(&NewTransformationChapter {
                    transformation_novel_id: tn.id,
                    chapter_id: chapter.id,
                    mode,
                    prompt_id: prompt.id,
                    model_config_id: model_cfg.id,
                    ctx_prev_original: payload.ctx_prev_original,
                    ctx_prev_transformed: payload.ctx_prev_transformed,
                    ctx_next_original: payload.ctx_next_original,
                    batch_id: None,            // 既有 enqueue 路径不接 batch
                    style_ref_chapter_id: None,
                })
                .map_err(|e| e.to_string())?;
            ids.push(id);
            let tc_for_tn = db.transformation_chapters().get(id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("tc {id} missing"))?;
            jobs.push(JobSpec {
                tc_id: id,
                tn_id: tc_for_tn.transformation_novel_id,
                mode,
                chapter,
                prompt: prompt.clone(),
                model_config: model_cfg.clone(),
                ctx_prev_original: payload.ctx_prev_original,
                ctx_prev_transformed: payload.ctx_prev_transformed,
                ctx_next_original: payload.ctx_next_original,
            });
        }
    }
    for job in jobs {
        queue.enqueue(job);
    }
    Ok(ids)
}

/// `transformation_novel` 下所有章节入队(从 `chapters` 表拉该 `data_asset_id`
/// 的全量 chapter_id,然后走 `enqueue_transformation_chapters` 的相同校验 +
/// 落库 + 入队流程)。返回新 `transformation_chapter.id` 列表(按 idx 顺序)。
#[tauri::command]
pub fn enqueue_all_chapters(
    db: State<'_, Arc<Db>>,
    queue: State<'_, Arc<JobQueue>>,
    payload: EnqueueAllPayload,
) -> Result<Vec<i64>, String> {
    let chapter_ids: Vec<i64> = {

        let tn = db
            .transformation_novels()
            .get(payload.transformation_novel_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!("transformation_novel {} 不存在", payload.transformation_novel_id)
            })?;
        db.chapters()
            .list_by_data_asset(tn.data_asset_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|c| c.id)
            .collect()
    };
    enqueue_transformation_chapters(
        db,
        queue,
        EnqueuePayload {
            transformation_novel_id: payload.transformation_novel_id,
            chapter_ids,
            prompt_id: payload.prompt_id,
            model_config_id: payload.model_config_id,
            ctx_prev_original: payload.ctx_prev_original,
            ctx_prev_transformed: payload.ctx_prev_transformed,
            ctx_next_original: payload.ctx_next_original,
        },
    )
}

/// 拉当前 `JobQueue` 快照(pending / running / done / failed 四组)。
/// 内部锁争用时返回空 snapshot,不阻塞 caller —— 前端 UI 1s 轮询用。
#[tauri::command]
pub fn get_queue_snapshot(queue: State<'_, Arc<JobQueue>>) -> Result<QueueSnapshot, String> {
    Ok(queue.snapshot())
}

#[derive(Debug, Serialize)]
pub struct AppendChaptersResult {
    pub batch_id: i64,
    pub added_tc_ids: Vec<i64>,
}

/// 把 chapter_ids 追加到 stopped batch(spec §3.4 / Task 3-4)。
/// 薄层委派给 BatchScheduler::append_chapters_to_batch —— 不在此层
/// 手撸事务、校验或入队,所有逻辑都跟 create_workflow 路径共用。
#[tauri::command]
pub fn append_chapters_to_batch(
    scheduler: State<'_, Arc<BatchScheduler>>,
    batch_id: i64,
    chapter_ids: Vec<i64>,
) -> Result<AppendChaptersResult, String> {
    let tc_ids = scheduler
        .append_chapters_to_batch(batch_id, chapter_ids)
        .map_err(|e| e.to_string())?;
    Ok(AppendChaptersResult {
        batch_id,
        added_tc_ids: tc_ids,
    })
}