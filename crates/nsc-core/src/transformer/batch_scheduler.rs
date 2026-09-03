//! 批号调度器:按 frontier 串行派发,跨工作流不共享结果。
//!
//! 单例;持共享 Arc<Db>由 lib.rs 在 JobQueue::set_notifier 时注册。
//!
//! 本片接:
//! - `create_workflow` 原子事务:batch + workflow_results + N 个 tc + N 个空 slot
//! - `on_chapter_done` / `on_chapter_failed` 派下一章(`on_chapter_failed` 按 batch.on_failure_policy 分流)
//! - 0.2 起移除 OnFailurePolicy::Terminate:"失败即终止"与 paused 时手动 Terminate 重复,且误选代价不可逆。
//! - 完成判据 → batch 状态迁移到 Stopped/Terminated 等
//! - `safe_stop_on_dispatch_failure` dispatch 失败的兜底
//! - `stop_workflow` 人工停止 + `retry_empty_slots` 重试空槽
//! - `resume` 配合 pause_and_review 策略,让用户在 paused 时介入

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::OptionalExtension;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::models::{
    Batch, BatchStatus, Chapter, ChapterPreviewRow, ModelConfig, OnFailurePolicy, Prompt,
    PromptKind,
};
use crate::models::AiCallBusiness;
use crate::models::transformation::SeedSource;
use crate::recorder::AiCallRecorder;
use crate::transformer::{
    DefaultTransformer, JobQueue, JobSpec, ProviderFactory, TransformRequest,
    TransformationNovelContext,
};
use crate::transformer::queue::read_context;

pub struct BatchScheduler {
    db: Arc<Db>,
    job_queue: Arc<JobQueue>,
    provider_factory: ProviderFactory,
    recorder: Arc<dyn AiCallRecorder>,
    /// 已知可关思考的 model_id 集合(由启动期 catalog 解析得到)。
    close_thinking: Arc<HashSet<String>>,
    /// 后台 tokio runtime —— regenerate_preview 的 AI 任务在这里跑
    /// (主线程没 tokio reactor,见 src-tauri/src/lib.rs:51 注释)。
    runtime: Arc<tokio::runtime::Runtime>,
}

/// `create_workflow` 入参 —— 字段全是必填,不走任何 TN 默认覆盖。
#[derive(Debug, Clone)]
pub struct WorkflowCreate {
    pub transformation_novel_id: i64,
    pub label: Option<String>,
    pub chapter_ids: Vec<i64>,
    pub prompt_id: i64,
    pub model_config_id: i64,
    pub mode: PromptKind,
    pub ctx_prev_original: i32,
    pub ctx_prev_transformed: i32,
    pub ctx_next_original: i32,
    pub ctx_next_transformed: i32,
    /// 章节失败时的处理策略。
    /// - PauseAndReview: batch → Paused,等用户通过 `resume` 决策。
    /// - Terminate: 同 batch 后续 pending → cancelled,batch → Terminated。
    /// - SkipFailed: 当前 tc → Skipped,继续派下一章(batch 留 Running)。
    pub on_failure_policy: OnFailurePolicy,
    /// 试运行首章结果(由「新建工作流」对话框传入)。None → 全部 tc pending;Some → 事务内把 idx 最小那个 chapter 的 tc 按 source 标 done。
    pub first_chapter_seed: Option<crate::models::transformation::FirstChapterSeed>,
}

