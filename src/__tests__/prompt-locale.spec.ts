import { describe, it, expect } from 'vitest';
import { formatPromptKind } from '../utils/prompt-locale';

describe('formatPromptKind', () => {
  it('formats compress as 压缩', () => {
    expect(formatPromptKind('compress')).toBe('压缩');
  });

  it('formats style as 文风转换', () => {
    expect(formatPromptKind('style')).toBe('文风转换');
  });

  it('throws on unknown kind (fail-fast, no fallback)', () => {
    // Per CLAUDE.md: no silent fallbacks. Unknown value must throw.
    expect(() => formatPromptKind('unknown' as any)).toThrow();
  });
});