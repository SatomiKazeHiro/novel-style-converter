import { shallowRef } from 'vue';

/// 全局确认对话框 —— 模块级单例状态 + 一个渲染宿主(ConfirmHost.vue)。
///
/// 为什么是全局服务而不是每个组件摆一份 `ConfirmDialog`：
/// - 调用方不再需要自己维护 `open` ref、模板插槽、`@confirm` 回调三件套；
///   一次 `await confirmDialog({...})` 就是一次决策，**业务逻辑读起来是线性的**。
/// - 确认框与业务解耦：store / 工具函数也能直接弹确认，不必把 UI 状态往上提。
/// - 全应用只有一处渲染，样式与措辞统一（旧代码里 15 处 `ConfirmDialog` + 6 处
///   `AlertDialog` 各自维护 open 状态，是这套方案要收掉的东西；旧调用方暂不动）。
///
/// 实现照 `composables/useTooltip.ts` 的既有约定：模块级 state + 单一 Host。
export interface ConfirmRequest {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  /// danger 用于不可逆操作（删除 / 放弃 / 覆盖），确认按钮走红色。
  kind?: 'default' | 'danger';
  /// true = 纯提示，只渲染一个"知道了"按钮（对应旧的 AlertDialog）。
  alertOnly?: boolean;
}

const current = shallowRef<ConfirmRequest | null>(null);
let resolveCurrent: ((ok: boolean) => void) | null = null;

/// 打开确认框，resolve 为 true 表示用户点了确认，false 表示取消 / 关闭 / 遮罩。
/// 调用方典型写法：`if (!(await confirmDialog({...}))) return;`
export function confirmDialog(req: ConfirmRequest): Promise<boolean> {
  // 宿主没挂（App.vue 忘了放 ConfirmHost）时立刻失败，避免 await 永久挂住 ——
  // 那种症状很像"点了没反应"，正是这套服务要消灭的东西。
  if (current.value) {
    // 已有确认框在等：按"用户取消"结束前一个，再开新的，避免前一个 await 永远悬空。
    resolveCurrent?.(false);
  }
  return new Promise<boolean>((resolve) => {
    resolveCurrent = resolve;
    current.value = req;
  });
}

/// 供 ConfirmHost 使用：当前请求 + 关闭动作。
export function useConfirmHost() {
  function settle(ok: boolean): void {
    const resolve = resolveCurrent;
    resolveCurrent = null;
    current.value = null;
    resolve?.(ok);
  }
  return { current, settle };
}

/// 只有一个"知道了"按钮的提示框（对应旧的 AlertDialog）。
/// 与 confirmDialog 共用同一个宿主与队列，只是不渲染取消按钮。
export function alertDialog(req: Omit<ConfirmRequest, 'cancelText' | 'alertOnly'>): Promise<boolean> {
  return confirmDialog({ ...req, cancelText: '知道了', alertOnly: true });
}

export type ConfirmApi = typeof confirmDialog;
