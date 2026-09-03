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