/// 转义正则特殊字符(保留,给别的调用方用)。
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${()|[\]\\]/g, '\\$&');
}

/// 不可见字符(空白 + ZWSP/BOM/WJ 等 format characters)正则 ——
/// 用于在已知会被 strip 的前后缀上做宽松比较(如 raw 与 splitted segLines
/// 一侧带不可见字符)。
export const INVIS_PREFIX_RE = /^[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+/u;
export const INVIS_SUFFIX_RE = /[\s\u{200B}\u{200C}\u{200D}\u{FEFF}\u{2060}]+$/u;

/// 清理首尾 invisible/whitespace 后是否为空 —— 空行/纯不可见字符视为视觉空行。
export function isVisuallyEmptyLine(line: string): boolean {
  return line.replace(INVIS_PREFIX_RE, '').replace(INVIS_SUFFIX_RE, '') === '';
}

/// zh-aware 字数简化版:Chinese 全角字符 + 英文/数字连续段都计 1。
/// 真正的 word_count 后端按 byte range 切片时精算,这里只用于 UI 列表显示。
export function countChapterChars(s: string): number {
  let n = 0;
  for (const ch of s) if (!/\s/.test(ch)) n++;
  return n;
}

/// 去掉首尾 whitespace + invisible 格式字符。
export function stripInvisibles(s: string): string {
  return s.replace(INVIS_PREFIX_RE, '').replace(INVIS_SUFFIX_RE, '');
}

/// 只去掉末尾的 whitespace + invisible(内容前导空白保留,与 splitter trim 语义对齐)。
export function stripTrailingInvisibles(s: string): string {
  return s.replace(INVIS_SUFFIX_RE, '');
}
