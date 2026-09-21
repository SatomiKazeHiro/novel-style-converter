import { describe, it, expect } from 'vitest';
import { deltaPercent, formatDeltaPercent } from '../utils/format';

/// 字变率（工作流详情表格的「字变率」列）。
/// 重点在"算不出来"与"看起来像 bug"两类边界 —— 它们决定 UI 显示 `—` 还是数字。
describe('deltaPercent', () => {
  it('变短为负、变长为正', () => {
    expect(deltaPercent(1000, 300)).toBeCloseTo(-70);
    expect(deltaPercent(1000, 1500)).toBeCloseTo(50);
  });

  it('等长是 0（这是"没变化"，不是"算不出来"）', () => {
    expect(deltaPercent(1000, 1000)).toBe(0);
  });

  it('结果槽为空 → null（没有可比对象）', () => {
    expect(deltaPercent(1000, null)).toBeNull();
  });

  it('原文 0 字 → null（没有分母，不能当作 0%）', () => {
    expect(deltaPercent(0, 500)).toBeNull();
    expect(deltaPercent(0, null)).toBeNull();
  });
});

describe('formatDeltaPercent', () => {
  it('正数带显式 + 号，负数带 - 号', () => {
    expect(formatDeltaPercent(12.4)).toBe('+12%');
    expect(formatDeltaPercent(-38.6)).toBe('-39%');
  });

  it('零不带符号', () => {
    expect(formatDeltaPercent(0)).toBe('0%');
  });

  it('四舍五入到 0 时不能渲染成 "-0%" / "+0%"', () => {
    // Math.round(-0.4) === -0，直接插值会得到 "-0%" —— 看着像 bug。
    expect(formatDeltaPercent(-0.4)).toBe('0%');
    expect(formatDeltaPercent(0.4)).toBe('0%');
  });

  it('恰好 -0.5 走 Math.round 的 -0 分支，同样归零号', () => {
    expect(formatDeltaPercent(-0.5)).toBe('0%');
  });
});
