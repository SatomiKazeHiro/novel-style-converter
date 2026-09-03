<template>
  <footer class="footer">
    <div v-if="status === 'failed' && error" class="alert error">
      ✗ {{ error }}
    </div>
    <div v-else class="row">
      <div class="meta">
        <span class="muted">prompt #{{ promptId }}</span>
        <span class="dot">·</span>
        <span class="muted">model #{{ modelConfigId }}</span>
        <span v-if="completedAt" class="dot">·</span>
        <span v-if="completedAt" class="muted">{{ completedAt }}</span>
      </div>
      <div class="stats">
        <span>tokens {{ tokensIn ?? '—' }} in / {{ tokensOut ?? '—' }} out</span>
        <span class="dot">·</span>
        <span class="status">{{ statusLabel }}</span>
      </div>
      <Button kind="primary" :disabled="!canRetransform" @click="$emit('retransform')">
        ⚙ 重新转换
      </Button>
    </div>
  </footer>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import Button from './ui/Button.vue';

const props = defineProps<{
  status: string | null | undefined;
  error: string | null | undefined;
  tokensIn: number | null | undefined;
  tokensOut: number | null | undefined;
  promptId: number;
  modelConfigId: number;
  completedAt: string | null | undefined;
}>();
defineEmits<{ retransform: [] }>();

const statusLabel = computed(() => {
  switch (props.status) {
    case 'done': return '✓ Done';
    case 'failed': return '✗ Failed';
    case 'pending': return '… Pending';
    case 'running': return '⟳ Running';
    case 'cancelled': return '— Cancelled';
    default: return '—';
  }
});

const canRetransform = computed(() => props.status !== 'running' && props.status !== 'pending');
</script>

<style scoped>
.footer { padding-top: 12px; border-top: 1px solid var(--border-color); }
.row {
  display: flex; align-items: center; gap: 12px;
}
.meta { flex: 1; display: flex; gap: 6px; align-items: center; font-size: 12px; }
.stats { display: flex; gap: 6px; align-items: center; font-size: 13px; font-variant-numeric: tabular-nums; }
.muted { color: var(--text-secondary); }
.dot { color: var(--text-muted); }
.status { font-weight: var(--font-weight-medium); }
.alert {
  padding: 8px 12px; background: var(--bg-hover); color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin); font-size: 13px; margin-bottom: 8px;
}
</style>
