use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingKind {
    Utf8,
    Gbk,
    Ascii,
    Other,
}

impl fmt::Display for EncodingKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf8 => write!(f, "utf-8"),
            Self::Gbk => write!(f, "gbk"),
            Self::Ascii => write!(f, "ascii"),
            Self::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedText {
    pub kind: EncodingKind,
    pub text: String,
}

/// 把任意字节流检测编码并解码为 UTF-8 字符串。
pub fn decode_to_utf8(bytes: &[u8]) -> Result<DecodedText, String> {
    if bytes.is_empty() {
        return Ok(DecodedText { kind: EncodingKind::Utf8, text: String::new() });
    }
    if bytes.iter().all(|b| *b < 0x80) {
        return Ok(DecodedText {
            kind: EncodingKind::Ascii,
            text: String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?,
        });
    }
    // 严格 UTF-8 验证优先:chardetng 是统计检测,短文本/ASCII-heavy 合法 UTF-8
    // 可能误判为 GBK。先尝试严格 UTF-8 验证,成功就直接走 UTF-8 路径。
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(DecodedText {
            kind: EncodingKind::Utf8,
            text: s.to_string(),
        });
    }
    let mut detected = chardetng::EncodingDetector::new();
    detected.feed(bytes, true);
    let encoding = detected.guess(None, true);
    let label = encoding.name();
    let enc = encoding_rs::Encoding::for_label_no_replacement(label.as_bytes())
        .or_else(|| encoding_rs::Encoding::for_label(b"utf-8"))
        .ok_or_else(|| format!("unsupported encoding: {label}"))?;
    let (text, _enc, had_unmappable) = enc.decode(bytes);
    let kind = match label {
        "UTF-8" => EncodingKind::Utf8,
        "GBK" | "GB18030" => EncodingKind::Gbk,
        _ => EncodingKind::Other,
    };
    if had_unmappable {
        return Err(format!("{kind} 编码含不可映射字符"));
    }
    Ok(DecodedText { kind, text: text.into_owned() })
}

