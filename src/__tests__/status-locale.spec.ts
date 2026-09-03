import { describe, it, expect } from 'vitest';
import { formatBatchStatus, formatChapterStatus } from '../utils/status-locale';

describe('formatBatchStatus', () => {
  it('formats pending as 待处理', () => {
    expect(formatBatchStatus('pending')).toBe('待处理');
  });
  it('formats running as 转换中', () => {
    expect(formatBatchStatus('running')).toBe('转换中');
  });
  it('formats stopped as 已停止', () => {
    expect(formatBatchStatus('stopped')).toBe('已停止');
  });
  it('formats paused as 已暂停', () => {
    expect(formatBatchStatus('paused')).toBe('已暂停');
  });
  it('formats completed as 已完成', () => {
    expect(formatBatchStatus('completed')).toBe('已完成');
  });
  it('formats terminated as 已终止', () => {
    expect(formatBatchStatus('terminated')).toBe('已终止');
  });
  it('formats cancelled as 已取消', () => {
    expect(formatBatchStatus('cancelled')).toBe('已取消');
  });
  it('throws on unknown status (fail-fast, no fallback)', () => {
    // Per CLAUDE.md: no silent fallbacks. Unknown value must throw.
    expect(() => formatBatchStatus('foo' as any)).toThrow();
  });
});

describe('formatChapterStatus', () => {
  it('formats pending as 待处理', () => {
    expect(formatChapterStatus('pending')).toBe('待处理');
  });
  it('formats running as 转换中', () => {
    expect(formatChapterStatus('running')).toBe('转换中');
  });
  it('formats done as 已完成', () => {
    expect(formatChapterStatus('done')).toBe('已完成');
  });
  it('formats failed as 失败', () => {
    expect(formatChapterStatus('failed')).toBe('失败');
  });
  it('formats skipped as 已跳过', () => {
    expect(formatChapterStatus('skipped')).toBe('已跳过');
  });
  it('formats cancelled as 已取消', () => {
    expect(formatChapterStatus('cancelled')).toBe('已取消');
  });
  it('throws on unknown status (fail-fast, no fallback)', () => {
    // Per CLAUDE.md: no silent fallbacks. Unknown value must throw.
    expect(() => formatChapterStatus('foo' as any)).toThrow();
  });
});