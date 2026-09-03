use std::collections::HashSet;
use std::result::Result as StdResult;
use std::sync::{Arc, Barrier};

use tokio::sync::{mpsc, Mutex};

use crate::ai::AiProvider;
use crate::db::Db;
use crate::error::Result;
use crate::models::{ModelConfig, TransformationNovel, TransformStatus};
use crate::recorder::AiCallRecorder;
use crate::transformer::{
    DefaultTransformer, JobInfo, JobSpec, JobStatus, QueueSnapshot,
    ProviderCache, TransformRequest, Transformer,
};

use super::job::SharedQueue;

pub type DbFactory = Arc<dyn Fn() -> Result<Arc<Db>> + Send + Sync>;
pub type ProviderFactory = Arc<dyn Fn(&ModelConfig) -> Box<dyn AiProvider> + Send + Sync>;
/// 队列状态变更回调。`(tid, success, error, content)`:
/// - `enqueue` → `(tid, false, None, "")`
/// - Done → `(tid, true, None, <正文>)`
/// - Failed (含 prep 失败) → `(tid, false, Some(err), "")`
///
/// 闭包在 worker 线程上执行 —— 不要在闭包里做重活或再次阻塞。
pub type Notifier = Arc<dyn Fn(i64, bool, Option<String>, String) + Send + Sync>;

type NotifySlot = Arc<std::sync::Mutex<Option<Notifier>>>;

/// Pending notifier 闭包 + 上下文。worker 在每次 `run_job` 后 drain 这些 envelope,
/// 代替直接调用 —— 这样 `fire → cb → enqueue → fire → ...` 的递归链被切断,
/// 栈深度始终 = 1,SkipFailed 大批失败也不会栈溢出。
struct CallbackEnvelope {
    cb: Notifier,
    tid: i64,
    success: bool,
    error: Option<String>,
    content: String,
}

/// `Vec<CallbackEnvelope>` 由所有 worker 共享。push / drain 短临界区,
/// 不嵌套重入(`queue_callback` 取闭包和 push 之间无 await/重锁)。
type PendingCallbacks = Arc<std::sync::Mutex<Vec<CallbackEnvelope>>>;

pub struct JobQueue {
    tx: mpsc::UnboundedSender<JobSpec>,
    shared: super::job::Shared,
    notify: NotifySlot,
    pending_callbacks: PendingCallbacks,
}

