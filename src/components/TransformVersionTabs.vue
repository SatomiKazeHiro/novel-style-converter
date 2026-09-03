<template>
  <div v-if="transformations.length > 0" class="tabs">
    <button
      v-for="(t, i) in transformations"
      :key="t.id"
      class="tab"
      :class="{ active: t.id === selectedId, [t.status]: true }"
      :title="tabTitle(t, i)"
      @click="$emit('select', t.id)"
    >
      <span class="v-label">v{{ transformations.length - i }}</span>
      <span class="v-status">{{ statusGlyph(t.status) }}</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import type { TransformationChapterRow } from '../ipc/types';

defineProps<{
  transformations: TransformationChapterRow[];
  selectedId: number | null;
}>();
defineEmits<{ select: [number] }>();

function statusGlyph(s: string): string {
  switch (s) {
    case 'done': return '✓';
    case 'failed': return '✗';
    case 'pending': return '…';
    case 'running': return '⟳';
    case 'cancelled': return '—';
    default: return '?';
  }
}

function tabTitle(t: TransformationChapterRow, i: number): string {
  const idx = i + 1;
  const stamp = t.completed_at ?? t.started_at ?? '';
  return `v${idx} · ${t.mode} · ${t.status}${stamp ? ` · ${stamp}` : ''}`;
}
</script>

<style scoped>
.tabs { display: flex; gap: 4px; flex-wrap: wrap; padding: 8px 0; }
.tab {
  display: inline-flex; align-items: center; gap: 6px;
  height: 30px; padding: 0 12px;
  border: 1px solid var(--border-color); border-radius: var(--radius-pin);
  background: var(--color-sheet); color: var(--text-secondary);
  font-size: 13px; font-family: inherit; cursor: pointer;
  position: relative; transition: background 0.1s, color 0.1s;
}
.tab:hover { background: var(--bg-hover); color: var(--text-primary); }
.tab.active {
  background: var(--color-cinnabar-light); color: var(--color-cinnabar-deep);
  font-weight: var(--font-weight-medium); border-color: var(--color-cinnabar);
}
.tab.active::before {
  content: ''; position: absolute; left: -1px; top: 6px; bottom: 6px;
  width: 2px; background: var(--color-cinnabar);
}
.tab.failed { color: var(--danger); border-color: var(--danger-border); }
.tab.cancelled { text-decoration: line-through; opacity: 0.6; }
.v-label { font-variant-numeric: tabular-nums; }
.v-status { font-size: 12px; }
</style>
