use serde::{Deserialize, Serialize};

/// `compress` = 内容压缩,`style` = 文风转换。`transformation_chapters.mode` /
/// `compress` = 内容压缩,`style` = 文风转换。`transformation_chapters.mode` 全部用这个 enum —— 历史曾与 `TransformMode` 是同一个语义的两次定义,合并到此处统一。
/// —— 历史上与 `TransformMode` 是同一个语义的两次定义,合并到此处统一。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind { Compress, Style }

/// 已持久化的 `prompts` 行。
/// - `id == 0` 表示新建;>0 表示更新。
/// - `archived == 1` 表示软删:行保留(供 `transformation_chapters.prompt_id` 引用解析
///   仍能拿到历史 prompt name / kind / template),但默认 `list_prompts` 不返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: i64,
    pub name: String,
    pub kind: PromptKind,
    pub template: String,
    pub is_builtin: bool,
    pub archived: i64,
}
