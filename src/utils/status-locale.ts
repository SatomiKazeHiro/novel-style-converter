/// 状态枚举值的中文映射 —— 后端 enum 与 UI 展示的中间层。
///
/// 为什么放 utils 不放 composables:
/// composables 暗示会用到响应式 ref / 副作用。
/// 这两个函数是 pure switch —— 同样的输入同样的输出,无副作用。
///
/// WorkflowStatus / TransformStatus 是后端 enum 的 TS 类型(见 ipc/types.ts),
/// 这里集中维护所有 enum 值的展示字符串。未知值 throw(per CLAUDE.md "不兜底 fallback"),
/// default 分支用 `const _: never = s` 强制 TS 穷举,新增 enum 值未加 case 时编译期就报错。
import type { WorkflowStatus, TransformStatus } from '../ipc/types';

/// 章节 status 中文映射 —— 后端 TransformStatus 6 值。
/// skipped 是失败策略 = skip_failed 或 paused 时用户显式跳过,cancelled 是终止运行。
/// 所有 6 值必须映射,否则编译期 exhaustive check 失败。
export function formatChapterStatus(s: TransformStatus): string {
  switch (s) {
    case 'pending':   return '待处理';
    case 'running':   return '转换中';
    case 'done':      return '已完成';
    case 'failed':    return '失败';
    case 'skipped':   return '已跳过';
    case 'cancelled': return '已取消';
    default: {
      const _exhaustive: never = s;
      throw new Error(`unknown TransformStatus: ${String(s)}`);
    }
  }
}

/// 工作流 status 中文映射 —— 后端 BatchStatus 7 值。
/// paused 是失败策略 = pause_and_review 时批停在等用户决策;
/// completed/terminated/cancelled 是最终终态。所有 7 值必须映射,
/// 否则编译期 exhaustive check 失败。
export function formatBatchStatus(s: WorkflowStatus): string {
  switch (s) {
    case 'pending':    return '待处理';
    case 'running':    return '转换中';
    case 'stopped':    return '已停止';
    case 'paused':     return '已暂停';
    case 'completed':  return '已完成';
    case 'terminated': return '已终止';
    case 'cancelled':  return '已取消';
    default: {
      const _exhaustive: never = s;
      throw new Error(`unknown WorkflowStatus: ${String(s)}`);
    }
  }
}