impl JobQueue {
    /// 启动 `workers` 个 tokio current-thread worker,共享一个 mpsc 队列。
    ///
    /// **工厂闭包是 JobQueue 能跨线程工作的核心**(因为 `Db` 不是 `Sync`,
    /// `AiProvider` 不是 `Send` 共享的)。
    /// - `db_factory`:每个 worker 启动时调一次,拿到**独立 owned** `Db`。
    ///   典型实现:`move || Ok(Db::connect(&db_path))`。
    /// - `provider_factory`:每个 job 调一次,基于 `ModelConfig` 生成 owned
    ///   `Box<dyn AiProvider>`。**必须返回 owned**(不能返回 `&'a dyn AiProvider`),
    ///   否则 `Box<dyn Transformer>` 装不下。
    /// - `recorder`:AI 调用日志 recorder —— 共享给所有 worker 的 `DefaultTransformer`;
    ///   transformer 路径(transform_chapter 业务)每次 chat 调用都通过它记账。
    ///   test_model 路径另在 commands 层 record,不走 JobQueue。
    ///
    /// 三个工厂 + recorder 都要求 `Send + Sync + 'static`(recorder 也是 `Arc<dyn ...>`)。
    /// `workers < 1` 会 panic。
    pub fn new<F, P>(
        workers: usize,
        db_factory: F,
        provider_factory: P,
        recorder: Arc<dyn AiCallRecorder>,
        close_thinking: Arc<HashSet<String>>,
    ) -> Self
    where F: Fn() -> Result<Arc<Db>> + Send + Sync + 'static,
        P: Fn(&ModelConfig) -> Box<dyn AiProvider> + Send + Sync + 'static,
    {
        assert!(workers >= 1, "at least 1 worker");
        let (tx, rx) = mpsc::unbounded_channel::<JobSpec>();
        let rx = Arc::new(Mutex::new(rx));
        let shared: super::job::Shared = Arc::new(SharedQueue::default());
        let db_factory: DbFactory = Arc::new(db_factory);
        let provider_factory: ProviderFactory = Arc::new(provider_factory);
        let notify: NotifySlot = Arc::new(std::sync::Mutex::new(None));
        let pending_callbacks: PendingCallbacks = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder: Arc<dyn AiCallRecorder> = recorder;
        let close_thinking: Arc<HashSet<String>> = close_thinking;
        // 屏障同步 worker 与主线程:`JobQueue::new` 返回前确保每个 worker
        // 都已进入 recv 循环,避免 `q.enqueue()` 在 worker 还没 ready 时就 send,
        // 导致 rx 被 drop → SendError。失败路径(runtime 构建失败 / db_factory
        // 返回 Err)也 wait,保证主线程不死锁。
        let ready = Arc::new(Barrier::new(workers + 1));

        // 每个 worker 独立的 provider cache —— 避免 provider 句柄跨线程引用计数竞争。
        for _ in 0..workers {
            let shared = shared.clone();
            let db_factory = db_factory.clone();
            let provider_factory = provider_factory.clone();
            let rx = rx.clone();
            let notify = notify.clone();
            let ready = ready.clone();
            let recorder = recorder.clone();
            let close_thinking = close_thinking.clone();
            let pending_callbacks = pending_callbacks.clone();
            // 每个 worker 内部独立的 provider cache —— 见 provider_cache.rs。
            std::thread::spawn(move || {
                // worker-local cache; 生命周期与 worker 线程一致。
                let cache = ProviderCache::new(provider_factory.clone());
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(_) => {
                        ready.wait();
                        return;
                    }
                };
                rt.block_on(async move {
                    let mut db = match db_factory() {
                        Ok(d) => d,
                        Err(_) => {
                            ready.wait();
                            return;
                        }
                    };
                    ready.wait();
                    loop {
                        let job = {
                            let mut guard = rx.lock().await;
                            guard.recv().await
                        };
                        let Some(job) = job else { break };
                        // 按 model_config.id 取缓存的 provider + per-model semaphore。
                        // cache miss 时通过 provider_factory 重建一次,后续 job 直接命中。
                        let cached = cache.get_or_create(&job.model_config)
                            .expect("provider cache get_or_create");
                        db = run_job(shared.clone(), db, cached.provider, cached.sem, job, notify.clone(), pending_callbacks.clone(), recorder.clone(), close_thinking.clone()).await;
                        // drain pending notifier callbacks(锁内 swap,锁外 invoke)
                        // 切断 `fire → cb → enqueue → fire → ...` 的同步递归链 —— 栈深度恒为 1。
                        let drained: Vec<CallbackEnvelope> = {
                            let mut g = pending_callbacks.lock().expect("callbacks lock");
                            std::mem::take(&mut *g)
                        };
                        for env in drained {
                            (env.cb)(env.tid, env.success, env.error, env.content);
                        }
                    }
                });
            });
        }
        ready.wait();
        Self { tx, shared, notify, pending_callbacks }
    }

    /// 注册队列变更回调。每次 `enqueue` / job 状态转换(Running / Done / Failed)末尾触发。
    /// 闭包在 worker 线程上执行 —— 不要在闭包里做重活或再次阻塞。
    pub fn set_notifier(&self, notifier: Notifier) {
        *self.notify.lock().expect("notify lock") = Some(notifier);
    }

    /// 入队一个 notifier 回调(不立即执行)。
    /// worker loop 在 `run_job` 之后 drain 这些 envelope 并执行,
    /// 这样 `fire → cb → enqueue → fire → ...` 的同步递归链被切断,
    /// 栈深度始终 = 1 —— SkipFailed 大批失败也不会爆栈。
    /// **必须先克隆闭包出锁**,再 push 进 `callbacks`(`std::sync::Mutex` 不可重入)。
    fn queue_callback(
        notify: &NotifySlot,
        callbacks: &std::sync::Mutex<Vec<CallbackEnvelope>>,
        tid: i64,
        success: bool,
        error: Option<String>,
        content: String,
    ) {
        let cb = notify
            .lock()
            .expect("notify lock")
            .as_ref()
            .cloned();
        if let Some(cb) = cb {
            let mut g = callbacks.lock().expect("callbacks lock");
            g.push(CallbackEnvelope { cb, tid, success, error, content });
        }
    }

    /// 入队一个 transform job。返回传入的 `JobSpec.transformation_id`(方便 caller 记录)。
    /// 内部通过 unbounded mpsc 派发给 worker;调用方需保证 `JobSpec` 字段齐全
    /// (job 字段由 `transformation_chapters` 行反查得到,通常在 command 层组装)。
    pub fn enqueue(&self, job: JobSpec) -> i64 {
        let id = job.tc_id;
        self.tx.send(job).expect("queue alive");
        Self::queue_callback(&self.notify, &self.pending_callbacks, id, false, None, String::new());
        id
    }

    /// 拉当前队列快照(pending / running / done / failed 四组)。
    /// 内部锁争用时返回空 snapshot,不阻塞 caller —— 用于前端 UI 1s 轮询。
    pub fn snapshot(&self) -> QueueSnapshot {
        self.shared.inner.try_lock().map(|m| m.clone()).unwrap_or_default()
    }
}

