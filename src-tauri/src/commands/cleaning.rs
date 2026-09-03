use nsc_core::cleaner::RuleId;

#[derive(Debug, serde::Serialize)]
pub struct CleaningPreview {
    pub cleaned_text: String,
    pub lines_delta: i64,
    pub chars_delta: i64,
}

/// 纯函数预览:**不改 DB**,只对传入文本按 `rule_ids` 顺序应用清洗规则,
/// 返回清洗后文本 + 行数 delta + 字符 delta。`rule_ids` 为空或含未知 id 报错。
#[tauri::command]
pub fn preview_cleaning(text: String, rule_ids: Vec<String>) -> Result<CleaningPreview, String> {
    if rule_ids.is_empty() {
        return Err("至少选择一条规则".into());
    }
    let rules = parse_rules(&rule_ids)?;
    let cleaned_text = nsc_core::cleaner::apply_rules(&text, &rules);
    let lines_delta = cleaned_text.lines().count() as i64 - text.lines().count() as i64;
    let chars_delta = cleaned_text.chars().count() as i64 - text.chars().count() as i64;
    Ok(CleaningPreview {
        cleaned_text,
        lines_delta,
        chars_delta,
    })
}

fn parse_rules(rule_ids: &[String]) -> Result<Vec<RuleId>, String> {
    rule_ids
        .iter()
        .map(|s| match s.as_str() {
            "add_indent_to_unindented" => Ok(RuleId::AddIndentToUnindented),
            "merge_short_paragraphs" => Ok(RuleId::MergeShortParagraphs),
            "collapse_blank_runs" => Ok(RuleId::CollapseBlankRuns),
            "ensure_blank_line_between_paragraphs" => {
                Ok(RuleId::EnsureBlankLineBetweenParagraphs)
            }
            other => Err(format!("未知规则: {other}")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsc_core::cleaner::default_rules;

    #[test]
    fn empty_rule_ids_returns_error() {
        let r = preview_cleaning("hello".into(), vec![]);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("至少选择一条规则"));
    }

    #[test]
    fn unknown_rule_id_returns_error() {
        let r = preview_cleaning(
            "hello".into(),
            vec!["add_indent_to_unindented".into(), "frobnicate".into()],
        );
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("未知规则"));
    }

    #[test]
    fn happy_path_with_all_three_rules() {
        let input = "第一行\n第二行\n\n\n\n第三行\n";
        let r = preview_cleaning(
            input.into(),
            vec![
                // 默认顺序:合并 → 缩进 → 折叠。合并在前是关键 ——
                // 加缩进后每行都以 　　开头,merge 的 starts_with(INDENT) 守卫会跳过。
                "merge_short_paragraphs".into(),
                "add_indent_to_unindented".into(),
                "collapse_blank_runs".into(),
            ],
        )
        .expect("ok");
        // 合并:第一行+第二行 → 一行;折叠空行:4 \n → 2 \n。
        // 输入 6 行(.lines() 不计尾部 \n 后的空)→ 输出 3 行,delta = -3。
        assert_eq!(r.lines_delta, -3);
        assert_ne!(r.cleaned_text, input);
        // chars_delta 应该 ≠ 0(加 INDENT 增加字符数)。
        assert_ne!(r.chars_delta, 0);
    }

    #[test]
    fn default_rules_constant_is_four_ids() {
        assert_eq!(default_rules().len(), 4);
    }

    #[test]
    fn merge_short_does_collapse_wrapped_lines() {
        // 用户视角:3 行不完整的折行应该被合并成 1 行。
        let input = "今天天气很\n不错,我们去\n公园散步。\n";
        let r = preview_cleaning(
            input.into(),
            vec![
                "merge_short_paragraphs".into(),
                "add_indent_to_unindented".into(),
                "collapse_blank_runs".into(),
            ],
        )
        .expect("ok");
        // 合并:3 行 → 1 行;AddIndent:加 　　;CollapseBlank:无变化。
        // 输入 4 行 → 输出 2 行 (.lines() 不计尾部 \n 后的空)。
        assert_eq!(r.lines_delta, -2);
        // 合并后内容正确
        assert!(r.cleaned_text.contains("今天天气很不错,我们去公园散步。"));
    }
}