/// 读盘 + 解码:把任意编码的文本文件归一为 UTF-8。
///
/// upload_file 写盘用的是原字节(只 decode 校验不入库),
/// 所以磁盘上的 .txt 可能是 GBK/BIG5 等非 UTF-8 字节。
/// 所有"读 upload 全文"的入口(get_upload_text / list_chapter_segments)
/// 统一走这里,避免 `read_to_string` 在非 UTF-8 文件上炸
/// "stream did not contain valid UTF-8"。
pub fn read_text_file(path: &Path) -> Result<DecodedText, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读文件失败({}): {e}", path.display()))?;
    decode_to_utf8(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// "中文" 的 GBK 字节(用 encoding_rs 实测确认后固化为字面量,
    /// 免得测试自己 encode 一遍变成"自己验自己")。
    const GBK_ZHONGWEN: &[u8] = &[214, 208, 206, 196];
    /// "测试" 的 GBK 字节。
    const GBK_CESHI: &[u8] = &[178, 226, 202, 212];

    #[test]
    fn empty_is_utf8_empty() {
        let r = decode_to_utf8(&[]).unwrap();
        assert_eq!(r.text, "");
        assert_eq!(r.kind, EncodingKind::Utf8);
    }

    /// 纯 ASCII(含换行/制表符)判为 Ascii 且原样解出。
    #[test]
    fn pure_ascii_is_detected_as_ascii() {
        let bytes = b"Chapter 1\n\tHello, world!\n";
        let r = decode_to_utf8(bytes).unwrap();
        assert_eq!(r.kind, EncodingKind::Ascii);
        assert_eq!(r.text, "Chapter 1\n\tHello, world!\n");
    }

    /// 合法 UTF-8(含中文/emoji)判为 Utf8 —— 走的是 `from_utf8` 严格验证这条快捷路径。
    #[test]
    fn valid_utf8_is_detected_as_utf8() {
        for s in ["中文标题", "第一章：开始", "emoji 🎉 也在", "混合 mixed 文本"] {
            let r = decode_to_utf8(s.as_bytes()).unwrap();
            assert_eq!(r.kind, EncodingKind::Utf8, "应识别为 UTF-8: {s}");
            assert_eq!(r.text, s);
        }
    }

    /// **GBK 必须被正确识别并解码** —— 中文小说常见 GBK 编码,
    /// 判错就是整本乱码。这是本模块存在的理由。
    #[test]
    fn gbk_is_detected_and_decoded() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(GBK_ZHONGWEN);
        bytes.extend_from_slice(GBK_CESHI);
        let r = decode_to_utf8(&bytes).unwrap();
        assert_eq!(r.kind, EncodingKind::Gbk, "GBK 字节应识别为 Gbk");
        assert_eq!(r.text, "中文测试");
    }

    /// 较长 GBK 文本(接近真实章节规模)—— 统计检测在长文本上更可靠,
    /// 短样本容易误判,所以要覆盖"够长"的情形。
    #[test]
    fn longer_gbk_text_decodes() {
        let long = "第一章　开始\n　　这是一段中文正文，用来让编码检测有足够的样本量。".repeat(10);
        let (bytes, _, err) = encoding_rs::GBK.encode(&long);
        assert!(!err, "构造 GBK 样本不应有不可映射字符");
        let r = decode_to_utf8(&bytes).unwrap();
        assert_eq!(r.kind, EncodingKind::Gbk);
        assert_eq!(r.text, long);
    }

    /// 严格 UTF-8 验证优先于统计检测:合法 UTF-8 即使含大量非 ASCII 也不能被判成 GBK。
    /// (短文本 + chardetng 统计误判是真实风险,注释里写明了这个顺序的原因。)
    #[test]
    fn strict_utf8_wins_over_statistical_detection() {
        // 纯中文的合法 UTF-8 —— chardetng 在极端情况下可能倾向 GBK
        let s = "第一章";
        let r = decode_to_utf8(s.as_bytes()).unwrap();
        assert_eq!(r.kind, EncodingKind::Utf8);
        assert_eq!(r.text, s);
    }

    /// 记录真实行为:孤立的高位字节 **不会** 报错 —— chardetng 会退到单字节编码
    /// (如 ISO-8859-1)把它解成 `"ÿ"`,`kind` 落 `Other`。
    /// 也就是说"不可解码"分支在实践中很罕见,单字节数据总能被某个编码吃下。
    /// 对 callers 的含义:读盘时若混入二进制,**不会**炸,而是得到一段 `Other` 文本 ——
    /// 判定文件是否"真的是文本"要靠上层(如 is_text_like),不能指望这里报错。
    #[test]
    fn lone_high_byte_decodes_as_other_not_error() {
        let r = decode_to_utf8(&[0xFF]).expect("chardetng 会选一个编码吃下它,不报错");
        assert_eq!(r.kind, EncodingKind::Other);
        assert_eq!(r.text, "ÿ", "应退化为单字节编码解释");
    }

    /// 但无论走哪条路径,返回的 `text` 永远是可用的 UTF-8 Rust String
    /// (这正是本模块的存在意义:让上层不必处理字节)。
    #[test]
    fn result_text_is_always_valid_utf8() {
        for bytes in [
            vec![],
            b"ascii".to_vec(),
            "中文".as_bytes().to_vec(),
            GBK_ZHONGWEN.to_vec(),
            vec![0xFF],
            vec![0xC3, 0x28], // 非法 UTF-8 连续序列
        ] {
            let r = decode_to_utf8(&bytes).expect("应总能给出结论");
            // String 天然合法 UTF-8;这里断言它非空规则与字节输入一致(除空输入)
            if !bytes.is_empty() {
                assert!(!r.text.is_empty(), "非空字节应产出非空文本: {bytes:?}");
            }
        }
    }

    /// EncodingKind 的 Display 是 IPC/日志可见的标签,固定住。
    #[test]
    fn encoding_kind_display_labels() {
        assert_eq!(EncodingKind::Utf8.to_string(), "utf-8");
        assert_eq!(EncodingKind::Gbk.to_string(), "gbk");
        assert_eq!(EncodingKind::Ascii.to_string(), "ascii");
        assert_eq!(EncodingKind::Other.to_string(), "other");
    }

    /// read_text_file:从磁盘读出任意编码并归一为 UTF-8。
    /// 这是所有"读 upload 全文"入口的统一路径(避免 read_to_string 在非 UTF-8 上炸)。
    #[test]
    fn read_text_file_normalizes_gbk_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gbk.txt");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(GBK_ZHONGWEN).unwrap();
            f.write_all(GBK_CESHI).unwrap();
        }
        let r = read_text_file(&path).unwrap();
        assert_eq!(r.kind, EncodingKind::Gbk);
        assert_eq!(r.text, "中文测试");
    }

    /// 读取不存在的文件 → Err(带路径信息),不是 panic。
    #[test]
    fn read_text_file_missing_path_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_text_file(&dir.path().join("nope.txt")).unwrap_err();
        assert!(err.contains("读文件失败"), "错误应说明是读文件失败: {err}");
    }
}
