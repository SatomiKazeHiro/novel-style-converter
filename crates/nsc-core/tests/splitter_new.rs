//! 回归测试:splitter 章节 regex 必须容忍 Unicode 水平空白。
//!
//! 之前用 `[ \t]*` 只容忍 ASCII 空白,如果章节标题前有全角空格(\u{3000})或
//! nbsp(\u{00A0})等,会被漏掉,前端拿到的 chapter 数比真实少 1,造成「两个
//! 第1章」、中间章节消失等 bug。2026-08 用户小说「我家老婆来自一千年前」就
//! 撞到这条。
use nsc_core::splitter::{ChapterSplitter, DefaultSplitter};

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
    println!("got: {:?}", titles);
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
