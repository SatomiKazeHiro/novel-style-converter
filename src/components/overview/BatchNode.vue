<template>
  <div class="ov-card" :class="`batch batch-${statusClass}`">
    <div class="ov-kind"><IconRefreshCw :size="14" :stroke-width="2.2" /> 工作流</div>
    <div class="ov-title">{{ data.title }}</div>
    <div class="ov-row">
      <span class="status-pill" v-if="data.status">{{ formatBatchStatus(data.status) }}</span>
      <span class="ov-meta" v-if="data.meta">{{ data.meta }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import IconRefreshCw from '~icons/lucide/refresh-cw';
import { formatBatchStatus } from '../../utils/status-locale';
import type { WorkflowStatus } from '../../ipc/types';

const props = defineProps<{ data: { title: string; status?: WorkflowStatus | null; meta?: string | null } }>();

const STATUS_MAP: Record<string, string> = {
  running: 'running',
  paused: 'paused',
  stopped: 'stopped',
  terminated: 'stopped',
  cancelled: 'stopped',
  completed: 'completed',
  pending: 'pending',
};

const statusClass = computed(() => STATUS_MAP[props.data.status ?? ''] ?? 'default');
</script>

<style scoped>
.ov-card {
  width: 240px;
  min-height: 92px;
  border-radius: 12px;
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-family: -apple-system, system-ui, sans-serif;
  box-shadow: 0 6px 14px -8px rgba(15, 23, 42, 0.35);
  border: 1.5px solid;
  border-top: 4px solid;
  transition: transform 120ms ease, box-shadow 120ms ease;
  cursor: pointer;
}
.ov-card:hover { transform: translateY(-1px); box-shadow: 0 10px 22px -10px rgba(15, 23, 42, 0.45); }
.ov-kind {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  opacity: 0.85;
}
.ov-title {
  font-size: 15px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ov-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ov-meta {
  font-size: 12px;
  opacity: 0.8;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.status-pill {
  font-size: 11px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.22);
}

/* batch 默认色(无 status) */
.batch {
  background: linear-gradient(180deg, #8B5CF6 0%, #6D28D9 100%);
  border-color: #5B21B6;
  border-top-color: #4C1D95;
  color: #FFFFFF;
}
.batch .ov-kind { color: #DDD6FE; }
.batch .ov-meta { color: #DDD6FE; }

/* running:亮蓝 */
.batch-running {
  background: linear-gradient(180deg, #60A5FA 0%, #2563EB 100%);
  border-color: #1D4ED8;
  border-top-color: #1E3A8A;
}
.batch-running .ov-kind { color: #DBEAFE; }
.batch-running .ov-meta { color: #DBEAFE; }

/* paused:亮黄(深字) */
.batch-paused {
  background: linear-gradient(180deg, #FCD34D 0%, #F59E0B 100%);
  border-color: #B45309;
  border-top-color: #92400E;
  color: #78350F;
}
.batch-paused .ov-kind { color: #78350F; opacity: 0.85; }
.batch-paused .ov-meta { color: #78350F; opacity: 0.75; }
.batch-paused .status-pill { background: rgba(120, 53, 15, 0.15); color: #78350F; }

/* stopped / terminated / cancelled:亮红 */
.batch-stopped {
  background: linear-gradient(180deg, #F87171 0%, #DC2626 100%);
  border-color: #B91C1C;
  border-top-color: #991B1B;
}
.batch-stopped .ov-kind { color: #FEE2E2; }
.batch-stopped .ov-meta { color: #FEE2E2; }

/* completed:亮绿(深字) */
.batch-completed {
  background: linear-gradient(180deg, #34D399 0%, #10B981 100%);
  border-color: #047857;
  border-top-color: #065F46;
  color: #064E3B;
}
.batch-completed .ov-kind { color: #064E3B; opacity: 0.85; }
.batch-completed .ov-meta { color: #064E3B; opacity: 0.75; }
.batch-completed .status-pill { background: rgba(6, 78, 59, 0.12); color: #064E3B; }
</style>