import { onMounted, onUnmounted, ref, watch, nextTick, type Ref, type WatchSource } from 'vue';

export interface DynamicTableHeightOptions {
  /// 表格容器元素的 ref —— 用于测量表格 div 在 main.app 内的垂直偏移
  tableEl: Ref<HTMLElement | null>;
  /// 最小高度(像素)。低于这个值不缩,避免数据多时被压扁
  minHeight?: number;
  /// 底部预留(像素):横向滚动条 + border + 容错
  bottomPadding?: number;
  /// 依赖数组。任一变化(数据量、tab 切换等)都会 nextTick 后重算一次
  deps?: WatchSource[];
  /// 首次渲染前的占位高度。挂载后会立刻被重算覆盖
  initialHeight?: string;
}

/// 页面级 DataTable 自适应高度:
/// - 计算 = max(minHeight, main.app 可用高 - 表格 div 顶部偏移 - bottomPadding)
/// - 监听 main.app 尺寸变化(窗口 resize、侧边栏折叠等)实时重算
/// - deps 变化(如数据量变更、tab 切换)时 nextTick 后重算一次
///
/// 弹窗内 DataTable 不要用这个 —— 弹窗尺寸相对稳定,直接传静态
/// `:max-height="'600px'"` 字符串即可,不需要动态监听。
export function useDynamicTableHeight(opts: DynamicTableHeightOptions) {
  const minH = opts.minHeight ?? 300;
  const pad = opts.bottomPadding ?? 48;
  const maxHeight = ref<string>(opts.initialHeight ?? '420px');
  let observer: ResizeObserver | null = null;

  function recalc() {
    const main = document.querySelector('main.app') as HTMLElement | null;
    const el = opts.tableEl.value;
    if (main === null || el === null) return;
    const mainRect = main.getBoundingClientRect();
    const tableRect = el.getBoundingClientRect();
    const available = main.clientHeight - (tableRect.top - mainRect.top);
    maxHeight.value = `${Math.max(minH, available - pad)}px`;
  }

  onMounted(async () => {
    await nextTick();
    recalc();
    const main = document.querySelector('main.app');
    if (main !== null) {
      observer = new ResizeObserver(() => recalc());
      observer.observe(main);
    }
  });

  onUnmounted(() => {
    if (observer !== null) {
      observer.disconnect();
      observer = null;
    }
  });

  if (opts.deps !== undefined && opts.deps.length > 0) {
    watch(opts.deps, () => {
      void nextTick(() => recalc());
    });
  }

  return { maxHeight };
}