pub struct Prep {
    pub transformation_novel: TransformationNovel,
    pub chapter: crate::models::Chapter,
    pub chapter_content: String,
    pub prev_orig: Vec<(String, String)>,
    /// 邻章已转换正文 (title, content) 对 —— 真内容在 workflow_result_chapters,
    /// 不再是 tc 行(§3.3)。
    pub prev_tx: Vec<(String, String)>,
    pub next_orig: Vec<(String, String)>,
}

struct Final {
    chapter_title: String,
    chapter_idx: i32,
    db_write: DbWrite,
    /// worker 写出的正文 —— 成功路径带正文,失败路径留空。
    /// 仅用于通过 notifier 透传给 `BatchScheduler::on_chapter_done`,
    /// 写 `workflow_result_chapters.content` 槽;
    /// `transformation_chapters.result_content` 不再写(spec §5.x 收口到结果集)。
    content: String,
}

enum DbWrite {
    Done { tokens_in: i32, tokens_out: i32 },
    Failed { err: String },
}

// run_job 是异步 worker 的执行入口,9 个参数全是必需依赖:
// shared / db / ai / sem / callbacks / recorder / close_thinking 是 worker 共享状态,
// job 是任务负载,notify 是结果回流通道。这些参数的共同生命周期 = 单个 worker,
// 打包成 ctx struct 会增加一层间接而无收益,故允许此 lint。
#[allow(clippy::too_many_arguments)]
async fn run_job(
    shared: super::job::Shared,
    db: Arc<Db>,
    ai: Arc<dyn AiProvider>,
    sem: Arc<tokio::sync::Semaphore>,
    job: JobSpec,
    notify: NotifySlot,
    callbacks: PendingCallbacks,
    recorder: Arc<dyn AiCallRecorder>,
    close_thinking: Arc<HashSet<String>>,
) -> Arc<Db> {
    let tid = job.tc_id;
    let chapter_title = job.chapter.title.clone();
    let chapter_idx = job.chapter.idx;

    let prep: StdResult<Prep, String> = read_context(&db, &job);
    let prep = match prep {
        Ok(p) => p,
        Err(err) => {
            let _ = db.transformation_chapters().mark_failed(tid, err.clone());
            push_failed(&shared, tid, job.tn_id, String::new(), 0, err.clone()).await;
            JobQueue::queue_callback(&notify, &callbacks, tid, false, Some(err), String::new());
            return db;
        }
    };

    let _ = db.transformation_chapters().mark_running(tid);

    let req = TransformRequest {
        transformation_id: job.tn_id,
        chapter: prep.chapter,
        chapter_content: prep.chapter_content,
        novel_context: crate::transformer::TransformationNovelContext {
            transformation_novel: prep.transformation_novel,
            prev_original: prep.prev_orig,
            prev_transformed: prep.prev_tx,
            next_original: prep.next_orig,
        },
        prompt: job.prompt.clone(),
        model_config: job.model_config.clone(),
        custom_input: None,
        preview_id: None,
    };
    // per-model 并发限流:同一 model 的多个 job 共享一个 semaphore,
    // 超过 `model_config.concurrency` 时本 job 在 await 处排队,permit drop 时自动释放。
    let _permit = sem.acquire().await.expect("semaphore closed");
    let tx: Box<dyn Transformer> = Box::new(DefaultTransformer::new(ai.clone(), recorder.clone(), close_thinking.clone()));
    let ai_result = tx.transform(req).await;

    let final_state: Final = apply_result(&db, tid, chapter_title, chapter_idx, ai_result);

    match final_state.db_write {
        DbWrite::Done { tokens_in, tokens_out } => {
            push_running(
                &shared, tid, job.tn_id,
                final_state.chapter_title.clone(),
                final_state.chapter_idx,
            ).await;
            push_done(
                &shared, tid, job.tn_id,
                final_state.chapter_title,
                final_state.chapter_idx,
                tokens_in, tokens_out,
            ).await;
            JobQueue::queue_callback(&notify, &callbacks, tid, true, None, final_state.content);
        }
        DbWrite::Failed { err } => {
            push_failed(
                &shared, tid, job.tn_id,
                final_state.chapter_title,
                final_state.chapter_idx,
                err.clone(),
            ).await;
            JobQueue::queue_callback(&notify, &callbacks, tid, false, Some(err), String::new());
        }
    }

    db
}

