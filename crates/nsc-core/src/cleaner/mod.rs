//! 文本清洗规则集(精简版:4 条规则)。
//! 仅供 Upload.vue 实时预览使用 — 不维护 byte offset map(无 raw 坐标系需求)。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleId {
    AddIndentToUnindented,
    MergeShortParagraphs,
    CollapseBlankRuns,
    EnsureBlankLineBetweenParagraphs,
}

/// 规则默认顺序:合并短段 → 段落间空行 → 缩进 → 折叠空行。
/// 合并必须先于缩进:缩进后每行都以 　　开头,merge 的 `next.starts_with(INDENT)`
/// 守卫会跳过这些行 → 永远合并不上。ensure_blank 跟在 merge 后面,把合出来的
/// 段落再补上空行分隔;放在 indent 前面是为了不被 indent 影响(空行保持空行,
/// 内容行各自缩进)。Legacy pipeline 也是先 merge 后 indent。
pub fn default_rules() -> Vec<RuleId> {
    vec![
        RuleId::MergeShortParagraphs,
        RuleId::EnsureBlankLineBetweenParagraphs,
        RuleId::AddIndentToUnindented,
        RuleId::CollapseBlankRuns,
    ]
}

pub fn apply_rules(text: &str, rules: &[RuleId]) -> String {
    let mut s = normalize_newlines(text);
    for &r in rules {
        s = match r {
            RuleId::AddIndentToUnindented => run_add_indent(&s),
            RuleId::MergeShortParagraphs => run_merge_short_paragraphs(&s),
            RuleId::CollapseBlankRuns => run_collapse_blank_runs(&s),
            RuleId::EnsureBlankLineBetweenParagraphs => {
                run_ensure_blank_line_between_paragraphs(&s)
            }
        };
    }
    s
}

/// 规整行尾: `\r\n`、孤立 `\r`、`\r\r\n` 这种奇葩行尾都归一成单个 `\n`;
/// 旧的 Mac `\r\r`(空白行)仍保留两个 `\n`,即 2 个 line break。
///
/// 用户的 .txt 多半是 Windows 行结尾(实际扫到 `\r\r\n` 双 CR),合并规则用
/// `split('\n')` 切完后每行末尾还挂着 `\r`。merge 把多行拼成一行后,中间残留
/// 的 `\r` 在浏览器 `<textarea>` 里仍被当成换行渲染 → 视觉上跟原文一样,
/// 用户看着"合并无效"。
///
/// `text.replace("\r\n", "\n").replace('\r', "\n")` 行不通 —— `\r\r\n` 经第一遍
/// 变 `\r\n`,第二遍把孤立 `\r` 也换成 `\n`,变成 `\n\n`,行数翻倍。
fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\r' {
            match chars.get(i + 1) {
                Some('\n') => {
                    // \r\n → \n
                    out.push('\n');
                    i += 2;
                }
                Some('\r') => match chars.get(i + 2) {
                    Some('\n') => {
                        // \r\r\n → \n(用户的奇葩行尾,1 个 line break)
                        out.push('\n');
                        i += 3;
                    }
                    _ => {
                        // \r\r 或 \r\r<非 \n>:两个独立 \r,各算一个 line break
                        out.push('\n');
                        i += 1;
                    }
                },
                _ => {
                    // \r 末尾或后跟普通字符 → \n
                    out.push('\n');
                    i += 1;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

const INDENT: &str = "　　";

fn run_add_indent(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.starts_with(INDENT) {
            out.push_str(line);
        } else {
            out.push_str(INDENT);
            out.push_str(line);
        }
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

/// 行尾"闭合/句读"标点 = 这里本可以断句,换行是作者意图,不该合并。
const TRAILING_PUNCT: &[char] = &[
    '。', '，', '、', '；', '：', '？', '！', '…', '—', '～', '·',
    '”', '’', '」', '』', '）', '》', '〉', '】',
    '.', ',', ';', ':', '?', '!', '"', '\'', ')', ']', '}',
];

fn ends_with_punctuation(line: &str) -> bool {
    line.chars().last().is_some_and(|c| TRAILING_PUNCT.contains(&c))
}

/// 行尾逗号(中/英)是"分句未完成"的强信号 → 强制合并下一行。
/// 这覆盖 `ends_with_punctuation` 默认的"行尾有标点不合并"语义:
/// 逗号不是句末标点,折行通常是被动换行/复制粘贴残留,不该保留。
const TRAILING_COMMA: &[char] = &[',', '，'];

fn ends_with_comma(line: &str) -> bool {
    line.chars().last().is_some_and(|c| TRAILING_COMMA.contains(&c))
}

fn run_merge_short_paragraphs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(line);
        let Some(next) = lines.get(i + 1) else { break };
        let join_next = !line.trim().is_empty()
            && !next.trim().is_empty()
            && !next.starts_with(INDENT)
            && (ends_with_comma(line) || !ends_with_punctuation(line));
        if !join_next {
            out.push('\n');
        }
    }
    out
}

fn run_collapse_blank_runs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut consecutive = 0usize;
    for c in text.chars() {
        if c == '\n' {
            consecutive += 1;
            if consecutive <= 2 {
                out.push('\n');
            }
        } else {
            consecutive = 0;
            out.push(c);
        }
    }
    out
}

