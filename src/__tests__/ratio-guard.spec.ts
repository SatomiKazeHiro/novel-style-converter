import { describe, it, expect } from 'vitest';
import { bandOf, guardNote, ratioPercent, COMPRESS_BAND, STYLE_BAND } from '../utils/ratio-guard';

/// 前端护栏镜像 —— 阈值必须与 crates/nsc-core/src/transformer/ratio_guard.rs 一致。
/// 这里重点测**边界闭区间**：后端 `band_edges_are_inclusive` 断言"正好 15% / 90% 算正常，
/// 越 1 个字符才报"，前端若用开区间就会与后端判定不一致（同一章一边标警示一边不标）。
describe('ratioPercent', () => {
  it('结果占原文的百分比（100% = 等长）', () => {
    expect(ratioPercent(1000, 300)).toBeCloseTo(30);
    expect(ratioPercent(1000, 1000)).toBeCloseTo(100);
  });

  it('原文 0 字或结果为空 → null', () => {
    expect(ratioPercent(0, 500)).toBeNull();
    expect(ratioPercent(1000, null)).toBeNull();
  });
});

describe('guardNote 压缩模式（15%–90%）', () => {
  it('护栏内不报', () => {
    expect(guardNote('compress', 1000, 300)).toBeNull();
    expect(guardNote('compress', 1000, 500)).toBeNull();
    expect(guardNote('compress', 1000, 600)).toBeNull();
  });

  it('边界是闭区间：正好 15% / 90% 算正常', () => {
    expect(guardNote('compress', 1000, 150)).toBeNull();
    expect(guardNote('compress', 1000, 900)).toBeNull();
  });

  it('越界 1 个字符即报；显示保留 1 位小数，避免"15% 低于护栏 15%"这种自相矛盾', () => {
    const low = guardNote('compress', 1000, 149);
    expect(low).toContain('14.9%'); // 原始比例 14.9% < 15%，四舍五入会显示成 15%
    expect(low).toContain('15%');

    const high = guardNote('compress', 1000, 901);
    expect(high).toContain('90.1%');
    expect(high).toContain('90%');
  });

  it('压得过分（只剩 8%）要被标出来 —— 这正是用户要抓的"不正常"', () => {
    const note = guardNote('compress', 10_000, 800);
    expect(note).toContain('8.0%');
    expect(note).toContain('丢失情节');
  });
});

describe('guardNote 文风模式（50%–200%）', () => {
  it('同一产出比例在两档护栏下判定不同：30% 对压缩正常、对文风越界', () => {
    expect(guardNote('compress', 1000, 300)).toBeNull();
    expect(guardNote('style', 1000, 300)).not.toBeNull();
  });

  it('文风下限 50%：60% 通过、40% 越界', () => {
    expect(guardNote('style', 1000, 600)).toBeNull();
    expect(guardNote('style', 1000, 400)).not.toBeNull();
  });

  it('文风上限 200%：190% 通过、250% 越界', () => {
    expect(guardNote('style', 1000, 1900)).toBeNull();
    expect(guardNote('style', 1000, 2500)).not.toBeNull();
  });
});

describe('bandOf', () => {
  it('两种模式的区间互不相同', () => {
    expect(bandOf('compress')).toEqual(COMPRESS_BAND);
    expect(bandOf('style')).toEqual(STYLE_BAND);
    expect(bandOf('compress').floorPct).toBeLessThan(bandOf('style').floorPct);
  });
});
