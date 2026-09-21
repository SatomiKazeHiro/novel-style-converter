/// 产出比例护栏 —— 前端镜像，用于在工作流详情表里把"越界"的章节标出来。
///
/// 阈值与 `crates/nsc-core/src/transformer/ratio_guard.rs` 一一对应（改那边要同步改这里）：
/// - 压缩：<15% 说明情节被删（压缩 ≠ 摘要）；>90% 说明基本没删，任务未生效。
/// - 文风：<50% 是被缩写成梗概；>200% 是注水/跑飞。
///
/// 为什么这里是**重复定义**而不是从后端取：这套阈值只用于 UI 上的一处视觉标记，
/// 权威判定仍由后端做（`ratio_note` 落 `ai_call_logs`，AiCalls 页展示）。
/// 为它加一个 IPC 接口的复杂度不划算；代价是两边要同步，故在此显式标注来源。
export type TransformMode = 'compress' | 'style';

export interface RatioBand {
  floorPct: number;
  ceilingPct: number;
}

export const COMPRESS_BAND: RatioBand = { floorPct: 15, ceilingPct: 90 };
export const STYLE_BAND: RatioBand = { floorPct: 50, ceilingPct: 200 };

export function bandOf(mode: TransformMode): RatioBand {
  return mode === 'compress' ? COMPRESS_BAND : STYLE_BAND;
}

/// 结果占原文的百分比（≠ 字变率：这里是"还剩多少"，100% 表示等长）。
/// 原文 0 字或结果为空 → null（没有可比对象）。
export function ratioPercent(source: number, result: number | null): number | null {
  if (result === null || source <= 0) return null;
  return (result / source) * 100;
}

/// 越界说明；在护栏内返回 null。
/// 文案与后端 `ratio_guard::ratio_note` 保持同一套说法，避免同一个越界在两处说法不同。
///
/// 百分比保留 1 位小数（后端 `ratio_guard::SCALE` 同为 10）：越界判定用的是**原始**比例，
/// 若显示时四舍五入成整数，14.9% 会渲染成"15% 低于护栏 15%"，看着像自相矛盾。
export function guardNote(mode: TransformMode, source: number, result: number | null): string | null {
  const pct = ratioPercent(source, result);
  if (pct === null) return null;
  const { floorPct, ceilingPct } = bandOf(mode);
  const what = mode === 'compress' ? '压缩' : '改写';
  const shown = (Math.round(pct * 10) / 10).toFixed(1);
  if (pct < floorPct) {
    return `${what}后仅剩原文 ${shown}%(低于护栏 ${floorPct}%),疑似丢失情节`;
  }
  if (pct > ceilingPct) {
    return `${what}后为原文 ${shown}%(高于护栏 ${ceilingPct}%),任务可能未生效`;
  }
  return null;
}
