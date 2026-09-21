//! 文本清洗(cleaner)的规则回归 —— 该模块此前 **0 测试**。
//!
//! 为什么值得单独测:`apply_rules` 是**会写回正文**的路径 ——
//! `Upload.vue::onCleaningConfirm` 把清洗结果写进 `rawText` 并保存到
//! `uploads.original_text`;而 `CleaningDialog.vue` 里四条规则**默认全开**。
//! 也就是说清洗输出直接决定用户存下来的正文,规则错一步就是数据损坏。
//!
//! 既有覆盖缺口:`src-tauri/src/commands/cleaning.rs` 的功能测试刻意只用了
//! 三条规则(见其 `happy_path_with_all_three_rules`),恰好绕开了
//! `EnsureBlankLineBetweenParagraphs` —— 本文件把四条规则逐条补齐。
use nsc_core::cleaner::{apply_rules, default_rules, RuleId};

/// 单条规则作用(normalize 在规则链之前无条件执行,这里一并覆盖)。
fn one(text: &str, rule: RuleId) -> String {
    apply_rules(text, &[rule])
}

// ── 行尾规整 ────────────────────────────────────────────────────────────────

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

// ── 折叠连续空行 ────────────────────────────────────────────────────────────

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

// ── 加缩进 ──────────────────────────────────────────────────────────────────

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

// ── 合并短段 ────────────────────────────────────────────────────────────────

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

// ── 段落间补空行 ────────────────────────────────────────────────────────────
// 这条规则此前零功能测试,且实现有 bug(两行都非空时会把换行吞掉)。

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

// ── 默认规则链(用户在 UI 上默认全选的那四条) ──────────────────────────────

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