/// 同步读所有 job 上下文:从 uploads.original_text 切片 chapter / 邻章正文。
/// 通过 tid 反查 transformation_novel_id(避免 caller 多传字段)。
pub fn read_context(db: &Arc<Db>, job: &JobSpec) -> StdResult<Prep, String> {
    let cid = job.chapter.id;
    let idx = job.chapter.idx;
    let data_asset_id = job.chapter.data_asset_id;

    let chapter = db.chapters().get(cid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "chapter missing".to_string())?;
    let chapter_content = chapter.body.clone();

    let prev_chapters = db
        .chapters()
        .prev_n(data_asset_id, idx, job.ctx_prev_original)
        .map_err(|e| e.to_string())?;
    let next_chapters = db
        .chapters()
        .next_n(data_asset_id, idx, job.ctx_next_original)
        .map_err(|e| e.to_string())?;

    let prev_orig: Vec<(String, String)> = prev_chapters
        .into_iter()
        .map(|c| (c.title, c.body.clone()))
        .collect();
    let next_orig: Vec<(String, String)> = next_chapters
        .into_iter()
        .map(|c| (c.title, c.body.clone()))
        .collect();

    let prev_tx: Vec<(String, String)> = {
        let mut out = Vec::new();
        let take = job.ctx_prev_transformed.max(0) as usize;
        if take > 0 {
            let chs = db.chapters().prev_n(data_asset_id, idx, take as i32)
                .map_err(|e| e.to_string())?;
            for ch in chs.iter() {
                let list = db.transformation_chapters().list_by_chapter(ch.id)
                    .map_err(|e| e.to_string())?;
                if let Some(t) = list.into_iter().find(|t| {
                    t.transformation_novel_id == job.tn_id
                        && t.prompt_id == job.prompt.id
                        && t.model_config_id == job.model_config.id
                        && matches!(t.status, TransformStatus::Done)
                }) {
                    // 真内容在 workflow_result_chapters.content,不是 tc.result_content。
                    let content = match t.batch_id {
                        Some(bid) => db.workflow_results()
                            .get_content_by_batch_and_chapter(bid, ch.id)
                            .map_err(|e| e.to_string())?,
                        None => None,
                    };
                    if let Some(c) = content {
                        out.push((ch.title.clone(), c));
                    }
                }
            }
        }
        out
    };

    let _ = idx;
    Ok(Prep {
        transformation_novel: db.transformation_novels().get(job.tn_id).map_err(|e| e.to_string())?
            .ok_or_else(|| "tn missing".to_string())?,
        chapter,
        chapter_content,
        prev_orig,
        prev_tx,
        next_orig,
    })
}

fn apply_result(
    db: &Arc<Db>,
    tid: i64,
    chapter_title: String,
    chapter_idx: i32,
    ai_result: Result<crate::transformer::TransformOutcome>,
) -> Final {
    match ai_result {
        Ok(out) => {
            // `tc.result_content` 不再写(spec §5.x 收口到结果集);正文走 `Final.content`
            // → notifier → `BatchScheduler::on_chapter_done` → `workflow_result_chapters.content`。
            let _ = db.transformation_chapters().mark_done(
                tid, String::new(), out.tokens_in, out.tokens_out,
            );
            Final {
                chapter_title, chapter_idx,
                db_write: DbWrite::Done {
                    tokens_in: out.tokens_in,
                    tokens_out: out.tokens_out,
                },
                content: out.result_content,
            }
        }
        Err(e) => {
            let err_str = e.to_string();
            let _ = db.transformation_chapters().mark_failed(tid, err_str.clone());
            Final {
                chapter_title, chapter_idx,
                db_write: DbWrite::Failed { err: err_str },
                content: String::new(),
            }
        }
    }
}

async fn push_running(
    shared: &super::job::Shared,
    tid: i64,
    tn_id: i64,
    chapter_title: String,
    chapter_idx: i32,
) {
    let mut s = shared.inner.lock().await;
    s.running.push(JobInfo {
        tc_id: tid, tn_id,
        chapter_title, chapter_idx,
        status: JobStatus::Running,
        error: None, tokens_in: None, tokens_out: None,
    });
}

async fn push_done(
    shared: &super::job::Shared,
    tid: i64,
    tn_id: i64,
    chapter_title: String,
    chapter_idx: i32,
    tokens_in: i32, tokens_out: i32,
) {
    let mut s = shared.inner.lock().await;
    s.done.push(JobInfo {
        tc_id: tid, tn_id,
        chapter_title, chapter_idx,
        status: JobStatus::Done,
        error: None, tokens_in: Some(tokens_in), tokens_out: Some(tokens_out),
    });
}

async fn push_failed(
    shared: &super::job::Shared,
    tid: i64,
    tn_id: i64,
    chapter_title: String,
    chapter_idx: i32,
    err: String,
) {
    let mut s = shared.inner.lock().await;
    s.failed.push(JobInfo {
        tc_id: tid, tn_id,
        chapter_title, chapter_idx,
        status: JobStatus::Failed,
        error: Some(err),
        tokens_in: None, tokens_out: None,
    });
}
