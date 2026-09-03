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
            let guard = self.inner.lock().expect("provider cache lock");
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
        let mut guard = self.inner.lock().expect("provider cache lock");
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
        self.inner.lock().expect("provider cache lock").clear();
    }
}
