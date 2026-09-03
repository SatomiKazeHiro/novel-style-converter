import { describe, expect, it } from 'vitest';
import {
  countChapterChars,
  isVisuallyEmptyLine,
  stripInvisibles,
  stripTrailingInvisibles,
} from '../utils/splitChapters';

describe('isVisuallyEmptyLine', () => {
  it('plain whitespace counts as visually empty', () => {
    expect(isVisuallyEmptyLine('')).toBe(true);
    expect(isVisuallyEmptyLine('   ')).toBe(true);
    expect(isVisuallyEmptyLine('\t\n')).toBe(true);
  });
  it('ZWSP / BOM only counts as visually empty', () => {
    expect(isVisuallyEmptyLine('\u{200B}')).toBe(true);
    expect(isVisuallyEmptyLine('\u{FEFF}')).toBe(true);
    expect(isVisuallyEmptyLine('  \u{200B}\u{FEFF}  ')).toBe(true);
  });
  it('non-empty content is not visually empty', () => {
    expect(isVisuallyEmptyLine('  正文  ')).toBe(false);
    expect(isVisuallyEmptyLine('\u{200B}a')).toBe(false);
  });
});

describe('countChapterChars', () => {
  it('whitespace is not counted', () => {
    expect(countChapterChars('  a b\nc')).toBe(3);
  });
  it('empty string returns 0', () => {
    expect(countChapterChars('')).toBe(0);
  });
  it('chinese + english + digits', () => {
    expect(countChapterChars('你好 world 123')).toBe(10);
  });
});

describe('stripInvisibles / stripTrailingInvisibles', () => {
  it('stripInvisibles trims leading + trailing whitespace and invisibles', () => {
    // 覆盖所有 5 种 invisible + ZWSP + BOM + 全角空格 + LF
    expect(stripInvisibles('  \u{200B}\u{FEFF}\u{200C}\u{200D}\u{2060}第1章\u{3000}\n')).toBe('第1章');
  });
  it('stripTrailingInvisibles preserves leading whitespace', () => {
    // 保留前导空格,只 trim 末尾的 \n
    expect(stripTrailingInvisibles('  正文\n\n')).toBe('  正文');
  });
  it('stripTrailingInvisibles trims trailing invisibles (ZWSP/BOM/WJ)', () => {
    // 这是与 trimEnd 的关键区别:覆盖 ZWSP/BOM/WJ
    expect(stripTrailingInvisibles('正文\u{200B}\u{FEFF}\u{2060}')).toBe('正文');
  });
});
