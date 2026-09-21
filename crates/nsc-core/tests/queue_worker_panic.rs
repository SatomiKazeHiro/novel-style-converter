//! worker 的 per-job panic 边界回归 —— queue.rs。
//!
//! 背景:`JobQueue` 的 worker 循环此前没有任何 panic 隔离。一次 job 内 panic 会
//! unwind 出 `rt.block_on` → worker 线程直接死掉;而且 `Db` 的 Mutex 若在 panic 时
//! 被持有就中毒,之后**每个**线程的每一次 `lock()` 都 panic —— 单次 panic 升级成
//! 整个应用不可用。
//!
//! 本测试锁定两件事(单 worker,保证顺序确定):
//! 1. job A 在 AI 调用内 panic 后,worker **仍然活着**并继续消费 job B;
//! 2. job B 正常完成(done),证明 panic 没有污染共享的 Db / provider cache。
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::db::Db;
use nsc_core::error::Result;
use nsc_core::models::batch::{BatchStatus, NewBatch, OnFailurePolicy};
use nsc_core::models::prompt::PromptKind;
use nsc_core::models::{ModelConfig, NewModelConfig, NewTransformationChapter, Prompt};
use nsc_core::recorder::NoopRecorder;
use nsc_core::transformer::{JobQueue, JobSpec, JobStatus};

/// 第一次 chat 调用 panic,之后正常返回 —— 模拟"某个章节因内部 bug panic"。
struct PanicOnceProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AiProvider for PanicOnceProvider {
    async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            panic!("模拟 job 内部 panic");
        }
        Ok(ChatResponse {
            content: "转换后的正文".into(),
            tokens_in: 11,
            tokens_out: 22,
        })
    }
}

fn make_prompt(id: i64) -> Prompt {
    Prompt {
        id,
        name: "test".into(),
        kind: PromptKind::Compress,
        template: "{{chapter_content}}".into(),
        is_builtin: false,
        archived: 0,
    }
}

/// 建 upload + da + n 章,返回 (db, da_id, cids)。
fn seed(db: &Arc<Db>, n: i32) -> (i64, Vec<i64>) {
    let upload_id = db
        .uploads()
        .insert(&nsc_core::models::NewUpload {
            sha256: format!("sha-{n}"),
            filename: "t.txt".into(),
            byte_size: 0,
            file_path: String::new(),
            original_text: String::new(),
            word_count: 0,
        })
        .unwrap();
    let da_id = db
        .data_assets()
        .insert(&nsc_core::models::NewDataAsset {
            upload_id,
            title: "DA".into(),
            source_filename: "t.txt".into(),
            ..Default::default()
        })
        .unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        cids.push(
            db.chapters()
                .insert(&nsc_core::models::NewChapter {
                    data_asset_id: da_id,
                    idx: i,
                    title: format!("chapter {i}"),
                    body: format!("orig body {i}"),
                    word_count: 3,
                    ..Default::default()
                })
                .unwrap(),
        );
    }
    (da_id, cids)
}

/// 建 tn + batch + prompt/model,并为给定章节插 tc 行。返回 (prompt_id, model_id, tc_ids)。
fn seed_tcs(db: &Arc<Db>, da_id: i64, cids: &[i64]) -> (i64, i64, Vec<i64>) {
    let prompt_id = db.prompts().insert(&make_prompt(0)).unwrap();
    let model_id = db
        .model_configs()
        .insert(&NewModelConfig {
            name: "Test".into(),
            base_url: "http://localhost".into(),
            api_key: "x".into(),
            model: "test-model".into(),
            max_tokens: None,
            max_context: Some(8000),
            temperature: None,
            disable_thinking: false,
            concurrency: 1,
        })
        .unwrap();
    let tn_id = db
        .transformation_novels()
        .insert(&nsc_core::models::NewTransformationNovel {
            data_asset_id: da_id,
            title: "TN".into(),
            note: String::new(),
        })
        .unwrap();
    let batch_id = db
        .batches()
        .insert(&NewBatch {
            transformation_novel_id: tn_id,
            label: None,
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id,
            model_config_id: model_id,
            mode: "compress".into(),
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            ctx_next_transformed: 0,
        })
        .unwrap();
    db.batches()
        .set_status(batch_id, BatchStatus::Running)
        .unwrap();
    let mut tc_ids = Vec::new();
    for &cid in cids {
        tc_ids.push(
            db.transformation_chapters()
                .insert(&NewTransformationChapter {
                    transformation_novel_id: tn_id,
                    chapter_id: cid,
                    mode: PromptKind::Compress,
                    prompt_id,
                    model_config_id: model_id,
                    ctx_prev_original: 0,
                    ctx_prev_transformed: 0,
                    ctx_next_original: 0,
                    batch_id: Some(batch_id),
                    style_ref_chapter_id: None,
                })
                .unwrap(),
        );
    }
    (prompt_id, model_id, tc_ids)
}

