use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::word_count;

#[derive(Debug, Clone)]
pub struct ParsedChapter {
    pub title: String,
    pub content: String,
    pub word_count: i32,
    pub title_line: usize,
}

#[derive(Debug, Clone)]
pub struct SplitResult {
    pub chapters: Vec<ParsedChapter>,
}

pub trait ChapterSplitter: Send + Sync {
    fn split(&self, text: &str) -> SplitResult;
}

static RE_CHAPTER_CN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[\s\p{Cf}]*第[零一二三四五六七八九十百千万亿0-9]+[章回][^\n]*$").unwrap());
static RE_CHAPTER_EN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[\s\p{Cf}]*Chapter\s+\d+[^\n]*$").unwrap());
/// 图片 spec: 限定后续字必须为 [节部篇集辑]。避免"第一次/第三式/第一我们"等任意字都被误识别。
static RE_VOLUME: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[\s\p{Cf}]*第[零一二三四五六七八九十百千万亿0-9]+[节部篇集辑][^\n]*$").unwrap());
static RE_CHAPTER_PCN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[\s\p{Cf}]*[^\n]{0,40}[《〈][^\n]{0,80}[》〉][^\S\n]*$").unwrap());
static RE_BLANK_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\r?\n[ \t]*\r?\n+").unwrap());

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSplitter;

fn auto_matches_in(text: &str) -> Vec<(usize, usize, String)> {
    let mut matches = Vec::new();
    for re in [&RE_CHAPTER_CN, &RE_CHAPTER_EN, &RE_VOLUME, &RE_CHAPTER_PCN] {
        for m in re.find_iter(text) { matches.push((m.start(), m.end(), strip_invisibles(m.as_str()))); }
    }
    matches.sort_by_key(|(pos, _, _)| *pos);
    matches.dedup_by_key(|(pos, _, _)| *pos);
    matches
}

/// 去掉首尾的 whitespace + Cf 格式字符。
/// splitter regex 用 [\s\p{Cf}]* 容忍 invisible 前缀(ZWSP/BOM/全角空格等),
/// 但 match 出来的 title 会把这些 invisible 也带进来 —— 这里把首尾 invisible 清掉,
/// 让 title 存的就是干净的「第N章：xxx」。
fn strip_invisibles(s: &str) -> String {
    let is_inv = |c: char| c.is_whitespace() || c.is_control() || matches!(c,
        // Rust stdlib 的 is_control() 漏了 ZWSP (U+200B)、BOM (U+FEFF)、WJ (U+2060),
        // 这三个是最常见的 invisible 字符(网页复制经常带),在这里显式补上。
        '​' | '﻿' | '⁠'
    );
    s.trim_matches(is_inv).to_string()
}

/// 把空行 fallback 的段落切成 (title=首行, content=次行起)。
/// 消除「正则路径 content 不含标题、fallback 含首行」的不一致。
/// 单行段落 → content 为空(确定性输出)。
fn split_first_line(s: &str) -> (String, String) {
    match s.find('\n') {
        Some(pos) => (s[..pos].trim().to_string(), s[pos + 1..].trim().to_string()),
        None => (s.to_string(), String::new()),
    }
}

impl ChapterSplitter for DefaultSplitter {
    fn split(&self, text: &str) -> SplitResult {
        if text.trim().is_empty() { return SplitResult { chapters: vec![] }; }
        let matches = auto_matches_in(text);
        if matches.is_empty() {
            let mut chapters = Vec::new();
            let mut cursor = 0;
            for m in RE_BLANK_LINE.find_iter(text) {
                let (start, end) = (cursor, m.start());
                if start < end {
                    let s = &text[start..end];
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        let (title, content) = split_first_line(trimmed);
                        let title_line = text[..start].matches('\n').count();
                        chapters.push(ParsedChapter { title, content: content.to_string(), word_count: word_count(&content), title_line });
                    }
                }
                cursor = m.end();
            }
            if cursor < text.len() {
                let s = &text[cursor..];
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    let (title, content) = split_first_line(trimmed);
                    let title_line = text[..cursor].matches('\n').count();
                    chapters.push(ParsedChapter { title, content: content.to_string(), word_count: word_count(&content), title_line });
                }
            }
            return SplitResult { chapters };
        }
        let mut chapters = Vec::new();
        for (i, (_, end, title)) in matches.iter().enumerate() {
            let content_end = matches.get(i + 1).map(|(start, _, _)| *start).unwrap_or(text.len());
            let content = text[*end..content_end].trim().to_string();
            let title_line = text[..*end].matches('\n').count();
            if !content.is_empty() { chapters.push(ParsedChapter { title: title.clone(), word_count: word_count(&content), content, title_line }); }
        }
        SplitResult { chapters }
    }
}

#[cfg(test)]
mod tests {
    //! 回归测试:splitter 章节 regex 必须容忍 Unicode 水平空白。
    //!
    //! 之前用 `[ \t]*` 只容忍 ASCII 空白,如果章节标题前有全角空格(\u{3000})或
    //! nbsp(\u{00A0})等,会被漏掉,前端拿到的 chapter 数比真实少 1,造成「两个
    //! 第1章」、中间章节消失等 bug。2026-08 用户小说「我家老婆来自一千年前」就
    //! 撞到这条。

