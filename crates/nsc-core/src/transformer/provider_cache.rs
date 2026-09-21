//! Per-worker provider cache.
//!
//! `JobQueue` 每个 worker 内部持有一个 `ProviderCache`。worker 收到 job 时按
//! `model_config.id` 查表:
//! - 命中:clone 一份 `Arc<dyn AiProvider>` 出去(避免重建 `reqwest::Client` + 连接池)。
//! - 未命中:用 `provider_factory` 创建,缓存下来,并构造一个大小为 `model_config.concurrency`
//!   的 `Arc<Semaphore>` 配套(per-model 并发上限)。
//!
//! 设计取舍:
//! - `Mutex<HashMap>` 而不是 `DashMap`:worker 数 ≤ 8,竞争不热点,简单优于并发。
//! - key 用 `model_config.id` 而非 `api_key` / `base_url`:这样用户在 UI 改 key 后
//!   旧 worker 仍持有旧 provider(避免运行中替换),新建工作流时新 key 才生效。
//!   这是显式 trade-off —— 改 key 不会热替换,但避免运行中突然 401。
//! - cache 不在 worker 间共享,避免 `Arc<dyn AiProvider>` 跨线程引用计数竞争。
//!   worker 数很少(默认 2),provider 重建代价可控,共享收益低。
//! - 软删 model(`archived=1`)仍可被 worker 命中:`BatchScheduler::create_workflow`
//!   按 id 查出的归档行仍能进入 cache;provider.factory 拿到 `api_key=''` 时
//!   会创建失败(被 OpenAI endpoint 401),让错误以自然的 AI 错误冒出来。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;

use crate::ai::AiProvider;
use crate::error::Result;
use crate::models::ModelConfig;

use super::queue::ProviderFactory;

struct CachedEntry {
    provider: Arc<dyn AiProvider>,
    sem: Arc<Semaphore>,
}

#[derive(Clone)]
pub struct CachedProvider {
    pub provider: Arc<dyn AiProvider>,
    pub sem: Arc<Semaphore>,
}

pub struct ProviderCache {
    factory: ProviderFactory,
    inner: Mutex<HashMap<i64, CachedEntry>>,
}

impl ProviderCache {
    pub fn new(factory: ProviderFactory) -> Self {
        Self { factory, inner: Mutex::new(HashMap::new()) }
    }

    /// 拿一个 `(provider, semaphore)`,cache miss 时通过 `factory` 创建。
    /// `model_config.concurrency <= 0` 时退化为 1(避免 0 死锁)。
    pub fn get_or_create(&self, model_config: &ModelConfig) -> Result<CachedProvider> {
        let key = model_config.id;
        {
            let guard = crate::sync::lock_recover(&self.inner, "provider cache");
            if let Some(entry) = guard.get(&key) {
                return Ok(CachedProvider {
                    provider: entry.provider.clone(),
                    sem: entry.sem.clone(),
                });
            }
        }
        // cache miss —— 不持锁创建(创建可能 await / 慢)。
        let provider: Box<dyn AiProvider> = (self.factory)(model_config);
        let provider: Arc<dyn AiProvider> = Arc::from(provider);
        let permits = model_config.concurrency.max(1) as usize;
        let sem = Arc::new(Semaphore::new(permits));
        let mut guard = crate::sync::lock_recover(&self.inner, "provider cache");
        // double-check:避免并发 miss 时重复创建。
        if let Some(entry) = guard.get(&key) {
            return Ok(CachedProvider {
                provider: entry.provider.clone(),
                sem: entry.sem.clone(),
            });
        }
        guard.insert(key, CachedEntry { provider: provider.clone(), sem: sem.clone() });
        Ok(CachedProvider { provider, sem })
    }

    /// 清空整个 cache。下次出队会重建。用于运行期"换 model"的极端情况,
    /// 目前未挂到 IPC(用户可重启 app 等同于清空)。
    #[allow(dead_code)]
    pub fn clear(&self) {
        crate::sync::lock_recover(&self.inner, "provider cache").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatRequest, ChatResponse};
    use crate::error::Result;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 可计数的假 provider —— 用来观察 factory 被调了几次。
    struct CountingProvider;

    #[async_trait::async_trait]
    impl AiProvider for CountingProvider {
        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse> {
            Ok(ChatResponse { content: "x".into(), tokens_in: None, tokens_out: None })
        }
    }

    fn cfg(id: i64, concurrency: i32) -> ModelConfig {
        ModelConfig {
            id, name: format!("m{id}"), base_url: "http://localhost".into(),
            api_key: "k".into(), model: "m".into(),
            max_tokens: None, max_context: None, temperature: None,
            disable_thinking: false, concurrency, archived: 0,
        }
    }

    /// 造一个带计数的 factory,返回 (factory, 计数句柄)。
    fn counting_factory() -> (ProviderFactory, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let f: ProviderFactory = Arc::new(move |_cfg: &ModelConfig| -> Box<dyn AiProvider> {
            c.fetch_add(1, Ordering::SeqCst);
            Box::new(CountingProvider)
        });
        (f, calls)
    }

