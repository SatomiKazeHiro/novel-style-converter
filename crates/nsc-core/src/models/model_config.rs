use serde::{Deserialize, Serialize};

/// 已持久化的 `model_configs` 行。
/// - `id == 0` 表示新建;>0 表示更新。
/// - `archived == 1` 表示软删:`api_key` 已被清空,但行保留(供 `transformation_chapters` /
///   `transformation_novels` 引用时仍能展示历史 model 名 / 端点 / 并发配置)。
/// - `concurrency` 是 per-model 并发上限,worker 端按 `model_config_id` 共享信号量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub max_context: Option<i32>,
    pub temperature: Option<f32>,
    /// 用户主动关闭思考的开关 —— 0 = 模型自决,1 = 主动禁用思考。
    /// 仅对官方支持 reasoning_effort:"none" / toggle 类型模型生效。
    pub disable_thinking: bool,
    pub concurrency: i32,
    pub archived: i32,
}

/// 写入前的 `ModelConfig`,不携带 `id` / `archived`(由数据库默认)。
#[derive(Debug, Clone)]
pub struct NewModelConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub max_context: Option<i32>,
    pub temperature: Option<f32>,
    pub disable_thinking: bool,
    pub concurrency: i32,
}
