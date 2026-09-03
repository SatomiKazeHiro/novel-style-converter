//! Prompt 模板渲染:
//! - 占位符语法:`{{var}}`(双花括号),与 builtin 一致(spec § 模板格式)。
//! - 可用变量:`chapter_title / chapter_content / prev_original / prev_transformed /
//!   next_original / novel_title`。
//! - system/user 分离:模板里以独占一行的 `---` 单行分隔标记切两段;
//!   标记前的内容(若有)作为 system 消息,标记后的内容作为 user 消息;
//!   没有标记则整段作为 user 消息(向后兼容旧模板)。
//! - 渲染单次扫描:不调 7 次 String::replace,改用单次字节扫 + 字符串拼(性能 §3.1)。
//! - `prev_transformed` 接受 `&[(String, String)]`(title, content)对 —— 调用方(queue.rs)
//!   负责从 workflow_result_chapters 拿真内容(§3.3:transformation_chapters.result_content
//!   在新设计下永远是 NULL,不能再用 tc 行做内容来源)。

use crate::models::{Chapter, PromptKind, TransformationChapter, TransformationNovel};

pub struct PromptContext<'a> {
    pub transformation_novel: &'a TransformationNovel,
    pub chapter: &'a Chapter,
    /// 章节正文切片(由 `queue.rs` 从 `chapters.body` 取出)。
    pub chapter_content: &'a str,
    /// 邻章原文片段 —— Vec 元素是 `(title, content)` 对。
    pub prev_original: &'a [(String, String)],
    /// 邻章已转换正文 —— Vec 元素是 `(title, content)` 对;queue.rs 负责 join
    /// workflow_result_chapters 拿真内容。
    pub prev_transformed: &'a [(String, String)],
    pub next_original: &'a [(String, String)],
    /// `prompt.kind` 传给 render,用于 filter 上下文(预留,目前仅做占位)。
    pub kind: PromptKind,
}

/// `render` 输出:system 段(可选) + user 段。
/// - `system.is_some()`:模板里出现了独占一行的 `---` 标记,标记前内容作 system 消息。
/// - `system.is_none()`:模板整段作为 user 消息。
#[derive(Debug, Clone)]
pub struct RenderedPrompt {
    pub system: Option<String>,
    pub user: String,
}

fn join_chapter_pairs(parts: &[(String, String)], sep: &str) -> String {
    parts
        .iter()
        .map(|(title, content)| format!("{title}\n{content}"))
        .collect::<Vec<_>>()
        .join(sep)
}

/// 单次扫描渲染:遍历 template,遇到 `{{name}}` 查表替换。
/// 不再走 7 次 `String::replace`(章节大时显著慢)。
fn fill_template(template: &str, vars: &[(&str, &str)]) -> String {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len() + 64);
    let mut i = 0;
    while i < bytes.len() {
        if let Some(rel) = template[i..].find("{{") {
            let abs = i + rel;
            if let Some(close_rel) = template[abs + 2..].find("}}") {
                let close = abs + 2 + close_rel;
                let name = &template[abs + 2..close];
                out.push_str(&template[i..abs]);
                if let Some((_, val)) = vars.iter().find(|(k, _)| *k == name) {
                    out.push_str(val);
                } else {
                    // 未知占位符 —— 原样保留(便于排查缺失变量名)
                    out.push_str(&template[abs..close + 2]);
                }
                i = close + 2;
                continue;
            }
        }
        out.push_str(&template[i..]);
        break;
    }
    out
}

/// 切模板为 system / user 两段。
/// 触发条件:任意一行内容是 `---`(首尾允许空白)即切;切点前作 system 候选,后作 user。
/// 无标记则 user = 整段,system = None。
fn split_system_user(template: &str) -> (Option<String>, String) {
    let lines: Vec<&str> = template.split('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        if line.trim() == "---" {
            let system_part: String = lines[..idx].join("\n");
            let user_part: String = lines[idx + 1..].join("\n");
            let system = if system_part.trim().is_empty() { None } else { Some(system_part) };
            return (system, user_part);
        }
    }
    (None, template.to_string())
}

pub fn render(template: &str, ctx: &PromptContext<'_>) -> RenderedPrompt {
    let (system_raw, user_raw) = split_system_user(template);
    let prev_o = join_chapter_pairs(ctx.prev_original, "\n\n");
    let next_o = join_chapter_pairs(ctx.next_original, "\n\n");
    let prev_t = join_chapter_pairs(ctx.prev_transformed, "\n\n");
    let vars: [(&str, &str); 6] = [
        ("chapter_title", ctx.chapter.title.as_str()),
        ("chapter_content", ctx.chapter_content),
        ("prev_original", prev_o.as_str()),
        ("next_original", next_o.as_str()),
        ("prev_transformed", prev_t.as_str()),
        ("novel_title", ctx.transformation_novel.title.as_str()),
    ];
    RenderedPrompt {
        system: system_raw.as_deref().map(|s| fill_template(s, &vars)),
        user: fill_template(&user_raw, &vars),
    }
}

// `TransformationChapter` 仍被 PromptContext 不直接引用,留此 use 让类型在 API 中保持可见
// 便于未来在 PromptContext 上加 prev_transformed 索引元数据(比如 tc.id 用于追溯)。
#[allow(dead_code)]
pub fn _type_marker(_t: &TransformationChapter) {}
