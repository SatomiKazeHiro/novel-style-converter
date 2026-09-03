import { nextTick, onMounted, onUnmounted, ref } from 'vue';

// 单一 anchor / text / 可见性,模块级 singleton —— TooltipHost 渲染 + 所有触发器共享。
// 之前在模块顶层 addEventListener 三件套,有挂无卸(HMR 切换 TooltipHost 时监听器会累加)。
// 现改为 setup-style:onMounted 注册、onUnmounted 移除;TooltipHost.vue 是唯一调用方,生命周期正确配对。
const text = ref('');
const visible = ref(false);
const x = ref(0);
const y = ref(0);

const mouseX = ref(0);
const mouseY = ref(0);
let pendingAnchor: HTMLElement | null = null;
let pendingText = '';

async function place() {
  const tipEl = document.getElementById('app-tooltip-host');
  if (!tipEl || !pendingAnchor) return;
  // 等一拍:DOM 中的 text 还没刷新时,tipRect 是旧值,会算出错误位置
  await nextTick();
  // await 期间可能被 hide() 把 pendingAnchor 清掉,不能继续写状态
  if (!pendingAnchor) return;

  const tipRect = tipEl.getBoundingClientRect();
  // 期望:tooltip 水平居中于鼠标,底部距鼠标 10px(鼠标上方)
  const desiredX = mouseX.value;
  const desiredY = mouseY.value - 10 - tipRect.height;

  const margin = 8;
  const maxX = window.innerWidth - margin - tipRect.width;
  const maxY = window.innerHeight - margin - tipRect.height;

  // clamp(lo, v, hi):最小居中坐标 = margin + 半宽;最大居中坐标 = innerWidth - margin - 半宽
  x.value = Math.min(Math.max(desiredX, margin + tipRect.width / 2), maxX + tipRect.width / 2);
  y.value = Math.min(Math.max(desiredY, margin), maxY);

  visible.value = true;
}

function onMouseMove(e: MouseEvent) {
  mouseX.value = e.clientX;
  mouseY.value = e.clientY;
  if (visible.value) void place();
}

function onScrollOrResize() {
  if (visible.value) void place();
}

export function useTooltip() {
  // tooltip 跟随鼠标 + 滚动 / 窗口尺寸变化时重定位;挂载期注册、卸载时清理。
  // 注意:必须在 setup 顶层调用 —— 当前唯一调用方 TooltipHost.vue 满足此条件。
  onMounted(() => {
    if (typeof window === 'undefined') return;
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('scroll', onScrollOrResize, true);
    window.addEventListener('resize', onScrollOrResize);
  });
  onUnmounted(() => {
    if (typeof window === 'undefined') return;
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('scroll', onScrollOrResize, true);
    window.removeEventListener('resize', onScrollOrResize);
  });

  function show(t: string, anchor: HTMLElement) {
    pendingText = t;
    pendingAnchor = anchor;
    text.value = t;
    // 立即定位:不靠 enterTimer 延迟,让 tooltip 直接出现在鼠标处
    void place();
  }

  function hide() {
    // 立即清掉 anchor / text,任何在飞的 scroll/resize 都会被 ignore
    pendingAnchor = null;
    pendingText = '';
    // 不能把 x/y 重置 0 —— TooltipHost 的 transition 只覆盖 opacity,
    // 重置位置会让 tooltip 在 120ms 渐隐期间瞬时跳到 (0,0),看起来是左上角闪一下。
    // 不可见时 x/y 值无意义,下次 show() 走 place() 重算。
    visible.value = false;
  }

  return { text, visible, x, y, show, hide };
}

export type TooltipApi = ReturnType<typeof useTooltip>;
