/// 模块级弹窗 z-index 计数器 —— 所有 Dialog 实例共享同一递增序列。
///
/// 关键点:必须放在 Dialog.vue 的 <script setup> 之外,否则每个 Dialog 实例
/// 会得到独立的 stack,后打开的弹窗与先打开的弹窗 z-index 撞值,
/// DOM 顺序决定层级,导致嵌套弹窗被父弹窗遮挡。
///
/// 上限 9999 后回卷到 1001,防止长期使用 z-index 无限增长。
let stack = 1000;

export function nextStack(): number {
  stack += 1;
  if (stack > 9999) stack = 1001;
  return stack;
}