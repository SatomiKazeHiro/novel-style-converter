<template>
  <header class="nav">
    <nav class="crumbs">
      <router-link to="/data-assets">数据资产</router-link>
      <span class="sep">/</span>
      <span class="crumb-static">第 {{ currentIdx + 1 }} 章</span>
    </nav>
    <div class="title-row">
      <h2>{{ chapter?.title || '加载中...' }}</h2>
      <span class="badge">{{ totalChapters }} 章</span>
      <Button size="small" :disabled="!canGoPrev" @click="$emit('prev')">◀</Button>
      <Button size="small" :disabled="!canGoNext" @click="$emit('next')">▶</Button>
    </div>
  </header>
</template>

<script setup lang="ts">
import Button from './ui/Button.vue';
import type { Chapter } from '../ipc/types';

defineProps<{
  chapter: Chapter | null;
  currentIdx: number;
  totalChapters: number;
  canGoPrev: boolean;
  canGoNext: boolean;
}>();
defineEmits<{ prev: []; next: [] }>();
</script>

<style scoped>
.nav { padding-bottom: 12px; border-bottom: 1px solid var(--border-color); }
.crumbs { font-size: 12px; color: var(--text-secondary); margin-bottom: 8px; }
.crumbs a { color: var(--text-secondary); text-decoration: none; }
.crumbs a:hover { color: var(--color-cinnabar); }
.sep { margin: 0 6px; color: var(--text-muted); }
.crumb-static { color: var(--text-primary); }
.title-row { display: flex; align-items: center; gap: 12px; }
.title-row h2 { margin: 0; flex: 1; font-size: 16px; font-weight: var(--font-weight-medium); }
.badge {
  padding: 2px 8px; background: var(--bg-hover); border-radius: 4px;
  font-size: 12px; color: var(--text-secondary);
}
</style>
