/// 字数 = 除空白和换行外的所有字符。包含汉字、字母、数字、标点符号。
/// 与 Word / WPS / 网文平台 / AI 输出的字的概念一致。
pub fn count(s: &str) -> i32 {
    s.chars().filter(|c| !c.is_whitespace()).count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        assert_eq!(count(""), 0);
    }

    #[test]
    fn whitespace_only() {
        assert_eq!(count("   "), 0);
        assert_eq!(count("\n\t "), 0);
    }

    #[test]
    fn chinese_pure() {
        // 中文每字 1
        assert_eq!(count("第一章 开启"), 5);
        assert_eq!(count("正文一\n\n正文二"), 6);
        assert_eq!(count(&"一".repeat(10_000)), 10_000);
    }

    #[test]
    fn english_pure() {
        assert_eq!(count("hello world"), 10);
        assert_eq!(count("don\"t go there"), 12);
    }

    #[test]
    fn mixed_chinese_english() {
        assert_eq!(count("hello 中文"), 7);
        assert_eq!(count("第 1 章 Chapter One"), 13);
    }

    #[test]
    fn leading_and_trailing_whitespace() {
        assert_eq!(count("  hello  "), 5);
        assert_eq!(count("\n正文一\n"), 3);
    }

    #[test]
    fn punctuation_counted() {
        // 标点符号现在算字
        assert_eq!(count("你好,世界!"), 6);
        assert_eq!(count("“引号”也算标点"), 8);
    }

    #[test]
    fn cjk_punctuation_counted() {
        // CJK 标点全要算
        assert_eq!(count("大清早，对方恢复神志，挥剑对他动手。"), 18);
        assert_eq!(count("她说道：“吃下它。”"), 10);
    }
}
