/// Prompt 模板可用变量清单。
///
/// **必须与后端 `crates/nsc-core/src/prompts/render.rs::render` 的 vars 表一致** ——
/// 变量名写错不会报错(未知占位符会被原样保留在 prompt 里),所以这份清单 + 点击插入
/// 是用户防拼错的主要手段。改后端变量表时这里要同步。
///
/// `prev_context_budget` / `total_context_budget` 的取值来自 render.rs 的
/// `NEIGHBOR_CHAPTER_BUDGET` / `SLOT_BUDGET` 常量,数值只用于展示。
export interface PromptVariable {
  token: string;
  desc: string;
}

export const PROMPT_VARIABLES: ReadonlyArray<PromptVariable> = [
  { token: '{{chapter_title}}', desc: '当前章节标题' },
  { token: '{{chapter_content}}', desc: '当前章节正文(必填,不截断)' },
  { token: '{{prev_original}}', desc: '上一章原文(邻章,按预算截断)' },
  { token: '{{prev_transformed}}', desc: '上一章已改写正文(风格锚点)' },
  { token: '{{next_original}}', desc: '下一章原文(邻章,按预算截断)' },
  { token: '{{novel_title}}', desc: '转换小说(工程)标题' },
  { token: '{{prev_context_budget}}', desc: '单个邻章片段的字数上限(1500)' },
  { token: '{{total_context_budget}}', desc: '同一占位符下邻章合计上限(4000)' },
];
