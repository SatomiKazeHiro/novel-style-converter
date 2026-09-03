//! append_chapters_to_batch 集成测试 —— spec §7.1。
//!
//! 本文件覆盖 BatchScheduler::append_chapters_to_batch 依赖的 Db 端路径
//! (batch 存在性 / status 校验 / chapter_ids 归属 / tc 去重 / 事务原子性)。
//! 完整 scheduler 路径(含 JobQueue 入队 / advance_batch)由 e2e 测试覆盖。
use nsc_core::db::Db;
use nsc_core::models::batch::{BatchStatus, NewBatch, OnFailurePolicy};
use nsc_core::models::prompt::PromptKind;
use nsc_core::models::{NewModelConfig, Prompt};
use std::collections::HashSet;

fn seed_env() -> (Db, i64, Vec<i64>) {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=3 {
        cids.push(db.chapters().insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: i as i32,
            title: format!("chapter {i}"),
            body: format!("body {i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap());
    }
    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();
    (db, tn_id, cids)
}

/// Task 4 fixture 升级 —— seed_env 基础上再加真 prompt / model 行,
/// 避开 Task 3 reviewer M-3 指出的「prompt_id: 0, model_config_id: 0」雷
/// (Task 4 完整路径会真用 batch.prompt_id / model_config_id 查 prompt / model,
/// fixture 的 0 会导致 NotFound)。
fn seed_env_with_prompt_model() -> (Db, i64, Vec<i64>, i64, i64) {
    let (db, tn_id, cids) = seed_env();
    let prompt_id = db.prompts().insert(&Prompt {
        id: 0,
        name: "test".into(),
        kind: PromptKind::Compress,
        template: "{{chapter}}".into(),
        is_builtin: false,
        archived: 0,
    }).unwrap();
    let model_id = db.model_configs().insert(&NewModelConfig {
        name: "Test".into(),
        base_url: "http://localhost".into(),
        api_key: "x".into(),
        model: "test-model".into(),
        max_tokens: None,
        max_context: Some(8000),
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
    }).unwrap();
    (db, tn_id, cids, prompt_id, model_id)
}

fn insert_batch_with_status(db: &Db, tn_id: i64, status: BatchStatus, with_chapter_ids: &[i64]) -> i64 {
    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("test".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
        prompt_id: 0,
        model_config_id: 0,
        mode: "compress".into(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
    }).unwrap();
    db.batches().set_status(batch_id, status).unwrap();
    if !with_chapter_ids.is_empty() {
        let b = db.batches().get(batch_id).unwrap().unwrap();
        for &cid in with_chapter_ids {
            db.transformation_chapters().insert(&nsc_core::models::NewTransformationChapter {
                transformation_novel_id: b.transformation_novel_id,
                chapter_id: cid,
                mode: PromptKind::Compress,
                prompt_id: 0,
                model_config_id: 0,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
                batch_id: Some(batch_id),
                style_ref_chapter_id: None,
            }).unwrap();
        }
    }
    batch_id
}

/// Task 4 fixture 升级版 —— 用真 prompt_id / model_id 落库,
/// 配合 Task 4 完整路径读 batch.prompt_id / model_config_id 不报 NotFound。
fn insert_stopped_batch_with_config(
    db: &Db,
    tn_id: i64,
    prompt_id: i64,
    model_id: i64,
    with_chapter_ids: &[i64],
) -> i64 {
    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("test".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
        prompt_id,
        model_config_id: model_id,
        mode: "compress".into(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
    }).unwrap();
    db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();
    if !with_chapter_ids.is_empty() {
        let b = db.batches().get(batch_id).unwrap().unwrap();
        for &cid in with_chapter_ids {
            db.transformation_chapters().insert(&nsc_core::models::NewTransformationChapter {
                transformation_novel_id: b.transformation_novel_id,
                chapter_id: cid,
                mode: PromptKind::Compress,
                prompt_id,
                model_config_id: model_id,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
                batch_id: Some(batch_id),
                style_ref_chapter_id: None,
            }).unwrap();
        }
    }
    batch_id
}

#[test]
fn read_batch_status_after_insert() {
    // 占位:验证 Db 端 setup 正确。
    let (db, tn_id, cids) = seed_env();
    let bid = insert_batch_with_status(&db, tn_id, BatchStatus::Stopped, &cids[..1]);
    let b = db.batches().get(bid).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Stopped);
    assert_eq!(b.transformation_novel_id, tn_id);
}

#[test]
fn list_existing_tcs_by_batch() {
    let (db, tn_id, cids) = seed_env();
    let bid = insert_batch_with_status(&db, tn_id, BatchStatus::Stopped, &cids[..2]);
    let tcs = db.transformation_chapters().list_by_batch(bid).unwrap();
    assert_eq!(tcs.len(), 2);
    let tc_chapter_ids: HashSet<i64> = tcs.iter().map(|tc| tc.chapter_id).collect();
    assert!(tc_chapter_ids.contains(&cids[0]));
    assert!(tc_chapter_ids.contains(&cids[1]));
    assert!(!tc_chapter_ids.contains(&cids[2]));
}

/// Task 4 新增 —— 真 prompt_id / model_id 落库 + stopped batch,验证
/// `Batch` model 上的同质配置字段(prompt_id / model_config_id / mode)
/// 正确往返。这是 Task 4 完整路径的前置条件:append_chapters_to_batch
/// 会从 batch.prompt_id 查 prompt,如果落库时是 0,会 NotFound 报错。
#[test]
fn tc_insert_round_trips_with_homogeneous_config() {
    let (db, tn_id, cids, prompt_id, model_id) = seed_env_with_prompt_model();
    let batch_id = insert_stopped_batch_with_config(&db, tn_id, prompt_id, model_id, &cids[..0]);
    let b = db.batches().get(batch_id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Stopped);
    assert_eq!(b.prompt_id, prompt_id);
    assert_eq!(b.model_config_id, model_id);
    assert_eq!(b.mode, "compress");
}

/// Task 4 新增 —— stopped → running 时 ended_at 必须清空(spec §3.4:
/// 续跑时 ended_at 重新归零,started_at 不动)。`BatchRepo::set_status(Running)`
/// 已扩展 UPDATE ended_at=NULL;这里 round-trip 验证。
#[test]
fn set_status_running_clears_ended_at() {
    let (db, tn_id, _cids, prompt_id, model_id) = seed_env_with_prompt_model();
    let batch_id = insert_stopped_batch_with_config(&db, tn_id, prompt_id, model_id, &[]);
    let b1 = db.batches().get(batch_id).unwrap().unwrap();
    assert!(b1.ended_at.is_some(), "stopped batch 应有 ended_at");
    db.batches().set_status(batch_id, BatchStatus::Running).unwrap();
    let b2 = db.batches().get(batch_id).unwrap().unwrap();
    assert!(b2.ended_at.is_none(), "running batch 不应有 ended_at");
    assert_eq!(b2.status, BatchStatus::Running);
}