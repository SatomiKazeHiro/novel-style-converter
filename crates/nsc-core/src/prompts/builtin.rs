//! 内置 prompt 模板。
//!
//! 模板格式规范:
//! - `---` 独占一行切分 system / user 两段(spec § 模板格式)。
//! - 占位符 `{{var}}` 双花括号;变量集见 `prompts::render`。
//! - 内置模板结尾必须包含硬性输出约束(不要输出章节标题 / 元注释 / "待续" 等),
//!   防止 LLM 在 content 前后贴心地附加结构化前缀,污染 `workflow_result_chapters.content` 存储。
//!
//! 风格(style_default)与压缩(compress_default)都引用 `{{prev_original}}` 作为
//! 风格锚点,避免仅看已改写上文导致风格漂移。

use crate::models::PromptKind;

#[derive(Debug, Clone)]
pub struct BuiltinPrompt {
    pub name: &'static str,
    pub kind: PromptKind,
    pub template: &'static str,
}

pub fn builtin_prompts() -> Vec<BuiltinPrompt> {
    vec![
        BuiltinPrompt {
            name: "compress_default",
            kind: PromptKind::Compress,
            template: COMPRESS_DEFAULT,
        },
        BuiltinPrompt {
            name: "style_default",
            kind: PromptKind::Style,
            template: STYLE_DEFAULT,
        },
    ]
}

const COMPRESS_DEFAULT: &str = "\
你是一名专业的小说编辑,负责将章节压缩为关键情节与人物驱动。
目标:删除冗余的环境描写、心理独白与重复叙述,保留主线。
---
工作要求:
1. 只压缩,不重写风格。
2. 保留所有已出场人物的对话与关键动作。
3. 删除不影响剧情推进的修饰段落。

# 上一章原文
{{prev_original}}

# 上一章改写后(若有)
{{prev_transformed}}

# 下一章原文(若有)
{{next_original}}

# 当前章节
{{chapter_title}}
{{chapter_content}}

硬性输出要求(违反即视为失败):
- 只输出压缩后的章节正文,不要输出章节标题。
- 不要附加任何说明、注释、\"以下是改写\"、\"待续\"、\"(完)\" 等占位符。
- 不要用 markdown 代码块包裹正文。";

const STYLE_DEFAULT: &str = "\
你是一名资深网文风改写编辑,负责将章节改写为更紧凑、更有张力的现代网文风格。
约束:保持人物性格与情节走向不变,只调整表达节奏与文风。
---
工作要求:
1. 短句优先,适当增加动作与对话占比。
2. 段落切分更细,适合移动端阅读。
3. 保持原作的人称与时间线,不增删情节。

# 上一章原文(风格锚点)
{{prev_original}}

# 上一章改写后(若有)
{{prev_transformed}}

# 下一章原文(若有)
{{next_original}}

# 当前章节
{{chapter_title}}
{{chapter_content}}

硬性输出要求(违反即视为失败):
- 只输出改写后的章节正文,不要输出章节标题。
- 不要附加任何说明、注释、\"以下是改写\"、\"待续\"、\"(完)\" 等占位符。
- 不要用 markdown 代码块包裹正文。";