    /// 主契约:同一 model id 只创建一次 provider —— 第二次必须命中缓存。
    /// 这条决定 worker 是否会为每个 job 重建 reqwest Client + 连接池。
    #[test]
    fn same_model_id_hits_cache() {
        let (f, calls) = counting_factory();
        let cache = ProviderCache::new(f);
        let a = cache.get_or_create(&cfg(1, 1)).unwrap();
        let b = cache.get_or_create(&cfg(1, 1)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "同一 id 不应重复创建 provider");
        assert!(Arc::ptr_eq(&a.provider, &b.provider), "应复用同一个 provider 实例");
        assert!(Arc::ptr_eq(&a.sem, &b.sem), "应复用同一个信号量");
    }

    /// 不同 model id 各自建一份(互不干扰)。
    #[test]
    fn different_model_ids_get_separate_entries() {
        let (f, calls) = counting_factory();
        let cache = ProviderCache::new(f);
        let a = cache.get_or_create(&cfg(1, 1)).unwrap();
        let b = cache.get_or_create(&cfg(2, 1)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(!Arc::ptr_eq(&a.provider, &b.provider));
        assert!(!Arc::ptr_eq(&a.sem, &b.sem));
    }

    /// 信号量尺寸 = `model_config.concurrency` —— per-model 并发限流的上限。
    #[test]
    fn semaphore_permits_equal_concurrency() {
        let (f, _) = counting_factory();
        let cache = ProviderCache::new(f);
        for n in [1i32, 2, 5, 8] {
            let p = cache.get_or_create(&cfg(n as i64, n)).unwrap();
            assert_eq!(
                p.sem.available_permits(), n as usize,
                "concurrency={n} 时应有 {n} 个 permit"
            );
        }
    }

    /// `concurrency <= 0` 退化为 1 —— 否则 0 permit 会让 job 永久阻塞(死锁)。
    #[test]
    fn non_positive_concurrency_falls_back_to_one_permit() {
        let (f, _) = counting_factory();
        let cache = ProviderCache::new(f);
        for n in [0i32, -1, -100] {
            let p = cache.get_or_create(&cfg(n as i64 - 1000, n)).unwrap();
            assert_eq!(p.sem.available_permits(), 1, "concurrency={n} 应退化为 1");
        }
    }

    /// clear() 后下一次查询会重建(cache 真的被清空,不是 no-op)。
    #[test]
    fn clear_forces_recreation() {
        let (f, calls) = counting_factory();
        let cache = ProviderCache::new(f);
        let first = cache.get_or_create(&cfg(1, 1)).unwrap();
        cache.clear();
        let second = cache.get_or_create(&cfg(1, 1)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "clear 后应重新创建");
        assert!(!Arc::ptr_eq(&first.provider, &second.provider));
    }

    /// 并发首查不重复创建(double-check 生效):多线程同时 miss,只应创建一次。
    /// 这保证并发 worker 不会各自建一份连接池。
    #[test]
    fn concurrent_first_lookup_creates_once() {
        let (f, calls) = counting_factory();
        let cache = Arc::new(ProviderCache::new(f));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let c = cache.clone();
            handles.push(std::thread::spawn(move || {
                c.get_or_create(&cfg(1, 4)).unwrap()
            }));
        }
        let sems: Vec<usize> = handles
            .into_iter()
            .map(|h| h.join().unwrap().sem.available_permits())
            .collect();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "并发 miss 只应创建一次");
        // 所有线程必须拿到同一个信号量(否则限流会被绕过)
        assert!(sems.iter().all(|p| *p == 4), "所有线程应拿到同一个 4-permit 信号量: {sems:?}");
    }

    /// 已归档的 model 仍能被缓存命中(注释里明说的行为):worker 拿归档行不报错,
    /// 让 401 以自然的 AI 错误冒出来,而不是在缓存层静默拒绝。
    #[test]
    fn archived_model_is_still_cacheable() {
        let (f, calls) = counting_factory();
        let cache = ProviderCache::new(f);
        let mut archived = cfg(9, 1);
        archived.archived = 1;
        archived.api_key = String::new();
        let p = cache.get_or_create(&archived).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "归档行也应走 factory 创建");
        assert_eq!(p.sem.available_permits(), 1);
    }

    /// cache 用 id 作 key,因此**先按旧配置创建、后传新配置仍命中旧的** ——
    /// 这是模块头写明的 trade-off(UI 改 key 不热替换,避免运行中突然 401)。
    #[test]
    fn cache_key_is_id_so_config_changes_do_not_hot_swap() {
        let (f, calls) = counting_factory();
        let cache = ProviderCache::new(f);
        let a = cache.get_or_create(&cfg(1, 1)).unwrap();
        let mut changed = cfg(1, 1);
        changed.api_key = "sk-new".into();
        let b = cache.get_or_create(&changed).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "改 key 不应触发重建");
        assert!(Arc::ptr_eq(&a.provider, &b.provider), "仍应复用旧 provider");
    }
}