    use super::*;

    fn assert_chapters(text: &str, expected: &[&str]) {
        let r = DefaultSplitter.split(text);
        let got: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(got, expected, "titles mismatch for text:\n{}\n\n--- got ---\n{:#?}\n", text, r.chapters);
    }

    #[test]
    fn ascii_only() {
        let t = "第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        assert_chapters(t, &["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn fullwidth_space_u3000_before_title() {
        let t = "第1章：我是好人\nbody1\n\u{3000}第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        assert_chapters(t, &["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn nbsp_u00a0_before_title() {
        let t = "第1章：我是好人\nbody1\n\u{00A0}第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        assert_chapters(t, &["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn tab_before_title() {
        let t = "第1章：我是好人\nbody1\n\t第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        assert_chapters(t, &["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn volume_with_u3000() {
        // RE_VOLUME spec 限定"第"+中文/阿拉伯数字+节部篇集辑(刻意排除"卷"以免误识别"第一次")。
        let t = "第一节 开篇\nbody1\n\u{3000}第二节 接续\nbody2\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("第二节")), "RE_VOLUME 漏了带全角空格的 '第二节', got {:?}", titles);
    }

    #[test]
    fn chapter_pcn_with_u3000() {
        let t = "序章\n\u{3000}《楔子》\nbody1\n《正篇》\u{3000}\nbody2\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("楔子")), "RE_CHAPTER_PCN 漏了带全角空格的《楔子》, got {:?}", titles);
    }

    #[test]
    fn chinese_numerals_match_volume_spec() {
        // 中文数字命中节部篇集辑 → RE_CHAPTER_CN + RE_VOLUME 都吃中文数字。
        let t = "第1章\nbody\n第二篇\nbody\n第3章\nbody\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<String> = r.chapters.iter().map(|c| c.title.clone()).collect();
        assert_eq!(titles, vec!["第1章", "第二篇", "第3章"]);
    }

    #[test]
    fn no_match_one_segment() {
        let r = DefaultSplitter.split("普通段落一\n普通段落二\n");
        assert_eq!(r.chapters.len(), 1);
    }

    #[test]
    fn user_novel_double_fullwidth_space() {
        let t = "第1章：我是好人\n\u{3000}\u{3000}正文段落一\n正文段落二\n\u{3000}\u{3000}第2章：这是个误会\n正文段落三\n第3章：他们都已经成为历史\n\u{3000}\u{3000}正文四段\n正文五段\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn user_novel_with_volume_prefix() {
        let t = "我家老婆来自一千年前\n作者：花还没开\n\n第一卷 遇见\n\n第1章：我是好人\n正文\n\u{3000}\u{3000}第2章：这是个误会\n正文\n第3章：他们都已经成为历史\n正文\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.contains("第1章")), "第1章 missing, got {:?}", titles);
        assert!(titles.iter().any(|t| t.contains("第2章")), "第2章 missing, got {:?}", titles);
        assert!(titles.iter().any(|t| t.contains("第3章")), "第3章 missing, got {:?}", titles);
    }

    #[test]
    fn user_novel_long_with_markers_split() {
        let t = "第1章：我是好人\n正文一段正文一段\n\u{3000}\u{3000}第2章：这是个误会\n正文二段正文二段\n第3章：他们都已经成为历史\n正文三段\n";
        let r = DefaultSplitter.split(t);
        assert_eq!(r.chapters.len(), 3, "got {:?}", r.chapters);
        assert_eq!(r.chapters[1].title, "第2章：这是个误会");
    }

    #[test]
    fn zwsp_before_title() {
        // 网上复制常带 ZWSP (U+200B),是 \p{Cf} 不是 \s — 之前 [ \t]* 漏
        let t = "第1章：我是好人\nbody1\n\u{200B}第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn bom_before_title() {
        let t = "第1章：我是好人\nbody1\n\u{FEFF}第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn multiple_cf_chars_before_title() {
        let t = "第1章：我是好人\nbody1\n\u{200B}\u{FEFF}\u{3000}第2章：这是个误会\nbody2\n第3章：他们都已经成为历史\nbody3\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["第1章：我是好人", "第2章：这是个误会", "第3章：他们都已经成为历史"]);
    }

    #[test]
    fn title_line_reported_for_regex_chapters() {
        let t = "第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n";
        let r = DefaultSplitter.split(t);
        let lines: Vec<usize> = r.chapters.iter().map(|c| c.title_line).collect();
        assert_eq!(lines, vec![0, 2], "title_line 应指向标题行, got {:?}", lines);
    }

    #[test]
    fn title_line_with_leading_blank_lines() {
        let t = "\n\n第1章：我是好人\nbody1\n第2章：这是个误会\nbody2\n";
        let r = DefaultSplitter.split(t);
        let lines: Vec<usize> = r.chapters.iter().map(|c| c.title_line).collect();
        assert_eq!(lines, vec![2, 4], "前导空行会 off-by, got {:?}", lines);
    }

    #[test]
    fn title_line_for_blank_line_fallback() {
        let t = "段落一标题\n段落一正文\n\n段落二标题\n段落二正文\n";
        let r = DefaultSplitter.split(t);
        assert_eq!(r.chapters.len(), 2);
        assert_eq!(r.chapters[0].title, "段落一标题");
        assert_eq!(r.chapters[0].title_line, 0);
        assert_eq!(r.chapters[0].content, "段落一正文");
        assert_eq!(r.chapters[1].title_line, 3);
        assert_eq!(r.chapters[1].content, "段落二正文");
    }

    // ── 此前未覆盖的路径 ────────────────────────────────────────────────────
    //
    // 下面这组补的是 34 个既有用例完全没碰过的分支。重点是英文正则 `RE_CHAPTER_EN`
    // (混排场景下直接决定章节数)与「空正文章节」的处理。

    #[test]
    fn english_chapter_regex() {
        // RE_CHAPTER_EN 此前零覆盖。它失效的后果很严重:英文小说会退化成
        // 「整本 = 1 章」(走空行兜底),而不是切出 N 章。
        let t = "Chapter 1: The Beginning\nbody one\nChapter 2: Rising\nbody two\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["Chapter 1: The Beginning", "Chapter 2: Rising"]);
        assert_eq!(r.chapters[0].content, "body one");
        assert_eq!(r.chapters[1].content, "body two");
    }

    #[test]
    fn english_chapter_regex_is_case_sensitive() {
        // 记录当前行为:正则无 `(?i)`,只认 `Chapter N` 这一种大小写。
        // 小写 `chapter 1` / 全大写 `CHAPTER 1` **不被识别** → 落到空行兜底,
        // 整段成为「标题=首行」的单章。若非有意,这是一处英文小说的识别缺口。
        for text in ["chapter 1\nbody\nchapter 2\nbody2\n", "CHAPTER 1\nbody\n"] {
            let r = DefaultSplitter.split(text);
            assert_eq!(r.chapters.len(), 1, "当前不识别该大小写,应退化为单章: {text:?}");
        }
    }

    #[test]
    fn english_spelled_out_number_is_not_matched() {
        // `Chapter One`(拼写数字)不匹配 `\d+` → 整本退化为单章。
        // 同样是「记录现状 + 标出缺口」:英文小说常见这种写法。
        let r = DefaultSplitter.split("Chapter One\nbody\nChapter Two\nbody2\n");
        assert_eq!(r.chapters.len(), 1);
        assert_eq!(r.chapters[0].title, "Chapter One");
        assert_eq!(r.chapters[0].content, "body\nChapter Two\nbody2");
    }

    #[test]
    fn mixed_chinese_and_english_chapters() {
        // 中英混排时两个正则都要生效,且按出现位置排序。
        let t = "第1章 中\nbody\nChapter 2 EN\nbody2\n第3章 中\nbody3\n";
        let r = DefaultSplitter.split(t);
        let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["第1章 中", "Chapter 2 EN", "第3章 中"]);
    }

    #[test]
    fn empty_text_and_whitespace_only_yield_no_chapters() {
        for text in ["", "   ", "\n\n\n", "  \n\t\n  "] {
            let r = DefaultSplitter.split(text);
            assert!(r.chapters.is_empty(), "空/纯空白输入应产出 0 章: {text:?} → {:?}",
                r.chapters.iter().map(|c| &c.title).collect::<Vec<_>>());
        }
    }

    #[test]
    fn chapter_without_body_is_dropped() {
        // ⚠️ 记录当前行为,同时也是已知风险点(rules.rs 正则路径的
        // `if !content.is_empty()`):**标题存在但没有正文的章节会被整章丢弃**。
        //
        // 后果:作者偶尔会写一个只有标题的过场章,或正文只有空白 —— 那一章会从
        // 结果里消失,章节 idx 随之整体前移(不是空洞占位)。若这是有意的(避免产出
        // 空章节),应在 rules.rs 注释里写明;若不是,应改为保留空 content 的章节。
        let only = DefaultSplitter.split("第1章：只有标题");
        assert!(only.chapters.is_empty(), "仅有标题、无正文 → 当前会整章丢弃");

        // 丢的是「无正文那一章」,其余章保留且 idx 顺延
        let head = DefaultSplitter.split("第1章：甲\n正文甲\n第2章：乙\n");
        assert_eq!(head.chapters.len(), 1);
        assert_eq!(head.chapters[0].title, "第1章：甲");

        let mid = DefaultSplitter.split("第1章：甲\n第2章：乙\n正文乙\n");
        assert_eq!(mid.chapters.len(), 1);
        assert_eq!(mid.chapters[0].title, "第2章：乙", "被丢的是无正文的第1章");

        // 正文只有空白等同于无正文
        let blank = DefaultSplitter.split("第1章：甲\n   \n第2章：乙\n正文乙\n");
        assert_eq!(blank.chapters.len(), 1);
        assert_eq!(blank.chapters[0].title, "第2章：乙");
    }
}
