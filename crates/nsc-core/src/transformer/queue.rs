use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::result::Result as StdResult;
use std::sync::{Arc, Barrier};

use futures_util::FutureExt;
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
    /// **工厂闭包是 JobQueue 能跨线程工作的核心**(`AiProvider` 不共享单实例)。
    /// - `db_factory`:每个 worker 启动时调一次。典型实现是克隆同一个 `Arc<Db>`
    ///   (`move || Ok(db.clone())`)—— `Db` 内部是 `Mutex<Connection>`,多线程共享
    ///   是设计意图(见 db/pool.rs 顶部注释)。返回值是 `Arc<Db>` 而不是 owned `Db`。
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
                    Err(e) => {
                        // 不能静默:这个 worker 从此不会消费任何 job,若不上报,
                        // UI 会停在 pending 而无人知道为什么。
                        eprintln!("[queue] worker runtime 构建失败,该 worker 不参与消费: {e}");
                        ready.wait();
                        return;
                    }
                };
                rt.block_on(async move {
                    let db = match db_factory() {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("[queue] db_factory 失败,该 worker 不参与消费: {e}");
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
                        // ── per-job panic 边界 ──
                        // 单个章节的 panic 不能带走整个 worker:worker 死了之后
                        // channel 里后续 job 无人消费,UI 停在 pending 且没有任何提示。
                        // 缓存/信号量/Db 都是共享的,pinned 在闭包外 —— 即使内部 panic,
                        // 它们仍然有效,循环可以继续消费下一个 job。
                        // 注:必须用 futures_util::FutureExt::catch_unwind(async 块不能用
                        // std::panic::catch_unwind 包:panic 是在 poll 时从 future 内部
                        // unwind 出来的)。
                        let tid = job.tc_id;
                        let tn_id = job.tn_id;
                        // 供 panic 边界使用 —— job 被 run_job 消费后就取不到了。
                        let chapter_title = job.chapter.title.clone();
                        let chapter_idx = job.chapter.idx;
                        let drained = {
                            let work = run_job(shared.clone(), db.clone(), cached.provider, cached.sem, job, notify.clone(), pending_callbacks.clone(), recorder.clone(), close_thinking.clone());
                            let result = AssertUnwindSafe(work).catch_unwind().await;
                            if let Err(payload) = result {
                                let msg = crate::sync::panic_message(&payload);
                                eprintln!("[queue] job tc={tid} 线程内 panic,已隔离并继续消费: {msg}");
                                // 尽力把该章标失败,避免永久卡在 running。
                                // 失败也不致命 —— 至少 panic 已被记录且 worker 活着。
                                let _ = db.transformation_chapters()
                                    .mark_failed(tid, format!("内部错误(panic): {msg}"));
                                // 同步进队列快照:UI 轮询的是快照而非 DB,
                                // 少了这一步该章会从 UI 上凭空消失(done/failed 都没有它)。
                                push_failed(&shared, tid, tn_id, chapter_title, chapter_idx,
                                    format!("内部错误(panic): {msg}")).await;
                            }
                            // drain pending notifier callbacks(锁内 swap,锁外 invoke)
                            // 切断 `fire → cb → enqueue → fire → ...` 的同步递归链 —— 栈深度恒为 1。
                            let mut g = crate::sync::lock_recover(&pending_callbacks, "callbacks");
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
        *crate::sync::lock_recover(&self.notify, "notify") = Some(notifier);
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
        let cb = crate::sync::lock_recover(notify, "notify")
            .as_ref()
            .cloned();
        if let Some(cb) = cb {
            let mut g = crate::sync::lock_recover(callbacks, "callbacks");
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
    /// 内部锁争用时返回空 snapshot,不阻塞 caller。
    ///
    /// 注意:`running` 列表在 job 终结后**不会**被清理(push_done / push_failed 只往
    /// 对应组追加),所以同一 tc 可能同时出现在 running 与 done/failed 里 —— 它表达的是
    /// "曾经进入过 running",不是"此刻仍在 running"。原消费方 IPC `get_queue_snapshot`
    /// 已删除(前端从未调用),现在只有测试在读;若要重新暴露给 UI,需要先修这个语义。
    pub fn snapshot(&self) -> QueueSnapshot {
        self.shared.inner.try_lock().map(|m| m.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatRequest, ChatResponse};
    use crate::models::batch::{NewBatch, OnFailurePolicy};
    use crate::models::prompt::PromptKind;
    use crate::models::{
        Chapter, NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
        NewTransformationNovel, NewUpload, Prompt, TransformStatus,
    };
    use crate::recorder::NoopRecorder;

    /// 立即返回固定正文的 provider(worker 会真跑完整条链路)。
    struct InstantProvider;

    #[async_trait::async_trait]
    impl AiProvider for InstantProvider {
        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse> {
            Ok(ChatResponse {
                content: "转换后正文".into(),
                tokens_in: Some(3),
                tokens_out: Some(4),
            })
        }
    }

    /// 每次都失败的 provider —— 用来驱动 failed 回调。
    struct FailingProvider;

    #[async_trait::async_trait]
    impl AiProvider for FailingProvider {
        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse> {
            Err(crate::error::Error::Ai("模拟 provider 失败".into()))
        }
    }

    fn seed(db: &Db) -> (i64, i64, i64, i64) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "q1".into(), filename: "f.txt".into(), byte_size: 1,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 2,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(), note: String::new(),
        }).unwrap();
        let prompt_id = db.prompts().insert(&Prompt {
            id: 0, name: "p".into(), kind: PromptKind::Compress,
            template: "{{chapter_content}}".into(), is_builtin: false, archived: 0,
        }).unwrap();
        let model_id = db.model_configs().insert(&NewModelConfig {
            name: "m".into(), base_url: "http://localhost".into(), api_key: "k".into(),
            model: "m".into(), max_tokens: None, max_context: None, temperature: None,
            disable_thinking: false, concurrency: 1,
        }).unwrap();
        let batch_id = db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id, label: None,
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id, model_config_id: model_id, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap();
        let cid = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: 1, title: "c1".into(),
            body: "正文一".into(), word_count: 3, ..Default::default()
        }).unwrap();
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id, chapter_id: cid,
            mode: PromptKind::Compress, prompt_id, model_config_id: model_id,
            ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
            batch_id: Some(batch_id), style_ref_chapter_id: None,
        }).unwrap();
        let tc_id = db.transformation_chapters().list_by_chapter(cid).unwrap()[0].id;
        (tc_id, cid, prompt_id, model_id)
    }

    fn job(db: &Db, tc_id: i64, cid: i64, prompt_id: i64, model_id: i64) -> JobSpec {
        // 分语句取数:repo guard 不可重入,别把两个 db.xxx() 嵌在同一个表达式里。
        let chapter: Chapter = db.chapters().get(cid).unwrap().unwrap();
        let batch_id = db.transformation_chapters()
            .get(tc_id).unwrap().unwrap().batch_id.unwrap();
        let tn_id = db.batches().get(batch_id).unwrap().unwrap().transformation_novel_id;
        let prompt = db.prompts().get(prompt_id).unwrap().unwrap();
        let model_config = db.model_configs().get(model_id).unwrap().unwrap();
        JobSpec {
            tc_id, tn_id, mode: PromptKind::Compress, chapter,
            prompt, model_config,
            ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        }
    }

    /// 构建队列;`failing=true` 时 provider 永远失败。
    fn queue(db: &Arc<Db>, failing: bool) -> JobQueue {
        let dbw = db.clone();
        JobQueue::new(
            1,
            move || Ok(dbw.clone()),
            move |_cfg: &ModelConfig| -> Box<dyn AiProvider> {
                if failing { Box::new(FailingProvider) } else { Box::new(InstantProvider) }
            },
            Arc::new(NoopRecorder),
            Arc::new(HashSet::new()),
        )
    }

    type Calls = Arc<std::sync::Mutex<Vec<(i64, bool, Option<String>, String)>>>;

    /// 通过**公开接口**注册记录型 notifier —— 回调在 worker 线程 drain 时执行,
    /// 所以断言前必须用 `wait_until` 等它落地(不是同步的)。
    fn record(q: &JobQueue, calls: Calls) {
        q.set_notifier(Arc::new(move |tid, success, error, content| {
            calls.lock().unwrap().push((tid, success, error, content));
        }));
    }

    /// 等谓词成真(worker 是线程池,完成时间不确定),超时即失败并回报实际内容。
    fn wait_until<F: Fn() -> bool>(f: F, what: &str) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if f() { return true; }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("等待超时: {what}");
    }

    /// **notifier 契约**:enqueue 立即回调一次 `(tid, false, None, "")` ——
    /// 这是"已入队"信号(UI 据此把行标成 pending),不是失败。
    /// 注意 worker 还没跑,所以此刻不该出现 success 回调。
    #[test]
    fn enqueue_fires_queued_notification_immediately() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let (tc_id, cid, pid, mid) = seed(&db);
        let q = queue(&db, false);
        let calls: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        record(&q, calls.clone());

        let returned = q.enqueue(job(&db, tc_id, cid, pid, mid));
        assert_eq!(returned, tc_id, "enqueue 应回传 tc_id 供 caller 记录");

        // 入队通知由 worker 在 drain 时执行 —— 异步,需等
        let c = calls.clone();
        wait_until(|| !c.lock().unwrap().is_empty(), "入队通知");
        let got = calls.lock().unwrap().clone();
        let queued = got.first().expect("应收到入队通知");
        assert_eq!(queued.0, tc_id);
        assert!(!queued.1, "入队通知的 success 应为 false");
        assert!(queued.2.is_none(), "入队通知不该带 error(它不是失败)");
        assert_eq!(queued.3, "", "入队通知不该带正文");
    }

    /// 成功路径:job 跑完后回调一次 `(tid, true, None, <正文>)`。
    #[test]
    fn successful_job_fires_done_notification_with_content() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let (tc_id, cid, pid, mid) = seed(&db);
        let q = queue(&db, false);
        let calls: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        record(&q, calls.clone());
        q.enqueue(job(&db, tc_id, cid, pid, mid));

        let c = calls.clone();
        wait_until(|| c.lock().unwrap().iter().any(|r| r.1), "成功回调");

        let got = calls.lock().unwrap().clone();
        let done = got.iter().find(|r| r.1).expect("应有 success 回调");
        assert_eq!(done.0, tc_id);
        assert_eq!(done.3, "转换后正文", "成功回调应带上模型产出的正文");
        assert!(done.2.is_none());
    }

    /// 失败路径:回调 `(tid, false, Some(err), "")` —— 与"入队通知"的差别在 error 非空。
    #[test]
    fn failed_job_fires_failure_notification_with_error() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let (tc_id, cid, pid, mid) = seed(&db);
        let q = queue(&db, true);
        let calls: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        record(&q, calls.clone());
        q.enqueue(job(&db, tc_id, cid, pid, mid));

        let c = calls.clone();
        wait_until(
            || c.lock().unwrap().iter().any(|r| !r.1 && r.2.is_some()),
            "失败回调",
        );

        let got = calls.lock().unwrap().clone();
        let fail = got.iter().find(|r| !r.1 && r.2.is_some()).expect("应有失败回调");
        assert_eq!(fail.0, tc_id);
        assert!(fail.2.as_deref().unwrap().contains("模拟 provider 失败"),
            "失败回调应带上真实错误: {fail:?}");
        assert_eq!(fail.3, "");

        // 该章应被标 failed 且 worker 仍活着(不是 panic 路径)
        let tc = db.transformation_chapters().get(tc_id).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Failed);
    }

    /// 没注册 notifier 时 enqueue 不应 panic(回调是可选的)。
    #[test]
    fn enqueue_without_notifier_is_safe() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let (tc_id, cid, pid, mid) = seed(&db);
        let q = queue(&db, false);
        assert_eq!(q.enqueue(job(&db, tc_id, cid, pid, mid)), tc_id);
    }

    /// 重复注册 notifier:以后者为准(旧的不再被调用)。
    #[test]
    fn set_notifier_replaces_previous() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let (tc_id, cid, pid, mid) = seed(&db);
        let q = queue(&db, false);
        let first: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let second: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        record(&q, first.clone());
        record(&q, second.clone());

        q.enqueue(job(&db, tc_id, cid, pid, mid));

        let s = second.clone();
        wait_until(|| !s.lock().unwrap().is_empty(), "新 notifier 的回调");
        assert!(first.lock().unwrap().is_empty(), "被替换的 notifier 不该再收到回调");
    }

    /// 空队列的 snapshot 四组皆空(不 panic、不阻塞)。
    #[test]
    fn snapshot_of_empty_queue_is_empty() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let q = queue(&db, false);
        let s = q.snapshot();
        assert!(s.pending.is_empty() && s.running.is_empty()
            && s.done.is_empty() && s.failed.is_empty());
    }

    /// snapshot 记录 job 的终结状态:成功进 done、失败进 failed,且都带 tokens 字段。
    #[test]
    fn snapshot_tracks_terminal_states() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let (tc_id, cid, pid, mid) = seed(&db);
        let q = queue(&db, false);
        q.enqueue(job(&db, tc_id, cid, pid, mid));

        let shared = q.shared.clone();
        wait_until(
            || shared.inner.try_lock().map(|m| !m.done.is_empty()).unwrap_or(false),
            "done 快照",
        );
        let s = q.snapshot();
        let d = s.done.iter().find(|j| j.tc_id == tc_id).expect("done 应有该 job");
        assert_eq!(d.status, JobStatus::Done);
        assert_eq!(d.tokens_in, Some(3));
        assert_eq!(d.tokens_out, Some(4));
        assert_eq!(d.chapter_idx, 1);
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
    /// `tokens_*` 为 None = provider 未返回 usage(不是失败)。
    Done { tokens_in: Option<i32>, tokens_out: Option<i32> },
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
) {
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
            return;
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
    tokens_in: Option<i32>, tokens_out: Option<i32>,
) {
    let mut s = shared.inner.lock().await;
    s.done.push(JobInfo {
        tc_id: tid, tn_id,
        chapter_title, chapter_idx,
        status: JobStatus::Done,
        error: None, tokens_in, tokens_out,
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
