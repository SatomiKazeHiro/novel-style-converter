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
