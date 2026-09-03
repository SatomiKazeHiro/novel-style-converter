<template>
  <div class="compare">
    <div class="pane">
      <div class="pane-title">原文 ({{ originalWordCount }} 字)</div>
      <pre ref="leftRef" class="content" @scroll="onScroll('left', $event)">{{ original }}</pre>
    </div>
    <div class="pane">
      <div class="pane-title">
        {{ selectedVersionLabel }} ({{ transformedWordCount }} 字)
      </div>
      <div v-if="status === 'failed'" class="alert error">{{ error || '转换失败' }}</div>
      <div v-else-if="status === 'pending' || status === 'running'" class="alert info">
        转换中... 状态: {{ status }}
      </div>
      <pre
        v-else
        ref="rightRef"
        class="content"
        @scroll="onScroll('right', $event)"
      >{{ transformed }}</pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';

const props = defineProps<{
  original: string;
  transformed: string;
  status: string | null | undefined;
  error: string | null | undefined;
  selectedVersionLabel: string;
}>();

const leftRef = ref<HTMLPreElement | null>(null);
const rightRef = ref<HTMLPreElement | null>(null);
let scrollLockUntil = 0;
const SCROLL_GUARD_MS = 50;

const originalWordCount = computed(() => [...props.original].length);
const transformedWordCount = computed(() => [...props.transformed].length);

function onScroll(side: 'left' | 'right', e: Event) {
  const now = Date.now();
  if (now < scrollLockUntil) return;
  const src = e.target as HTMLElement;
  const dst = side === 'left' ? rightRef.value : leftRef.value;
  if (!dst) return;
  const maxSrc = src.scrollHeight - src.clientHeight;
  const maxDst = dst.scrollHeight - dst.clientHeight;
  if (maxSrc <= 0 || maxDst <= 0) return;
  const ratio = src.scrollTop / maxSrc;
  scrollLockUntil = now + SCROLL_GUARD_MS;
  dst.scrollTop = ratio * maxDst;
}
</script>

<style scoped>
.compare { display: flex; gap: 16px; flex: 1; min-height: 0; }
.pane {
  flex: 1; width: 50%; display: flex; flex-direction: column;
  background: var(--color-sheet); border: 1px solid var(--border-color);
  border-radius: var(--radius-pin); overflow: hidden;
}
.pane-title {
  padding: 8px 12px; border-bottom: 1px solid var(--border-color);
  font-size: 13px; color: var(--text-secondary); flex-shrink: 0;
}
.content {
  flex: 1; margin: 0; padding: 12px;
  font-family: ui-monospace, monospace; font-size: 13px; line-height: 1.6;
  white-space: pre-wrap; word-break: break-word;
  overflow-y: auto; color: var(--text-primary);
}
.alert {
  flex: 1; display: flex; align-items: center; justify-content: center;
  margin: 0; padding: 16px; font-size: 13px;
}
.alert.error { color: var(--color-cinnabar-deep); background: var(--bg-hover); }
.alert.info { color: var(--text-secondary); background: var(--bg-hover); }
</style>