fn job_spec(db: &Arc<Db>, tc_id: i64, cid: i64, prompt_id: i64, model_id: i64) -> JobSpec {
    let chapter = db.chapters().get(cid).unwrap().unwrap();
    let tn_id = db
        .transformation_novels()
        .list_by_data_asset(chapter.data_asset_id)
        .unwrap()[0]
        .id;
    JobSpec {
        tc_id,
        tn_id,
        mode: PromptKind::Compress,
        chapter,
        prompt: make_prompt(prompt_id),
        model_config: ModelConfig {
            id: model_id,
            name: "Test".into(),
            base_url: "http://localhost".into(),
            api_key: "x".into(),
            model: "test-model".into(),
            max_tokens: None,
            max_context: Some(8000),
            temperature: None,
            disable_thinking: false,
            concurrency: 1,
            archived: 0,
        },
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }
}

/// 核心回归:job A panic 后 worker 存活并完成 job B。
#[test]
fn worker_survives_job_panic_and_keeps_consuming() {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let (da_id, cids) = seed(&db, 2);
    let (prompt_id, model_id, tc_ids) = seed_tcs(&db, da_id, &cids[0..2]);

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_factory = calls.clone();
    let db_for_workers = db.clone();

    // 单 worker —— 顺序确定:先 A(panic)后 B(成功)。
    // 若没有 per-job panic 边界,worker 会死在 A 上,B 永远停在 pending。
    let queue = JobQueue::new(
        1,
        move || Ok(db_for_workers.clone()),
        move |_cfg: &ModelConfig| -> Box<dyn AiProvider> {
            Box::new(PanicOnceProvider {
                calls: calls_for_factory.clone(),
            })
        },
        Arc::new(NoopRecorder),
        Arc::new(Default::default()),
    );

    queue.enqueue(job_spec(&db, tc_ids[0], cids[0], prompt_id, model_id));
    queue.enqueue(job_spec(&db, tc_ids[1], cids[1], prompt_id, model_id));

    // 等 B 跑完(或超时)。A 会 panic → 被边界隔离,不影响 B。
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut done_b = false;
    while Instant::now() < deadline {
        let snap = queue.snapshot();
        if snap.done.iter().any(|j| j.tc_id == tc_ids[1])
            && snap.failed.iter().any(|j| j.tc_id == tc_ids[0])
        {
            done_b = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let snap = queue.snapshot();
    let summary = format!(
        "pending={:?} running={:?} done={:?} failed={:?}",
        snap.pending.iter().map(|j| j.tc_id).collect::<Vec<_>>(),
        snap.running.iter().map(|j| j.tc_id).collect::<Vec<_>>(),
        snap.done.iter().map(|j| j.tc_id).collect::<Vec<_>>(),
        snap.failed.iter().map(|j| j.tc_id).collect::<Vec<_>>(),
    );

    assert!(
        done_b,
        "job B(tc={}) 必须在 job A(tc={}) panic 后仍被消费完成。实际快照: {summary}",
        tc_ids[1], tc_ids[0],
    );
    // 注:快照的 `running` 列表在 push_done/push_failed 后不会被清理(既有行为,
    // push_done 只往 done 追加),所以这里不断言 running 为空。
    // 该快照的 IPC 消费方 `get_queue_snapshot` 已删除(前端从未调用),本测试是它
    // 现在唯一的读者 —— 只锁定"panic 被隔离且后续 job 仍被消费"这一核心性质。
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "provider 应被调用 2 次(A panic 前 1 次 + B 1 次);实际快照: {summary}",
    );

    // panic 的 A 应被标记 failed 且带可读原因,而不是永久卡在 running、也不是从快照里消失。
    let a = snap
        .failed
        .iter()
        .find(|j| j.tc_id == tc_ids[0])
        .unwrap_or_else(|| panic!("A 应出现在 failed 快照里。实际快照: {summary}"));
    assert_eq!(a.status, JobStatus::Failed);
    let err = a.error.clone().unwrap_or_default();
    assert!(
        err.contains("panic") && err.contains("模拟 job 内部 panic"),
        "失败原因应带上真实的 panic 信息,实际: {err:?}",
    );

    // Db 未被污染:还能正常读写(锁中毒若未恢复,这里会 panic)。
    let (_, total) = db.ai_call_logs().list(&Default::default()).unwrap();
    assert_eq!(total, 0, "NoopRecorder 不落库,读操作只是证明 Db 仍可用");
    assert!(
        db.applied_schema_versions().unwrap().len() > 10,
        "Db 仍可查询"
    );
}