impl BatchScheduler {
    pub fn new(
        db: Arc<Db>,
        job_queue: Arc<JobQueue>,
        provider_factory: ProviderFactory,
        recorder: Arc<dyn AiCallRecorder>,
        close_thinking: Arc<HashSet<String>>,
    ) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("BatchScheduler runtime build"),
        );
        Self { db, job_queue, provider_factory, recorder, close_thinking, runtime }
    }

    /// 原子创建并启动工作流(spec §5.1,§12):单事务里写 batches(status='running',started_at=now) +
    /// workflow_results + N × transformation_chapters(status='pending') +
    /// N × 空 workflow_result_chapters;事务提交后立刻 advance_batch 派首章。
    /// 派首章失败时 safe_stop:所有 pending tc → failed,batch → stopped。
    /// 字段全是必填,不回退任何 TN 默认。
    pub fn create_workflow(&self, spec: WorkflowCreate) -> Result<Batch> {
        if spec.chapter_ids.is_empty() {
            return Err(Error::Validation("必须选择至少一个章节".into()));
        }

        // 1. 校验 TN / chapter 归属 / prompt / model / mode↔prompt.kind 一致
        let tn = self.db.transformation_novels().get(spec.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", spec.transformation_novel_id)))?;
        for cid in &spec.chapter_ids {
            let ch = self.db.chapters().get(*cid)?
                .ok_or_else(|| Error::NotFound(format!("chapter {cid} 不存在")))?;
            if ch.data_asset_id != tn.data_asset_id {
                return Err(Error::Validation(format!(
                    "chapter {cid} 不属于 tn 的 data_asset {}", tn.data_asset_id
                )));
            }
        }
        let prompt = self.db.prompts().get(spec.prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {} 不存在", spec.prompt_id)))?;
        // model_config 只用来校验存在性;create_workflow 事务提交后立刻按 tc 行的 model_config_id 派首章。
        let _model = self.db.model_configs().get(spec.model_config_id)?
            .ok_or_else(|| Error::NotFound(format!("model_config {} 不存在", spec.model_config_id)))?;
        if prompt.kind != spec.mode {
            return Err(Error::Validation("prompt kind 与 mode 不一致".into()));
        }

        // 2. 单事务:batch(status=running,started_at=now) + 结果集 + N × tc + N × 空槽
        let now = Utc::now().to_rfc3339();
        let batch_id: i64 = {
            let _bsg = self.db.lock();
            let tx = _bsg.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO batches \
                 (transformation_novel_id, label, on_failure_policy, status, created_at, started_at, \
                  prompt_id, model_config_id, mode, \
                  ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed) \
                 VALUES (?1, ?2, ?3, 'running', ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    spec.transformation_novel_id, spec.label,
                    policy_str(spec.on_failure_policy), now,
                    spec.prompt_id, spec.model_config_id, mode_str(spec.mode),
                    spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
                    spec.ctx_next_transformed,
                ],
            )?;
            let batch_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
                rusqlite::params![batch_id, now],
            )?;
            let result_id: i64 = tx.query_row(
                "SELECT id FROM workflow_results WHERE batch_id = ?1",
                rusqlite::params![batch_id], |r| r.get(0),
            )?;
            for cid in &spec.chapter_ids {
                let frontier_cid = frontier_chapter_id_in_workflow(&_bsg, batch_id, *cid)?;
                tx.execute(
                    "INSERT INTO transformation_chapters                      (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id,                       ctx_prev_original, ctx_prev_transformed, ctx_next_original,                       batch_id, style_ref_chapter_id, status)                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
                    rusqlite::params![
                        spec.transformation_novel_id, *cid, mode_str(spec.mode),
                        spec.prompt_id, spec.model_config_id,
                        spec.ctx_prev_original, spec.ctx_prev_transformed, spec.ctx_next_original,
                        batch_id,
                        frontier_cid,
                    ],
                )?;
                tx.execute(
                    "INSERT INTO workflow_result_chapters                      (workflow_result_id, chapter_id, content, created_at, updated_at)                      VALUES (?1, ?2, NULL, ?3, ?3)",
                    rusqlite::params![result_id, cid, now],
                )?;
            }
            // 试运行首章 seed(spec §3.1):Some → 把 idx 最小那个 chapter 对应的 tc 标 done,
            // wrc.content 同步写。scheduler 后续 advance_batch 跳过该 tc,自然派 idx 次小章节。
            if let Some(seed) = &spec.first_chapter_seed {
                apply_preview_in_tx(&tx, batch_id, &spec.chapter_ids, seed, &now)?;
            }
            tx.commit()?;
            batch_id
        };

        // 创建即运行(spec §12):事务提交后立刻 advance_batch 派首章。
        // 派首章失败时 safe_stop,所有 pending tc → failed,batch → stopped。
        if let Err(e) = self.advance_batch(&self.db, batch_id) {
            let now2 = Utc::now().to_rfc3339();
            let _bsg = self.db.lock();
            let tx2 = _bsg.unchecked_transaction()?;
            tx2.execute(
                "UPDATE transformation_chapters                  SET status='failed', error=?2, completed_at=?3, result_content=NULL,                      tokens_in=NULL, tokens_out=NULL                  WHERE batch_id=?1 AND status='pending'",
                rusqlite::params![batch_id, format!("create_workflow 派首章失败: {e}"), now2],
            )?;
            tx2.execute(
                "UPDATE batches SET status='stopped', ended_at=?1 WHERE id=?2",
                rusqlite::params![now2, batch_id],
            )?;
            tx2.commit()?;
            return Err(e);
        }

        let batch = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 写入后回读失败".into()))?;
        Ok(batch)
    }

    /// 派发一个具体 tc(按 tid)。从 Db 读 tc + chapter,构造 JobSpec 塞进 JobQueue。
    /// ctx_* 从 tc 行读取 —— tc 行是权威来源(`create_workflow` 在 INSERT 时
    /// 已从 `WorkflowCreate.ctx_*` 写入)。调用方不应再传 ctx 参数,避免与 tc 行不同步:
    /// 此前的 bug 就是因为两个调用点写死 0,0,0,永远丢掉了用户在 dialog 上 toggle
    /// 的「带前文/带后文」配置(spec §3.2)。
    pub(crate) fn dispatch(
        &self,
        prompt: &Prompt,
        model: &ModelConfig,
        tid: i64,
    ) -> Result<()> {
        let tc = self.db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let chapter = self.db.chapters().get(tc.chapter_id)?
            .ok_or_else(|| Error::NotFound(format!("chapter {} 不存在", tc.chapter_id)))?;

        let spec = JobSpec {
            // tc.transformation_novel_id 才是 transformation_novels 表的 id；
            // JobQueue / Transformer 根据这个 id 读 tn，不能实际为 transformation_chapters.id (tid)。
            tc_id: tid,
            tn_id: tc.transformation_novel_id,
            // tc.mode 由 `create_workflow` 在 tc 行 INSERT 时写入,是 per-task 的权威值。
            mode: tc.mode,
            chapter: Chapter {
                id: chapter.id,
                data_asset_id: chapter.data_asset_id,
                idx: chapter.idx,
                title: chapter.title.clone(),
                body: chapter.body.clone(),
                word_count: chapter.word_count,
                source_kind: chapter.source_kind.clone(),
                source_chapter_id: chapter.source_chapter_id,
                edited_at: chapter.edited_at.clone(),
                title_line: None,
            },
            prompt: prompt.clone(),
            model_config: model.clone(),
            // ctx_* 来自 `WorkflowCreate`,`create_workflow` 已经写入 tc 行,
            // 这里从 tc 行读 —— JobSpec 和 tc 行保持单一来源,不会再因为参数错传丢上下文。
            ctx_prev_original: tc.ctx_prev_original,
            ctx_prev_transformed: tc.ctx_prev_transformed,
            ctx_next_original: tc.ctx_next_original,
        };
        self.job_queue.enqueue(spec);
        Ok(())
    }

    /// JobQueue 完成回调:把正文写入结果集 + 派下一章(若还有)。
    /// 单事务里标 tc done(清空 `tc.result_content` 回到结果槽),同步写
    /// `workflow_result_chapters.content`,然后 `advance_batch`。
    pub fn on_chapter_done(&self, tid: i64, content: String) -> Result<()> {
        let tc = self.db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let batch_id = match tc.batch_id {
            Some(b) => b,
            None => return Ok(()),  // 散点行(非 batch 入队)不归 scheduler 管
        };
        let now = Utc::now().to_rfc3339();
        {
            let _bsg = self.db.lock();
            let tx = _bsg.unchecked_transaction()?;
            // tc 行:保留已由 worker 写入的 tokens_in/out,清空 result_content(spec §5.x 收口到结果集)。
            tx.execute(
                "UPDATE transformation_chapters                  SET result_content=NULL, completed_at=?1                  WHERE id=?2",
                rusqlite::params![now, tid],
            )?;
            // 同步写结果槽 —— `WorkflowResultRepo::write_content_by_chapter` 通过
            // sub-select 找 workflow_results.id,对未建结果集 / 缺槽的 batch 静默 noop,
            // 让老 batch(非工作流)路径也能调到这里而不报错。
            tx.execute(
                "UPDATE workflow_result_chapters                  SET content=?2, updated_at=?3                  WHERE workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id=?4)                    AND chapter_id=?1",
                rusqlite::params![tc.chapter_id, content, now, batch_id],
            )?;
            tx.commit()?;
        }
        self.advance_batch(&self.db, batch_id)
    }

    /// 失败回调:按 batch.on_failure_policy 分流。
    /// - PauseAndReview: tc → failed,batch → paused(ended_at 设 NOW),不 advance。
    /// - SkipFailed:      tc → skipped,advance_batch 派下一章(batch 保持 running)。
    ///
    /// batch 收尾交给 advance_batch → maybe_finalize_batch(skip_failed 走这条)。
    pub fn on_chapter_failed(&self, tid: i64, error: String) -> Result<()> {
        let tc = self.db.transformation_chapters().get(tid)?
            .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
        let Some(batch_id) = tc.batch_id else { return Ok(()); };
        let batch = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        let now = Utc::now().to_rfc3339();
        match batch.on_failure_policy {
            OnFailurePolicy::PauseAndReview => {
                let _bsg = self.db.lock();
            let tx = _bsg.unchecked_transaction()?;
                tx.execute(
                    "UPDATE transformation_chapters                      SET status='failed', error=?2, completed_at=?3, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL                      WHERE id=?1",
                    rusqlite::params![tid, error, now],
                )?;
                tx.execute(
                    "UPDATE batches SET status='paused', ended_at=?1 WHERE id=?2",
                    rusqlite::params![now, batch_id],
                )?;
                tx.commit()?;
                Ok(())
            }
                        OnFailurePolicy::SkipFailed => {
                // scope 锁,commit 后立刻 drop —— 否则后续 advance_batch 内部
                // `self.db.batches().get(batch_id)` 会再次 lock,std::sync::Mutex 非可重入 → 死锁。
                {
                    let _bsg = self.db.lock();
                    let tx = _bsg.unchecked_transaction()?;
                    tx.execute(
                        "UPDATE transformation_chapters                      SET status='skipped', error=?2, completed_at=?3, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL                      WHERE id=?1",
                        rusqlite::params![tid, error, now],
                    )?;
                    tx.commit()?;
                }
                self.advance_batch(&self.db, batch_id)
            }
        }
    }

    /// 派下一章(若有);完成判据。
    fn advance_batch(&self, db: &Db, batch_id: i64) -> Result<()> {
        // 取 batch 内第一个 pending 行(按 chapter_idx ASC)
        let next_tid: Option<i64> = {
        let _bsg = self.db.lock();
            let mut stmt = _bsg.prepare(
                "SELECT transformation_chapters.id FROM transformation_chapters                  JOIN chapters c ON c.id = transformation_chapters.chapter_id                  WHERE transformation_chapters.batch_id = ?1                    AND transformation_chapters.status = 'pending'                  ORDER BY c.idx ASC, transformation_chapters.id ASC                  LIMIT 1",
            )?;
            let mut rows = stmt.query(rusqlite::params![batch_id])?;
            if let Some(row) = rows.next()? { Some(row.get(0)?) } else { None }
        };

        if let Some(tid) = next_tid {
            // 还有 pending → 派下一章。prompt_id / model_config_id 从 tc 行直接读,
            // 跟 create_workflow 派首章对齐:WorkflowCreate.prompt_id/model_config_id
            // 在事务里已经写进每个 tc 行(`INSERT ... prompt_id, model_config_id`),
            // 无需任何 TN 层 fallback。
            let next_tc = self.db.transformation_chapters().get(tid)?
                .ok_or_else(|| Error::NotFound(format!("tc {tid} 不存在")))?;
            let prompt_id = next_tc.prompt_id;
            let model_cfg_id = next_tc.model_config_id;
            let prompt = self.db.prompts().get(prompt_id)?
                .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
            let model = self.db.model_configs().get(model_cfg_id)?
                .ok_or_else(|| Error::NotFound(format!("model_config {model_cfg_id} 不存在")))?;
            return self.dispatch(&prompt, &model, tid);
        }

        // 没 pending 了 → 完成判据
        self.maybe_finalize_batch(db, batch_id)
    }

    /// §3.3 / §5.2 收尾判据:批次内不存在 pending/running 任务 → batch → Stopped。
    /// Failed/Done/Skipped/Cancelled 都不阻塞收尾。
    /// COALESCE(ended_at, ?1) 保留已有 ended_at(如 Task 7 手动停止写入的)。
    fn maybe_finalize_batch(&self, _db: &Db, batch_id: i64) -> Result<()> {
        let active: i64 = self.db.lock().query_row(
            "SELECT COUNT(*) FROM transformation_chapters              WHERE batch_id = ?1 AND status IN ('pending','running')",
            rusqlite::params![batch_id],
            |row| row.get(0),
        )?;
        if active > 0 {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        self.db.lock().execute(
            "UPDATE batches SET status='stopped', ended_at = COALESCE(ended_at, ?1) WHERE id = ?2",
            rusqlite::params![now, batch_id],
        )?;
        Ok(())
    }

    /// 人工停止运行中的工作流(spec §6.1):事务里把全部 Pending 标 Skipped、结果槽保持空;
    /// 若当时没有 Running 任务,批次直接转 Stopped,否则等 worker 回调 finalize。
    /// 对已 Stopped 批次幂等返回(spec §10);其余状态(非 Running、非 Stopped)→ Validation。
    pub fn stop_workflow(&self, batch_id: i64) -> Result<Batch> {
        let batch = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if matches!(batch.status, BatchStatus::Stopped) {
            return Ok(batch);
        }
        if !matches!(batch.status, BatchStatus::Running) {
            return Err(Error::Validation(format!(
                "batch {batch_id} 状态 {:?} 不能 stop", batch.status
            )));
        }
        let now = Utc::now().to_rfc3339();
        // 整段进事务:skip pending tc + 视 in-flight 情况 batch → stopped +
        // 回读最新 batch —— 全程只持一次 std::sync::Mutex 锁。
        // 旧实现 commit 后再 self.db.batches().get(...) 第二次 lock(),std::sync::Mutex
        // 非可重入 → 死锁(线上复现:日志停 after tx.commit(),后续无输出)。
        let _bsg = self.db.lock();
        let tx = _bsg.unchecked_transaction()?;
        tx.execute(
            "UPDATE transformation_chapters SET status='skipped', completed_at=?2              WHERE batch_id=?1 AND status='pending'",
            rusqlite::params![batch_id, now],
        )?;
        let has_running: i64 = tx.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id=?1 AND status='running'",
            rusqlite::params![batch_id], |r| r.get(0),
        )?;
        if has_running == 0 {
            tx.execute(
                "UPDATE batches SET status='stopped', ended_at=?1 WHERE id=?2",
                rusqlite::params![now, batch_id],
            )?;
        }
        // 事务内回读最新 batch —— 失败映射 QueryReturnedNoRows → NotFound,
        // 其他 rusqlite 错误经 `?` 由 #[from] 自动转 Error::Db。
        let updated = match tx.query_row(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at \
             FROM batches WHERE id = ?1",
            rusqlite::params![batch_id],
            crate::db::repo::batch::batch_from_row,
        ) {
            Ok(b) => b,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::NotFound("batch 回读失败".into())),
            Err(e) => return Err(Error::Db(e)),
        };
        tx.commit()?;
        Ok(updated)
    }

    /// Stopped 后重试空槽(spec §6.2):把所选 Failed/Skipped 任务重置为 Pending,
    /// 仅当结果槽为 NULL 时通过;事务提交后 batch → Running,派序号最小的章节。
    /// 已被填过的槽重试会返回 Validation(spec §10)。
    pub fn retry_empty_slots(&self, batch_id: i64, chapter_ids: &[i64]) -> Result<Batch> {
        let batch = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        // Stopped: 原语义；Running/Paused: 允许在没有其他 running tc 的前提下对失败/跳过单章重试,避免重复 dispatch。
        let in_flight: i64 = self.db.lock().query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id=?1 AND status='running'",
            rusqlite::params![batch_id], |r| r.get(0),
        )?;
        match batch.status {
            BatchStatus::Stopped => {}
            BatchStatus::Running | BatchStatus::Paused if in_flight == 0 => {}
            _ => return Err(Error::Validation("当前 batch 状态不可重试（仅允许 Stopped 或无 in-flight 的 Running/Paused）".to_string())),
        }
        if chapter_ids.is_empty() {
            return Err(Error::Validation("必须至少选择一个章节".into()));
        }
        let first_tid: i64 = {
            let _bsg = self.db.lock();
            let tx = _bsg.unchecked_transaction()?;
            for cid in chapter_ids {
                let updated = tx.execute(
                    "UPDATE transformation_chapters                      SET status='pending', error=NULL, result_content=NULL,                          tokens_in=NULL, tokens_out=NULL, started_at=NULL, completed_at=NULL                      WHERE batch_id=?1                        AND chapter_id=?2                        AND status IN ('failed','skipped')                        AND (SELECT content FROM workflow_result_chapters wrc                              JOIN workflow_results wr ON wr.id = wrc.workflow_result_id                              WHERE wr.batch_id = transformation_chapters.batch_id                                AND wrc.chapter_id = transformation_chapters.chapter_id) IS NULL",
                    rusqlite::params![batch_id, cid],
                )?;
                if updated == 0 {
                    return Err(Error::Validation(format!(
                        "章节 {cid} 不是可重试空槽(不存在/非 failed-skipped/结果槽非空)"
                    )));
                }
            }
            if matches!(batch.status, BatchStatus::Stopped) {
                tx.execute(
                    "UPDATE batches SET status='running', ended_at=NULL WHERE id=?1",
                    rusqlite::params![batch_id],
                )?;
            }
            let first_tid: i64 = tx.query_row(
                "SELECT tc.id FROM transformation_chapters tc                  JOIN chapters c ON c.id = tc.chapter_id                  WHERE tc.batch_id=?1 AND tc.status='pending'                  ORDER BY c.idx ASC LIMIT 1",
                rusqlite::params![batch_id], |r| r.get(0),
            )?;
            tx.commit()?;
            first_tid
        };
        // 派首章(事务外):从 tc 行读固化好的 prompt/model(跟 create_workflow 对齐)。
        let prompt_id: i64 = self.db.lock().query_row(
            "SELECT prompt_id FROM transformation_chapters WHERE id=?1",
            rusqlite::params![first_tid], |r| r.get(0),
        )?;
        let model_id: i64 = self.db.lock().query_row(
            "SELECT model_config_id FROM transformation_chapters WHERE id=?1",
            rusqlite::params![first_tid], |r| r.get(0),
        )?;
        let prompt = self.db.prompts().get(prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
        let model = self.db.model_configs().get(model_id)?
            .ok_or_else(|| Error::NotFound(format!("model {model_id} 不存在")))?;
        self.dispatch(&prompt, &model, first_tid)?;
        let updated = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound("batch 回读失败".into()))?;
        Ok(updated)
    }

    /// 测试用:当前 batch 状态(方便断言)。
    pub fn batch_status(&self, batch_id: i64) -> Result<BatchStatus> {
        let b = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        Ok(b.status)
    }

    /// 异步发起一次预览生成(spec §5.1)。返回新插入的 preview id。
    /// AI 调用在 self.runtime 上跑 —— 调用方立即拿到 preview_id,
    /// 实际生成在后台进行,完成后由 chapter_previews 行 status 反映。
    pub fn regenerate_preview(
        &self,
        batch_id: i64,
        chapter_id: i64,
        custom_input: Option<String>,
    ) -> Result<i64> {
        let batch = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        let chapter = self.db.chapters().get(chapter_id)?
            .ok_or_else(|| Error::NotFound(format!("chapter {chapter_id} 不存在")))?;
        let tn = self.db.transformation_novels().get(batch.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
        let (prompt_id, model_config_id, ctx_prev_original, ctx_prev_transformed, ctx_next_original): (
            i64, i64, i32, i32, i32,
        ) = self.db.lock().query_row(
            "SELECT prompt_id, model_config_id, ctx_prev_original, ctx_prev_transformed, ctx_next_original \
             FROM transformation_chapters \
             WHERE batch_id=?1 AND chapter_id=?2",
            rusqlite::params![batch_id, chapter_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        let prompt = self.db.prompts().get(prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {prompt_id} 不存在")))?;
        let model = self.db.model_configs().get(model_config_id)?
            .ok_or_else(|| Error::NotFound(format!("model {model_config_id} 不存在")))?;
        if chapter.data_asset_id != tn.data_asset_id {
            return Err(Error::Validation(format!(
                "chapter {chapter_id} 不属于 tn 的 data_asset {}",
                tn.data_asset_id
            )));
        }

        let preview_id = self.db.chapter_previews()
            .insert_generating(batch_id, chapter_id, custom_input.as_deref())?;

        let job = JobSpec {
            tc_id: preview_id,
            tn_id: batch.transformation_novel_id,
            mode: prompt.kind,
            chapter: chapter.clone(),
            prompt: prompt.clone(),
            model_config: model.clone(),
            ctx_prev_original,
            ctx_prev_transformed,
            ctx_next_original,
        };
        let prep = read_context(&self.db, &job).map_err(Error::Other)?;

        let req = TransformRequest {
            transformation_id: batch.transformation_novel_id,
            chapter: prep.chapter,
            chapter_content: prep.chapter_content,
            novel_context: TransformationNovelContext {
                transformation_novel: prep.transformation_novel,
                prev_original: prep.prev_orig,
                prev_transformed: prep.prev_tx,
                next_original: prep.next_orig,
            },
            prompt: prompt.clone(),
            model_config: model.clone(),
            custom_input: custom_input.clone(),
            preview_id: Some(preview_id),
        };

        let provider = (self.provider_factory)(&model);
        let recorder = self.recorder.clone();
        let db = self.db.clone();
        let close_thinking = self.close_thinking.clone();
        self.runtime.spawn(async move {
            let transformer = DefaultTransformer::new(provider.into(), recorder.clone(), close_thinking.clone());
            let result = transformer.transform_with_business(req, AiCallBusiness::RegeneratePreview).await;
            let update_result = (|| -> Result<()> {
                match &result {
                    Ok(out) => db.chapter_previews().update_done(
                        preview_id, &out.result_content, out.tokens_in, out.tokens_out,
                    )?,
                    Err(e) => db.chapter_previews().update_failed(preview_id, &e.to_string())?,
                }
                Ok(())
            })();
            if let Err(e) = update_result {
                eprintln!("[regenerate_preview] 更新 preview {preview_id} 失败: {e}");
            }
        });

        Ok(preview_id)
    }

    /// 列出 (batch_id, chapter_id) 下的全部 preview 行,按 id DESC —— spec §5.1。
    pub fn list_chapter_previews(&self, batch_id: i64, chapter_id: i64) -> Result<Vec<ChapterPreviewRow>> {
        self.db.chapter_previews().list_by_chapter(batch_id, chapter_id)
    }

    /// 放弃某个 preview 行(直接删除,不管 status;generating 也允许)—— spec §5.1。
    pub fn discard_preview(&self, preview_id: i64) -> Result<()> {
        self.db.chapter_previews().delete(preview_id)
    }

    /// 用草稿区内容覆写 wrc.content(spec §4.2):单事务里写 wrc.content +
    /// tc.status='done' + 更新 tc.tokens(优先 source_preview_id,fallback 最新一条 done,都没有则 NULL)
    /// + 清空该章节所有 preview 行。不修改 batch 状态(spec §7.1)。
    pub fn commit_preview(
        &self,
        batch_id: i64,
        chapter_id: i64,
        draft_content: String,
        source_preview_id: Option<i64>,
    ) -> Result<Batch> {
        let now = Utc::now().to_rfc3339();
        {
            let _bsg = self.db.lock();
            let tx = _bsg.unchecked_transaction()?;
            let tc_id: i64 = tx.query_row(
                "SELECT id FROM transformation_chapters                  WHERE batch_id=?1 AND chapter_id=?2",
                rusqlite::params![batch_id, chapter_id],
                |r| r.get(0),
            ).optional()?.ok_or_else(|| Error::NotFound(format!(
                "transformation_chapter not found for batch={batch_id} chapter={chapter_id}"
            )))?;
            let wr_id: i64 = tx.query_row(
                "SELECT id FROM workflow_results WHERE batch_id=?1",
                rusqlite::params![batch_id],
                |r| r.get(0),
            ).optional()?.ok_or_else(|| Error::NotFound(format!(
                "workflow_result not found for batch={batch_id}"
            )))?;
            let updated = tx.execute(
                "UPDATE workflow_result_chapters                  SET content=?3, updated_at=?4                  WHERE workflow_result_id=?1 AND chapter_id=?2",
                rusqlite::params![wr_id, chapter_id, draft_content, now],
            )?;
            if updated == 0 {
                return Err(Error::NotFound("workflow_result_chapter slot missing".into()));
            }
            tx.execute(
                "UPDATE transformation_chapters                  SET status='done', error=NULL, result_content=NULL, completed_at=?2                  WHERE id=?1",
                rusqlite::params![tc_id, now],
            )?;
            let mut tokens: (Option<i32>, Option<i32>) = (None, None);
            let mut source_used = false;
            if let Some(pid) = source_preview_id {
                let mut stmt = tx.prepare(
                    "SELECT tokens_in, tokens_out FROM chapter_previews WHERE id=?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![pid])?;
                if let Some(row) = rows.next()? {
                    tokens = (row.get(0)?, row.get(1)?);
                    source_used = true;
                }
            }
            if !source_used {
                let mut stmt = tx.prepare(
                    "SELECT tokens_in, tokens_out FROM chapter_previews                  WHERE batch_id=?1 AND chapter_id=?2 AND status='done'                  ORDER BY id DESC LIMIT 1",
                )?;
                let mut rows = stmt.query(rusqlite::params![batch_id, chapter_id])?;
                if let Some(row) = rows.next()? {
                    tokens = (row.get(0)?, row.get(1)?);
                }
            }
            tx.execute(
                "UPDATE transformation_chapters                  SET tokens_in=?2, tokens_out=?3                  WHERE id=?1",
                rusqlite::params![tc_id, tokens.0, tokens.1],
            )?;
            tx.execute(
                "DELETE FROM chapter_previews WHERE batch_id=?1 AND chapter_id=?2",
                rusqlite::params![batch_id, chapter_id],
            )?;
            tx.commit()?;
        }
        self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))
    }
    /// 「新建工作流」试运行区 —— 调一次 AI 跑指定章节,返回 preview 结果(spec §3.4 / §5.1)。
    /// 不写 batch / tc / wrc 行;仅写一条 ai_call_logs(business=RegeneratePreview,
    /// context_type=transformation_chapter + context_id=tn_id)。
    /// 用户满意后通过 `create_workflow` 的 `preview_first_chapter` 入参传入此结果。
    ///
    /// ## 入参语义
    /// - `tn_id`:定位 data_asset_id(章节范围必须落在同一 da 下)。
    /// - `chapter_id`:要预览的章节(spec 默认 = selectedChapterIds 中 idx 最小那个)。
    /// - `include_prev` / `include_next`:从 toggle 转的 boolean —— true 时分别取最近 1 章原文。
    /// - `custom_input`:「附加指令」(本期 UI 不暴露,留 TODO)。
    ///
    /// ## 实现要点
    /// - `prev_transformed` 始终空:试运行时还没工作流,没有转换结果可拿。
    /// - 直接调 `DefaultTransformer::transform_with_business(req, RegeneratePreview)`,
    ///   不走 queue / worker(没 batch_id 可 dispatch)。
    pub async fn preview_first_chapter(
        &self,
        input: crate::models::transformation::PreviewFirstChapterInput,
    ) -> Result<crate::models::transformation::PreviewFirstChapterOutcome> {
        // 1. 校验 tn + chapter + 范围归属
        let tn = self.db.transformation_novels().get(input.tn_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", input.tn_id)))?;
        let chapter = self.db.chapters().get(input.chapter_id)?
            .ok_or_else(|| Error::NotFound(format!("chapter {} 不存在", input.chapter_id)))?;
        if chapter.data_asset_id != tn.data_asset_id {
            return Err(Error::Validation(format!(
                "chapter {}(da={}) 不属于 tn {}(da={})",
                chapter.id, chapter.data_asset_id, tn.id, tn.data_asset_id,
            )));
        }
        // 2. 读 prompt + model_config(挡 archived:防止空 api_key 进 AI 调用)。
        let prompt = self.db.prompts().get(input.prompt_id)?
            .ok_or_else(|| Error::NotFound(format!("prompt {} 不存在", input.prompt_id)))?;
        let model = self.db.model_configs().get(input.model_config_id)?
            .ok_or_else(|| Error::NotFound(format!("model_config {} 不存在", input.model_config_id)))?;
        if model.archived != 0 {
            return Err(Error::Validation(format!(
                "model_config {} 已归档,无法预览", input.model_config_id,
            )));
        }
        // 3. 按 include_prev/include_next 拼前后文(N=1)。
        let prev_original: Vec<(String, String)> = if input.include_prev {
            let chs = self.db.chapters().prev_n(chapter.data_asset_id, chapter.idx, 1)?;
            chs.into_iter().map(|c| (c.title, c.body)).collect()
        } else {
            Vec::new()
        };
        let next_original: Vec<(String, String)> = if input.include_next {
            let chs = self.db.chapters().next_n(chapter.data_asset_id, chapter.idx, 1)?;
            chs.into_iter().map(|c| (c.title, c.body)).collect()
        } else {
            Vec::new()
        };
        let prev_transformed: Vec<(String, String)> = Vec::new();
        // 4. 组装 TransformRequest 调 transformer。
        let req = TransformRequest {
            transformation_id: input.tn_id,
            chapter: chapter.clone(),
            chapter_content: chapter.body.clone(),
            novel_context: TransformationNovelContext {
                transformation_novel: tn.clone(),
                prev_original,
                prev_transformed,
                next_original,
            },
            prompt: prompt.clone(),
            model_config: model.clone(),
            custom_input: input.custom_input.clone(),
            preview_id: None,
        };
        let provider = (self.provider_factory)(&model);
        let recorder = self.recorder.clone();
        let close_thinking = self.close_thinking.clone();
        let outcome = DefaultTransformer::new(provider.into(), recorder, close_thinking)
            .transform_with_business(req, AiCallBusiness::RegeneratePreview)
            .await?;
        Ok(crate::models::transformation::PreviewFirstChapterOutcome {
            content: outcome.result_content,
            tokens_in: outcome.tokens_in,
            tokens_out: outcome.tokens_out,
        })
    }

    /// append chapters 到 stopped batch。
    /// 1. 校验 batch 存在 + status==Stopped
    /// 2. 校验 chapter_ids 都属于 tn.data_asset
    /// 3. 去重:剔除已在 batch 中的章节
    /// 4. 事务:insert tc + wrc 空槽 + set_status(Running)
    /// 5. advance_batch 派首章(按 chapter_idx ASC),后续由 on_chapter_done 自然推进
    ///
    /// 注意:不要循环 dispatch 每个新 tc — `advance_batch` 已覆盖所有 pending 派发,
    /// 循环 dispatch 会让同一个 tc 入队两次,worker 跑两次(第二次覆写第一次正文)。
    pub fn append_chapters_to_batch(
        &self,
        batch_id: i64,
        chapter_ids: Vec<i64>,
    ) -> Result<Vec<i64>> {
        if chapter_ids.is_empty() {
            return Err(Error::Validation("至少选 1 章".into()));
        }
        // 1. 读 batch + 校验 status
        let batch = self.db.batches().get(batch_id)?
            .ok_or_else(|| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if batch.status != BatchStatus::Stopped {
            return Err(Error::Validation(format!(
                "仅 stopped 工作流可追加章节(当前 {:?})", batch.status
            )));
        }
        // 2. 读 tn + 校验 chapter_ids 属于 tn.data_asset
        let tn = self.db.transformation_novels().get(batch.transformation_novel_id)?
            .ok_or_else(|| Error::NotFound(format!("tn {} 不存在", batch.transformation_novel_id)))?;
        let da_chapter_set: HashSet<i64> = self.db.chapters().list_by_data_asset(tn.data_asset_id)?
            .iter().map(|c| c.id).collect();
        for &cid in &chapter_ids {
            if !da_chapter_set.contains(&cid) {
                return Err(Error::Validation(format!(
                    "chapter {cid} 不属于本 tn 的 data_asset {}", tn.data_asset_id
                )));
            }
        }
        // 3. 去重
        let existing: HashSet<i64> = self.db.transformation_chapters().list_by_batch(batch_id)?
            .iter().map(|tc| tc.chapter_id).collect();
        let to_add: Vec<i64> = chapter_ids.iter().copied().filter(|c| !existing.contains(c)).collect();
        if to_add.is_empty() {
            return Err(Error::Validation("所选章节全部已在工作流中".into()));
        }
        // 4. 事务:insert tc + wrc 空槽 + set_status(Running)。
        // 全部走 tx.execute —— 持锁时再调 self.db.foo() 会让 std::sync::Mutex 非可重入 → 死锁。
        let now = Utc::now().to_rfc3339();
        let mut new_tc_ids: Vec<i64> = Vec::with_capacity(to_add.len());
        {
            let _bsg = self.db.lock();
            let tx = _bsg.unchecked_transaction()?;
            for &cid in &to_add {
                tx.execute(
                    "INSERT INTO transformation_chapters \
                     (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                      ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                      batch_id, style_ref_chapter_id, status) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'pending')",
                    rusqlite::params![
                        batch.transformation_novel_id, cid, batch.mode.as_str(),
                        batch.prompt_id, batch.model_config_id,
                        batch.ctx_prev_original, batch.ctx_prev_transformed,
                        batch.ctx_next_original,
                        batch_id,
                    ],
                )?;
                new_tc_ids.push(tx.last_insert_rowid());
            }
            // workflow_results + 空槽 —— inline create_for_batch_with_slots 语义。
            tx.execute(
                "INSERT OR IGNORE INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
                rusqlite::params![batch_id, now],
            )?;
            let result_id: i64 = tx.query_row(
                "SELECT id FROM workflow_results WHERE batch_id = ?1",
                rusqlite::params![batch_id], |r| r.get(0),
            )?;
            for &cid in &to_add {
                tx.execute(
                    "INSERT OR IGNORE INTO workflow_result_chapters \
                     (workflow_result_id, chapter_id, content, created_at, updated_at) \
                     VALUES (?1, ?2, NULL, ?3, ?3)",
                    rusqlite::params![result_id, cid, now],
                )?;
            }
            // batch 状态迁移 stopped → running(等价于 set_status(Running),inline 后不需重新 lock)。
            tx.execute(
                "UPDATE batches SET status='running', started_at=COALESCE(started_at, ?2), ended_at=NULL WHERE id=?1",
                rusqlite::params![batch_id, now],
            )?;
            tx.commit()?;
        }
        // 5. advance_batch 派首章(按 chapter_idx ASC),后续由 on_chapter_done 自然推进。
        self.advance_batch(&self.db, batch_id)?;
        Ok(new_tc_ids)
    }
}

/// frontier 章节 id(spec §5.3):仅读当前工作流结果集里的最近非空 slot。
fn frontier_chapter_id_in_workflow(guard: &rusqlite::Connection,
    batch_id: i64,
    chapter_id: i64,
) -> Result<Option<i64>> {
    let mut stmt = guard.prepare(
        "SELECT c.id FROM workflow_result_chapters wrc          JOIN workflow_results wr ON wr.id = wrc.workflow_result_id          JOIN chapters c ON c.id = wrc.chapter_id          WHERE wr.batch_id = ?1            AND wrc.content IS NOT NULL            AND c.idx < (SELECT idx FROM chapters WHERE id = ?2)          ORDER BY c.idx DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(rusqlite::params![batch_id, chapter_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

fn policy_str(p: OnFailurePolicy) -> &'static str {
    match p {
        OnFailurePolicy::PauseAndReview => "pause_and_review",
        OnFailurePolicy::SkipFailed => "skip_failed",
    }
}

fn mode_str(m: PromptKind) -> &'static str {
    match m {
        PromptKind::Compress => "compress",
        PromptKind::Style => "style",
    }
}

/// 把试运行首章结果落库(在 `create_workflow` 事务内 INSERT 完所有 tc + wrc 之后调用)。
///   - 找 idx 最小那个 chapter 对应的 tc,标 done + 写 result_content + tokens + completed_at
///   - workflow_result_chapters 对应行的 content 同步写
///   - scheduler 后续 advance_batch 跳过 idx 最小那个 tc,自然派 idx 次小的章节。
pub(crate) fn apply_preview_in_tx(
    tx: &rusqlite::Transaction,
    batch_id: i64,
    chapter_ids: &[i64],
    seed: &crate::models::transformation::FirstChapterSeed,
    now: &str,
) -> Result<()> {
    if chapter_ids.is_empty() {
        return Ok(());
    }
    // 1. 找 idx 最小那个 chapter_id
    let placeholders = std::iter::repeat_n("?", chapter_ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id FROM chapters WHERE id IN ({}) ORDER BY idx ASC, id ASC LIMIT 1",
        placeholders,
    );
    let mut stmt = tx.prepare(&sql)?;
    let first_chapter_id: i64 = stmt
        .query_row(rusqlite::params_from_iter(chapter_ids.iter()), |r| r.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows =>
                Error::Validation("preview seed: 章节列表为空".into()),
            other => other.into(),
        })?;
    // 2. UPDATE tc:标 done + 写 result_content + tokens + completed_at
    // 按 source 分支取 tokens:Llm 用 LLM 实算;Manual 写 0。
    let (tokens_in, tokens_out) = match seed.source {
        SeedSource::Llm { tokens_in, tokens_out } => (tokens_in, tokens_out),
        SeedSource::Manual => (0, 0),
    };
    tx.execute(
        "UPDATE transformation_chapters SET status='done', result_content=?1, tokens_in=?2, tokens_out=?3, started_at=?4, completed_at=?4, error=NULL WHERE batch_id=?5 AND chapter_id=?6",
        rusqlite::params![seed.content, tokens_in, tokens_out, now, batch_id, first_chapter_id],
    )?;
    // 3. UPDATE wrc:写 content + updated_at
    tx.execute(
        "UPDATE workflow_result_chapters SET content=?1, updated_at=?2 WHERE workflow_result_id=(SELECT id FROM workflow_results WHERE batch_id=?3) AND chapter_id=?4",
        rusqlite::params![seed.content, now, batch_id, first_chapter_id],
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::db::Db;
    use crate::models::transformation::{FirstChapterSeed, SeedSource};
    use crate::models::{
        NewChapter, NewDataAsset, NewTransformationNovel, NewUpload,
        Prompt, PromptKind, TransformStatus,
    };
    use crate::models::model_config::NewModelConfig;

    fn fresh_db() -> Arc<Db> {
        let dir = tempfile::tempdir().unwrap();
        crate::db::Db::open(&dir.path().join("test.db")).unwrap()
    }

    /// 最小可运行环境:1 upload + 1 da + 3 chapter(idx 0..2) + 1 tn + 1 prompt + 1 model。
    /// 返回 (tn_id, c0, c1, c2, prompt_id, model_id)。
    fn seed_env(db: &Db) -> (i64, i64, i64, i64, i64, i64) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "x".into(), filename: "f.txt".into(), byte_size: 10,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 4,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "源".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(), note: "".into(),
        }).unwrap();
        let c0 = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: 0, title: "c0".into(),
            body: "正文0".into(), word_count: 5, ..Default::default()
        }).unwrap();
        let c1 = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: 1, title: "c1".into(),
            body: "正文1".into(), word_count: 5, ..Default::default()
        }).unwrap();
        let c2 = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: 2, title: "c2".into(),
            body: "正文2".into(), word_count: 5, ..Default::default()
        }).unwrap();
        let prompt_id = db.prompts().insert(&Prompt {
            id: 0, name: "test".into(), kind: PromptKind::Style,
            template: "压缩".into(), is_builtin: false, archived: 0,
        }).unwrap();
        let model_id = db.model_configs().insert(&NewModelConfig {
            name: "m".into(), base_url: "http://127.0.0.1:1".into(), api_key: "k".into(),
            model: "m".into(), max_tokens: None, max_context: None,
            temperature: None, disable_thinking: false, concurrency: 1,
        }).unwrap();
        (tn_id, c0, c1, c2, prompt_id, model_id)
    }

    /// 在 db 上手动建 batch + 3 个 tc(pending),跳过 BatchScheduler::create_workflow(避免派发副作用)。
    /// 返回 batch_id。
    fn seed_batch_with_tcs(
        db: &Db, tn_id: i64, c0: i64, c1: i64, c2: i64, prompt_id: i64, model_id: i64,
    ) -> i64 {
        let now = Utc::now().to_rfc3339();
        let _bsg = db.lock();
        let tx = _bsg.unchecked_transaction().unwrap();
        tx.execute(
            "INSERT INTO batches \
             (transformation_novel_id, label, on_failure_policy, status, created_at, started_at, \
              prompt_id, model_config_id, mode, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed) \
             VALUES (?1, ?2, ?3, \"running\", ?4, ?4, ?5, ?6, \"compress\", 0, 0, 0, 0)",
            rusqlite::params![
                tn_id, "test", "pause_and_review", now,
                prompt_id, model_id,
            ],
        ).unwrap();
        let batch_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
            rusqlite::params![batch_id, now],
        ).unwrap();
        let result_id: i64 = tx.query_row(
            "SELECT id FROM workflow_results WHERE batch_id = ?1",
            rusqlite::params![batch_id], |r| r.get(0),
        ).unwrap();
        for cid in &[c0, c1, c2] {
            tx.execute(
                "INSERT INTO transformation_chapters \
                 (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
                  ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
                  batch_id, style_ref_chapter_id, status) \
                 VALUES (?1, ?2, \"style\", ?3, ?4, 0, 0, 0, ?5, NULL, \"pending\")",
                rusqlite::params![tn_id, *cid, prompt_id, model_id, batch_id],
            ).unwrap();
            tx.execute(
                "INSERT INTO workflow_result_chapters \
                 (workflow_result_id, chapter_id, content, created_at, updated_at) \
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                rusqlite::params![result_id, *cid, now],
            ).unwrap();
        }
        tx.commit().unwrap();
        drop(_bsg);
        batch_id
    }

    #[test]
    fn apply_preview_seeds_first_chapter_done() {
        let db = fresh_db();
        let (tn_id, c0, c1, c2, prompt_id, model_id) = seed_env(&db);
        let batch_id = seed_batch_with_tcs(&db, tn_id, c0, c1, c2, prompt_id, model_id);
        let seed = FirstChapterSeed {
            content: "preview result".into(),
            source: SeedSource::Llm { tokens_in: 100, tokens_out: 200 },
        };
        let now = Utc::now().to_rfc3339();
        let _bsg = db.lock();
        let tx = _bsg.unchecked_transaction().unwrap();
        apply_preview_in_tx(&tx, batch_id, &[c0, c1, c2], &seed, &now).unwrap();
        tx.commit().unwrap();
        drop(_bsg);
        let tcs = db.transformation_chapters().list_by_batch(batch_id).unwrap();
        let tc0 = tcs.iter().find(|t| t.chapter_id == c0).unwrap();
        let tc1 = tcs.iter().find(|t| t.chapter_id == c1).unwrap();
        let tc2 = tcs.iter().find(|t| t.chapter_id == c2).unwrap();
        assert_eq!(tc0.status, TransformStatus::Done);
        assert_eq!(tc0.result_content.as_deref(), Some("preview result"));
        assert_eq!(tc0.tokens_in, Some(100));
        assert_eq!(tc0.tokens_out, Some(200));
        assert!(tc0.started_at.is_some());
        assert!(tc0.error.is_none());
        assert_eq!(tc1.status, TransformStatus::Pending);
        assert_eq!(tc2.status, TransformStatus::Pending);
        let wrc0 = db.workflow_results().get_content_by_batch_and_chapter(batch_id, c0).unwrap();
        assert_eq!(wrc0.as_deref(), Some("preview result"));
        let wrc1 = db.workflow_results().get_content_by_batch_and_chapter(batch_id, c1).unwrap();
        assert!(wrc1.is_none());
    }

    #[test]
    fn apply_preview_seeds_first_chapter_done_manual() {
        // Manual 路径:content 写,tokens_in/out = 0。
        let db = fresh_db();
        let (tn_id, c0, c1, c2, prompt_id, model_id) = seed_env(&db);
        let batch_id = seed_batch_with_tcs(&db, tn_id, c0, c1, c2, prompt_id, model_id);
        let seed = FirstChapterSeed {
            content: "manual content".into(),
            source: SeedSource::Manual,
        };
        let now = Utc::now().to_rfc3339();
        let _bsg = db.lock();
        let tx = _bsg.unchecked_transaction().unwrap();
        apply_preview_in_tx(&tx, batch_id, &[c0, c1, c2], &seed, &now).unwrap();
        tx.commit().unwrap();
        drop(_bsg);
        let tcs = db.transformation_chapters().list_by_batch(batch_id).unwrap();
        let tc0 = tcs.iter().find(|t| t.chapter_id == c0).unwrap();
        assert_eq!(tc0.status, TransformStatus::Done);
        assert_eq!(tc0.result_content.as_deref(), Some("manual content"));
        assert_eq!(tc0.tokens_in, Some(0));
        assert_eq!(tc0.tokens_out, Some(0));
        assert!(tc0.started_at.is_some());
        assert!(tc0.error.is_none());
        let wrc0 = db.workflow_results().get_content_by_batch_and_chapter(batch_id, c0).unwrap();
        assert_eq!(wrc0.as_deref(), Some("manual content"));
    }

    #[test]
    fn apply_preview_noop_when_preview_is_none_path() {
        // 模拟"create_workflow 不传 preview"的路径:不调 apply_preview_in_tx。
        // 断言:所有 tc 保持 Pending,wrc 全空。
        let db = fresh_db();
        let (tn_id, c0, c1, c2, prompt_id, model_id) = seed_env(&db);
        let batch_id = seed_batch_with_tcs(&db, tn_id, c0, c1, c2, prompt_id, model_id);
        let tcs = db.transformation_chapters().list_by_batch(batch_id).unwrap();
        for t in &tcs {
            assert_eq!(t.status, TransformStatus::Pending);
        }
        for cid in &[c0, c1, c2] {
            let wrc = db.workflow_results().get_content_by_batch_and_chapter(batch_id, *cid).unwrap();
            assert!(wrc.is_none());
        }
    }
}
