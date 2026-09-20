//! Prompt 模板渲染:
//! - 占位符语法:`{{var}}`(双花括号),与 builtin 一致(spec § 模板格式)。
//! - 可用变量:`chapter_title / chapter_content / prev_original / prev_transformed /
//!   next_original / novel_title / prev_context_budget / total_context_budget`。
//! - system/user 分离:模板里以独占一行的 `---` 单行分隔标记切两段;
//!   标记前的内容(若有)作为 system 消息,标记后的内容作为 user 消息;
//!   没有标记则整段作为 user 消息(向后兼容旧模板)。
//! - 渲染单次扫描:不调 7 次 String::replace,改用单次字节扫 + 字符串拼(性能 §3.1)。
//! - `prev_transformed` 接受 `&[(String, String)]`(title, content)对 —— 调用方(queue.rs)
//!   负责从 workflow_result_chapters 拿真内容(§3.3:transformation_chapters.result_content
//!   在新设计下永远是 NULL,不能再用 tc 行做内容来源)。
//!
//! ## 邻章上下文预算(为什么在 render 层截断)
//! 邻章原文/改写正文此前是**整章全文**拼进 prompt,长章节直接把输入顶到模型
//! `max_context` 之上,worker 只能整章失败(见 transformer.rs 的 estimated_tokens 护栏)。
//! 截断放在 render 层而不是 queue.rs 读 DB 层,是为了让**预览路径**
//! (batch_scheduler::preview_first_chapter 自己拼 prev/next,不经过 read_context)
//! 落到同一套规则 —— 预览与实际转换看到的上下文必须一致。
//!
//! 规则:每个邻章片段最多 [`NEIGHBOR_CHAPTER_BUDGET`] 字,超出则保留**头尾**并插入
//! [`CUT_MARKER`](头尾都留:开头给场景与人名,结尾给衔接点);同一占位符下所有片段
//! 合计不超过 [`SLOT_BUDGET`] 字。截断处显式写明"因长度省略",避免模型把它当成
//! "这段没写"而在正文里补写。
//!
//! 注意:本预算只作用于邻章上下文,**不截断** `{{chapter_content}}` —— 当前章节是
//! 要处理的正文,截断它会直接损坏输出。

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

/// 单个邻章片段的字数上限(字符数,不是字节)。
/// 1500 字 ≈ 一部网文 1 章的三分之一到一半,足够给到人名/场景/衔接点。
pub const NEIGHBOR_CHAPTER_BUDGET: usize = 1500;

/// 同一占位符下所有邻章片段的合计字数上限。
/// ctx_prev_original=2 时最坏情况 2×1500=3000 < 4000,不会互相挤掉。
pub const SLOT_BUDGET: usize = 4000;

/// 片段中段被省略时的标记。必须写明"因长度省略"——
/// 否则模型会把它读成"原作者这里没写",进而在输出里补写。
const CUT_MARKER: &str = "……(因长度限制,此处省略若干段落)……";

/// 邻章上下文的统一格式:每章带标题 + 行号 + 只读提示。
/// 行号不是装饰 —— 它给模型一个引用"上一章哪一段"的锚点,也让片段边界不含糊。
fn format_context_chapter(title: &str, content: &str) -> String {
    let mut out = String::with_capacity(content.len() + 64);
    out.push_str(&format!(
        "----- 章节:{title}(以下为参考材料,只读,不要输出)-----\n"
    ));
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            out.push('\n');
            continue;
        }
        out.push_str(&(i + 1).to_string());
        out.push_str(" | ");
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// 压缩上下文的空白:去行尾空白 + 连续空行折叠为一行。
/// 只降 token,不丢字符信息;行号仍指向压缩后的行序(对模型是自洽的)。
fn compact_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_blank = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        let blank = trimmed.is_empty();
        if blank {
            if prev_blank {
                continue;
            }
            out.push('\n');
            prev_blank = true;
            continue;
        }
        out.push_str(trimmed);
        out.push('\n');
        prev_blank = false;
    }
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// 只留头尾的截断:开头给场景/人名,结尾给衔接点。
fn cut_middle(content: &str, budget: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    if chars.len() <= budget {
        return content.to_string();
    }
    if budget <= CUT_MARKER.chars().count() {
        // 预算连标记都放不下:退化为纯头部截断,不再插标记(否则标记比正文还长)。
        return chars[..budget].iter().collect();
    }
    let keep = budget - CUT_MARKER.chars().count();
    let head: String = chars[..keep / 2].iter().collect();
    let tail: String = chars[chars.len() - (keep - keep / 2)..].iter().collect();
    format!("{head}{CUT_MARKER}{tail}")
}

