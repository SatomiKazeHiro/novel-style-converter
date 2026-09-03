pub mod batch_scheduler;
pub mod job;
pub mod provider_cache;
pub mod queue;
// transformer 模块与父模块同名,是有意的领域命名:DefaultTransformer 实现位于
// transformer/transformer.rs,与 batch_scheduler / queue / job 平级;
// rust 生态类似命名(如 tokio runtime)也保留此风格。
#[allow(clippy::module_inception)]
pub mod transformer;

pub use batch_scheduler::{BatchScheduler, WorkflowCreate};
pub use job::{JobInfo, JobSpec, JobStatus, QueueSnapshot};
pub use provider_cache::{CachedProvider, ProviderCache};
pub use queue::{DbFactory, JobQueue, Notifier, ProviderFactory};
pub use transformer::{
    DefaultTransformer, TransformOutcome, TransformRequest,
    TransformationNovelContext, Transformer,
};
