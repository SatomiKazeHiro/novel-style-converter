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

    #[test]
    fn empty() {
        let r = decode_to_utf8(&[]).unwrap();
        assert_eq!(r.text, "");
    }
}