/// 在每对相邻非空行之间插一个空行,变成 "段落\n\n段落"。
///
/// 紧跟在 MergeShortParagraphs 后面跑 —— merge 把折行拼成一段后,段跟段
/// 直接相邻没有空行;这条规则补上空行做视觉分段。已经有的空行不会重复插
/// (只看 `line.trim().is_empty()`),所以输入是 "段1\n\n段2" 不会变成
/// "段1\n\n\n段2"。
fn run_ensure_blank_line_between_paragraphs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(line);
        let Some(next) = lines.get(i + 1) else { break };
        if !line.trim().is_empty() && !next.trim().is_empty() {
            // 当前行非空且下一行非空 → 在中间加一个空行
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // 为什么值得测:`apply_rules` 是**会写回正文**的路径 ——
    // `Upload.vue::onCleaningConfirm` 把清洗结果写进 `rawText` 并保存到
    // `uploads.original_text`;而 `CleaningDialog.vue` 里四条规则**默认全开**。
    // 也就是说清洗输出直接决定用户存下来的正文,规则错一步就是数据损坏。
    //
    // 既有覆盖缺口:`src-tauri/src/commands/cleaning.rs` 的功能测试刻意只用了
    // 三条规则(见其 `happy_path_with_all_three_rules`),恰好绕开了
    // `EnsureBlankLineBetweenParagraphs` —— 这里把四条规则逐条补齐。

    /// 单条规则作用(normalize 在规则链之前无条件执行,这里一并覆盖)。
    fn one(text: &str, rule: RuleId) -> String {
        apply_rules(text, &[rule])
    }

    // ── 行尾规整 ────────────────────────────────────────────────────────────

    #[test]
    fn normalizes_crlf_and_lone_cr_to_lf() {
        assert_eq!(apply_rules("甲\r\n乙\r\n", &[]), "甲\n乙\n");
        assert_eq!(apply_rules("甲\r乙\r", &[]), "甲\n乙\n");
        // 旧 Mac 的 \r\r(空行)保留成两个 \n = 2 个 line break
        assert_eq!(apply_rules("甲\r\r乙", &[]), "甲\n\n乙");
        // 实测扫到的 Windows 奇葩行尾 \r\r\n:只算 1 个 line break,不能翻倍
        assert_eq!(apply_rules("甲\r\r\n乙\r\r\n", &[]), "甲\n乙\n");
    }

    #[test]
    fn normalize_is_noop_for_lf_only_text() {
        let t = "甲\n乙\n\n丙\n";
        assert_eq!(apply_rules(t, &[]), t);
    }

    // ── 折叠连续空行 ────────────────────────────────────────────────────────

    #[test]
    fn collapse_blank_runs_keeps_at_most_two_newlines() {
        assert_eq!(
            one("甲\n\n\n\n乙\n", RuleId::CollapseBlankRuns),
            "甲\n\n乙\n"
        );
        assert_eq!(one("甲\n\n\n乙\n", RuleId::CollapseBlankRuns), "甲\n\n乙\n");
        // 已有 1 个空行(2 个 \n)不动
        assert_eq!(one("甲\n\n乙\n", RuleId::CollapseBlankRuns), "甲\n\n乙\n");
        assert_eq!(one("甲\n乙\n", RuleId::CollapseBlankRuns), "甲\n乙\n");
    }

    // ── 加缩进 ──────────────────────────────────────────────────────────────

    #[test]
    fn add_indent_skips_blank_lines_and_already_indented() {
        assert_eq!(
            one("甲\n\n　　已缩进\n乙\n", RuleId::AddIndentToUnindented),
            "　　甲\n\n　　已缩进\n　　乙\n"
        );
    }

    #[test]
    fn add_indent_does_not_touch_whitespace_only_lines() {
        assert_eq!(
            one("甲\n   \n乙\n", RuleId::AddIndentToUnindented),
            "　　甲\n   \n　　乙\n"
        );
    }

    // ── 合并短段 ────────────────────────────────────────────────────────────

    #[test]
    fn merge_joins_lines_without_trailing_punctuation() {
        assert_eq!(
            one(
                "今天天气很\n不错,我们去公园。\n",
                RuleId::MergeShortParagraphs
            ),
            "今天天气很不错,我们去公园。\n"
        );
    }

    #[test]
    fn merge_keeps_lines_ending_with_sentence_punctuation() {
        // 上一行以句末标点结尾 → 不并入下一行
        assert_eq!(
            one("甲。\n乙\n", RuleId::MergeShortParagraphs),
            "甲。\n乙\n"
        );
        assert_eq!(
            one("甲！\n乙\n", RuleId::MergeShortParagraphs),
            "甲！\n乙\n"
        );
    }

    #[test]
    fn merge_joins_when_current_line_lacks_final_punctuation() {
        // ⚠️ 记录当前契约(判据只看**上一行**):上一行没有句末标点时,无论下一行
        // 内容为何,都会被并入。这是 merge 的实现语义 —— 它服务的是"段内被动折行",
        // 并不认识"段落边界"。对**段末无标点**的正文,默认链会把相邻段落并成一行:
        //
        //   "第一段内容\n第二段内容\n" → "　　第一段内容第二段内容\n"
        //
        // 由于清洗结果会被写回 uploads.original_text,这是需要留意的数据损坏面。
        // 若非有意,修法是让 merge 也识别"下一行是完整句子"或先补段落空行(见下)。
        assert_eq!(one("甲\n乙。\n", RuleId::MergeShortParagraphs), "甲乙。\n");
    }

    #[test]
    fn merge_forces_join_after_comma() {
        assert_eq!(
            one("甲，\n乙。\n", RuleId::MergeShortParagraphs),
            "甲，乙。\n"
        );
        assert_eq!(one("a,\nb.\n", RuleId::MergeShortParagraphs), "a,b.\n");
    }

    #[test]
    fn merge_respects_blank_lines_and_indent_guard() {
        // 空行是硬边界,不跨空行合并
        assert_eq!(
            one("甲\n\n乙\n", RuleId::MergeShortParagraphs),
            "甲\n\n乙\n"
        );
        // 已缩进的下一行视为新段落开头 → 不合并(这也是默认顺序里 merge 必须先于 indent 的原因)
        assert_eq!(
            one("甲\n　　乙\n", RuleId::MergeShortParagraphs),
            "甲\n　　乙\n"
        );
    }

    // ── 段落间补空行 ────────────────────────────────────────────────────────

    #[test]
    fn ensure_blank_line_inserts_blank_between_adjacent_non_empty_lines() {
        assert_eq!(
            one("甲\n乙\n", RuleId::EnsureBlankLineBetweenParagraphs),
            "甲\n\n乙\n"
        );
    }

    #[test]
    fn ensure_blank_line_is_idempotent_when_blank_already_present() {
        assert_eq!(
            one("甲\n\n乙\n", RuleId::EnsureBlankLineBetweenParagraphs),
            "甲\n\n乙\n"
        );
        let once = one("甲\n乙\n", RuleId::EnsureBlankLineBetweenParagraphs);
        let twice = one(&once, RuleId::EnsureBlankLineBetweenParagraphs);
        assert_eq!(once, twice, "该规则必须幂等");
    }

    #[test]
    fn ensure_blank_line_never_merges_lines() {
        // 最核心的性质:这条规则的职责是**增加**空行,绝不能减少换行数。
        // 两行都非空时若直接拼接,整段正文会被并成一行。
        for input in ["甲\n乙\n", "甲\n乙\n丙\n", "甲\n\n乙\n丙\n"] {
            let out = one(input, RuleId::EnsureBlankLineBetweenParagraphs);
            let in_lines = input.lines().count();
            let out_lines = out.lines().count();
            assert!(
                out_lines >= in_lines,
                "换行数不应减少: {input:?} ({in_lines} 行) → {out:?} ({out_lines} 行)"
            );
            for line in input.lines().filter(|l| !l.trim().is_empty()) {
                assert!(
                    out.contains(line),
                    "原有内容行不应消失: {line:?} in {out:?}"
                );
            }
        }
    }

    // ── 默认规则链(用户在 UI 上默认全选的那四条) ──────────────────────────

    #[test]
    fn default_rules_has_four_ids_in_documented_order() {
        assert_eq!(
            default_rules(),
            vec![
                RuleId::MergeShortParagraphs,
                RuleId::EnsureBlankLineBetweenParagraphs,
                RuleId::AddIndentToUnindented,
                RuleId::CollapseBlankRuns,
            ]
        );
    }

    #[test]
    fn default_chain_keeps_paragraph_boundaries_when_paragraphs_end_with_punctuation() {
        // 典型中文正文:每段以句末标点收尾。此时段内不动、段间保留空行、各段加缩进。
        let out = apply_rules("第一段内容。\n第二段内容。\n", &default_rules());
        assert_eq!(out, "　　第一段内容。\n\n　　第二段内容。\n");
    }

    #[test]
    fn default_chain_merges_wrapped_lines_but_keeps_paragraphs() {
        // 真实场景:段内折行(行尾无标点)→ 合并;段末有标点 + 中间空行 → 段落边界保留。
        let input = "今天天气很\n不错,我们去\n公园散步。\n\n明天也是好\n天气。\n";
        let out = apply_rules(input, &default_rules());
        assert_eq!(
            out, "　　今天天气很不错,我们去公园散步。\n\n　　明天也是好天气。\n",
            "段内折行应合并、段间应保留空行"
        );
    }

    #[test]
    fn default_chain_collapses_adjacent_paragraphs_without_final_punctuation() {
        // 已知限制(非期望行为,记录以免将来被当成回归):
        // 相邻两段若**前一段末尾没有句末标点**,merge 会把它们并成一行 ——
        // 因为 merge 的判据只有"上一行是否以标点结尾",它不认识段落边界。
        // 清洗结果会写回 uploads.original_text,所以这条要记在案。
        let out = apply_rules("第一段内容\n第二段内容\n", &default_rules());
        assert_eq!(out, "　　第一段内容第二段内容\n");
    }

    #[test]
    fn default_chain_preserves_every_non_empty_character() {
        // 兜底性质:清洗会写回正文,任何输入下都不应吞掉文字
        // (空白/缩进可以变,字不能少)。段末带标点用真实正文形态。
        let input = "甲乙丙。\n丁戊己。\n";
        let out = apply_rules(input, &default_rules());
        let stripped: String = out
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '　')
            .collect();
        for ch in input.chars().filter(|c| !c.is_whitespace()) {
            assert!(
                stripped.contains(ch),
                "字符 {ch:?} 不应在清洗后消失: {out:?}"
            );
        }
    }
}