/// 一个占位符下的全部邻章片段 —— 逐片段截断,再按整体预算从头保留。
/// 返回空串表示没有上下文(调用方模板里对应位置就空着)。
fn join_chapter_pairs(parts: &[(String, String)]) -> String {
    let blocks: Vec<String> = parts
        .iter()
        .map(|(title, content)| {
            let compact = compact_blank_lines(content);
            let compact = if compact.chars().count() > NEIGHBOR_CHAPTER_BUDGET {
                cut_middle(&compact, NEIGHBOR_CHAPTER_BUDGET)
            } else {
                compact
            };
            format_context_chapter(title, &compact)
        })
        .collect();
    if blocks.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut kept = 0usize;
    for block in blocks {
        let len = block.chars().count();
        // 整个片段放不下时先截断再收尾:整体预算优先于单个片段。
        if kept + len > SLOT_BUDGET {
            let remain = SLOT_BUDGET.saturating_sub(kept);
            if remain == 0 {
                break;
            }
            out.push_str(&cut_middle(&block, remain));
            break;
        }
        out.push_str(&block);
        kept += len;
    }
    out
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
    let prev_o = join_chapter_pairs(ctx.prev_original);
    let next_o = join_chapter_pairs(ctx.next_original);
    let prev_t = join_chapter_pairs(ctx.prev_transformed);
    // 预算变量让模板能自我说明截断策略(也出现在前端变量清单里)。
    let per_chapter = NEIGHBOR_CHAPTER_BUDGET.to_string();
    let per_slot = SLOT_BUDGET.to_string();
    let vars: [(&str, &str); 8] = [
        ("chapter_title", ctx.chapter.title.as_str()),
        ("chapter_content", ctx.chapter_content),
        ("prev_original", prev_o.as_str()),
        ("next_original", next_o.as_str()),
        ("prev_transformed", prev_t.as_str()),
        ("novel_title", ctx.transformation_novel.title.as_str()),
        ("prev_context_budget", per_chapter.as_str()),
        ("total_context_budget", per_slot.as_str()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Chapter;
    use chrono::Utc;

    fn chapter(title: &str, body: &str) -> Chapter {
        Chapter {
            id: 1,
            data_asset_id: 1,
            idx: 1,
            title: title.into(),
            body: body.into(),
            word_count: body.chars().count() as i32,
            source_kind: "original".into(),
            source_chapter_id: None,
            edited_at: None,
            title_line: None,
        }
    }

    fn novel() -> TransformationNovel {
        TransformationNovel {
            id: 1,
            data_asset_id: 1,
            title: "测试书".into(),
            created_at: Utc::now(),
            note: String::new(),
        }
    }

    /// 短邻章:原样进(带定界与行号),不被截断,也不出现省略标记。
    #[test]
    fn short_neighbor_is_not_truncated() {
        let parts = vec![(
            "第一章".to_string(),
            "甲走过长街。\n\n乙在那里等他。".to_string(),
        )];
        let out = join_chapter_pairs(&parts);
        assert!(out.contains("----- 章节:第一章"), "缺少章节定界: {out}");
        assert!(out.contains("1 | 甲走过长街。"), "缺行号: {out}");
        assert!(out.contains("3 | 乙在那里等他。"), "空行应占一行号: {out}");
        assert!(!out.contains(CUT_MARKER), "短片段不该被截断: {out}");
    }

    /// 超长邻章:保留头尾且在预算内 —— 回归"整章全文塞进 prompt 顶爆 max_context"。
    #[test]
    fn long_neighbor_keeps_head_and_tail_within_budget() {
        let head = "头".repeat(5000);
        let tail = "尾".repeat(5000);
        let parts = vec![("长章".to_string(), format!("{head}{tail}"))];
        let out = join_chapter_pairs(&parts);
        assert!(out.contains(CUT_MARKER), "超预算片段必须带省略标记");
        assert!(out.contains('头'), "头部应保留");
        assert!(out.contains('尾'), "尾部应保留");
        let content_len = out
            .lines()
            .filter_map(|l| {
                l.split_once(" | ")
                    .or_else(|| l.split_once(" |\u{3000}"))
                    .map(|(_, rest)| rest.chars().count())
            })
            .sum::<usize>();
        assert!(
            content_len <= NEIGHBOR_CHAPTER_BUDGET,
            "截断后正文 {content_len} 字应落在 {NEIGHBOR_CHAPTER_BUDGET} 预算内"
        );
    }

    /// 多片段合计超整体预算时,从头保留并收尾截断。
    #[test]
    fn slot_budget_caps_total_context() {
        let one = "字".repeat(NEIGHBOR_CHAPTER_BUDGET);
        let parts: Vec<(String, String)> =
            (0..5).map(|i| (format!("第{i}章"), one.clone())).collect();
        let out = join_chapter_pairs(&parts);
        assert!(
            out.chars().count() <= SLOT_BUDGET + 64,
            "整体上下文 {} 字超出 SLOT_BUDGET {SLOT_BUDGET}",
            out.chars().count()
        );
    }

    /// 无邻章 → 空串(模板里那一行就空着,不留悬挂定界)。
    #[test]
    fn empty_context_renders_empty() {
        assert_eq!(join_chapter_pairs(&[]), "");
    }

    /// 当前章节正文不经过截断 —— 预算只作用于邻章。
    #[test]
    fn chapter_content_is_never_truncated() {
        let body = "正".repeat(NEIGHBOR_CHAPTER_BUDGET * 3);
        let ch = chapter("第十章", &body);
        let tn = novel();
        let ctx = PromptContext {
            transformation_novel: &tn,
            chapter: &ch,
            chapter_content: &body,
            prev_original: &[],
            prev_transformed: &[],
            next_original: &[],
            kind: PromptKind::Compress,
        };
        let rendered = render(
            "x\n---\n{{chapter_content}}\n预算={{prev_context_budget}}/{{total_context_budget}}",
            &ctx,
        );
        assert!(rendered.user.contains("正".repeat(10).as_str()));
        assert_eq!(
            rendered.user.matches('正').count(),
            body.chars().count(),
            "当前章节正文必须一字不少"
        );
        assert!(rendered
            .user
            .contains(&format!("预算={NEIGHBOR_CHAPTER_BUDGET}/{SLOT_BUDGET}")));
        assert!(rendered.system.is_some());
    }
}
