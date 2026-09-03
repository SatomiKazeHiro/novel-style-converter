<template>
  <section>
    <PageHeader title="AI 调用" subtitle="每次 LLM chat 调用的元数据 / token / 预览 / 错误">
      <template #actions>
        <Button :disabled="loading" @click="reload()">刷新</Button>
        <Button kind="danger" :disabled="logs.length === 0" @click="onClear">清空</Button>
      </template>
    </PageHeader>

    <div class="toolbar">
      <label class="filter">
        <span>业务</span>
        <select :value="filter.business ?? ''" @change="onBusinessChange(($event.target as HTMLSelectElement).value)">
          <option value="">全部</option>
          <option value="transform_chapter">transform_chapter</option>
          <option value="test_model">test_model</option>
          <option value="regenerate_preview">regenerate_preview</option>
        </select>
      </label>
      <label class="filter">
        <span>状态</span>
        <select :value="filter.status ?? ''" @change="onStatusChange(($event.target as HTMLSelectElement).value)">
          <option value="">全部</option>
          <option value="success">成功</option>
          <option value="failed">失败</option>
        </select>
      </label>
      <label class="filter">
        <span>模型</span>
        <select :value="filter.model_config_id ?? ''" @change="onModelConfigChange(($event.target as HTMLSelectElement).value)">
          <option value="">(不限)</option>
          <option v-for="m in models" :key="m.id" :value="m.id">
            {{ m.name }}
          </option>
        </select>
      </label>
    </div>

    <div v-if="error" class="alert">{{ error }}</div>

    <div v-if="loading && logs.length === 0" class="empty">加载中...</div>
    <div v-else-if="logs.length === 0" class="empty">
      <p class="empty-title">还没有 AI 调用日志</p>
      <p class="empty-hint">
        数据来自 transformer 路径(章节转换)与 test_model 路径(模型测试连通性)。
      </p>
    </div>
    <div v-else ref="aiCallTableEl" class="table-wrap">
      <DataTable
        :columns="aiCallColumns"
        :data="logs"
        :row-key="(row) => row.id"
        :widths="aiCallWidths"
        :truncate-columns="['error']"
        :max-height="aiCallTableMaxHeight"
        frozen-column="actions"
      >
      <template #cell-time="{ row }">
        <div class="time">
          <span class="time-hms">{{ formatTime(row.created_at).slice(11) }}</span>
          <span class="time-date">{{ formatDate(row.created_at) }}</span>
        </div>
      </template>
      <template #cell-business="{ row }">
        <Tag :kind="row.business === 'transform_chapter' ? 'info' : row.business === 'regenerate_preview' ? 'success' : 'warn'">
          {{ row.business === 'transform_chapter' ? '章节转换' : row.business === 'regenerate_preview' ? '试运行预览' : '模型测试' }}
        </Tag>
      </template>
      <template #cell-model="{ row }">
        <div class="model-cell">
          <div class="model-name">{{ row.model_name }}</div>
          <div class="model-url">{{ row.base_url }}</div>
        </div>
      </template>
      <template #cell-status="{ row }">
        <Tag :kind="row.status === 'success' ? 'success' : 'danger'">
          {{ row.status === 'success' ? '成功' : '失败' }}
        </Tag>
      </template>
      <template #cell-tokens="{ row }">
        <div class="tokens">
          <span>in 粗估 {{ row.estimated_tokens_in ?? '—' }}</span>
          <span class="arrow">→</span>
          <span :class="{ missing: row.actual_tokens_in === null }">
            实际 {{ row.actual_tokens_in ?? '—' }}
          </span>
          <span class="divider">/</span>
          <span>out {{ row.actual_tokens_out ?? '—' }}</span>
        </div>
      </template>
      <template #cell-latency="{ row }">{{ formatLatency(row.latency_ms) }}</template>
      <template #cell-error="{ row }">
        <span v-if="row.error" class="err-text" :title="row.error">
          {{ truncate(row.error, 40) }}
        </span>
        <span v-else class="muted">—</span>
      </template>
      <template #cell-actions="{ row }">
        <button type="button" class="row-link" @click="openDetail(row.id)">查看</button>
      </template>
      </DataTable>
      <div v-if="pageCount > 1 || total > 0" class="pager">
        <span class="meta">
          共 {{ total }} 条 · 每页 {{ filter.limit }}
        </span>
        <div class="pager-buttons">
          <button
            type="button"
            class="page-btn"
            :disabled="page === 1 || loading"
            @click="goPrev"
          >上一页</button>
          <template v-for="(item, idx) in pageNumbers" :key="idx">
            <span v-if="item === '…'" class="page-ellipsis">…</span>
            <button
              v-else
              type="button"
              class="page-btn"
              :class="{ active: item === page }"
              :disabled="loading"
              @click="goTo(item as number)"
            >{{ item }}</button>
          </template>
          <button
            type="button"
            class="page-btn"
            :disabled="page >= pageCount || loading"
            @click="goNext"
          >下一页</button>
        </div>
      </div>
    </div>

    <AiCallDetail
      v-if="detailId !== null"
      :id="detailId"
      @close="detailId = null"
    />

    <ConfirmDialog
      v-model:open="clearConfirmOpen"
      title="清空 AI 调用日志"
      message="将删除全部 ai_call_logs 行(无法恢复)。transform / test_model 历史 token / 错误信息都会清空。确认?"
      kind="danger"
      confirm-text="清空"
      @confirm="doClear"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { useIntervalFn } from '@vueuse/core';
import { formatDate, formatTime } from '../utils/format';
import Button from '../components/ui/Button.vue';
import DataTable from '../components/ui/DataTable.vue';
import { useDynamicTableHeight } from '../composables/useDynamicTableHeight';
import Tag from '../components/ui/Tag.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AiCallDetail from '../components/AiCallDetail.vue';
import { useModelsStore } from '../stores/models';
import {
  clearAiCallLogs,
  listAiCallLogs,
} from '../ipc/commands';
import type { AiCallBusiness, AiCallLog, AiCallLogFilter, ModelConfig } from '../ipc/types';

const logs = ref<AiCallLog[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const detailId = ref<number | null>(null);
const clearConfirmOpen = ref(false);

/// 模型下拉列表 —— 从 useModelsStore 拉,onMounted 时显式 load
/// (默认 store 可能未加载,UI 进来时下拉框为空)。
const modelsStore = useModelsStore();
const models = ref<ModelConfig[]>([]);

/// 过滤状态 —— 任何字段改都触发 reload,reload 会清空 cursor 回到第 1 页。
const filter = reactive<{
  business: AiCallLogFilter["business"];
  status: AiCallLogFilter["status"];
  model_config_id: AiCallLogFilter["model_config_id"];
  limit: number;
}>({
  business: null,
  status: null,
  model_config_id: null,
  limit: 30,
});

/// 传统页码翻页状态。
/// - page: 1-indexed 当前页。
/// - total: 当前 filter 下的总行数,后端 list 返回时一并带回。
/// - pageCount: 由 total / limit 算出;total=0 时 pageCount=1(避免除零 + UI 边界)。
/// 偏移漂移:OFFSET 在新写入时会让后续页整体上移一格。对显式页码导航可接受
/// —— 用户点 N 就是取第 N 页,UI 重新渲染。
const page = ref(1);
const total = ref(0);
const pageCount = computed(() => Math.max(1, Math.ceil(total.value / filter.limit)));

/// 表格自适应高度 —— 跟随 main.app 尺寸变化、跟随轮询新数据重算
const aiCallTableEl = ref<HTMLElement | null>(null);
const { maxHeight: aiCallTableMaxHeight } = useDynamicTableHeight({
  tableEl: aiCallTableEl,
  minHeight: 300,
  // 默认 48 不够 —— DataTable 底部横向滚动条(~17px) + pager 块(~44px)
  // + 容错 (~10px) 共 ~72px。这里显式覆盖,不要默默扩展默认 padding。
  bottomPadding: 72,
  deps: [() => logs.value.length],
});

/// DataTable(TanStack)列定义。time/business/status/tokens/latency/error 都用 slot 渲染
/// (含 Tag/格式化/截断/特殊布局),列定义只声明 header + id。
const aiCallColumns = [
  { accessorKey: 'created_at', id: 'time', header: '时间', enableSorting: true },
  { id: 'business', header: '业务', enableSorting: false },
  { id: 'model', header: '模型', enableSorting: false },
  { id: 'status', header: '状态', enableSorting: false },
  { id: 'tokens', header: 'tokens', enableSorting: false },
  { id: 'latency', header: '延迟', enableSorting: true },
  { id: 'error', header: '错误', enableSorting: false },
  { id: 'actions', header: '操作', enableSorting: false },
];
const aiCallWidths: Record<string, number> = {
  time: 170,
  business: 110,
  model: 240,
  status: 80,
  tokens: 280,
  latency: 90,
  error: 240,
  actions: 90,
};

/// AI 调用日志轮询 —— vueuse useIntervalFn 自动随组件卸载清理(immediate:false 让首次不立即触发,挂载期再 resume)。
const aiCallPoll = useIntervalFn(() => { void reload(); }, 3000, { immediate: false, immediateCallback: false });

/// 拉当前 page 的日志(offset = (page-1)*limit),刷新当前显示。
/// 轮询、换 filter、换页码都走这里 —— 都是"刷当前页"。
async function reload(): Promise<void> {
  loading.value = true;
  error.value = null;
  try {
    const { logs: rows, total: t } = await listAiCallLogs({
      business: filter.business,
      status: filter.status,
      model_config_id: filter.model_config_id,
      limit: filter.limit,
      offset: (page.value - 1) * filter.limit,
    });
    logs.value = rows;
    total.value = t;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

/// 切到第 n 页:先夹紧到 [1, pageCount],然后 reload。offset 由 reload 算。
async function goTo(n: number): Promise<void> {
  const target = Math.max(1, Math.min(pageCount.value, n));
  if (target === page.value) return;
  page.value = target;
  await reload();
}
const goPrev = () => void goTo(page.value - 1);
const goNext = () => void goTo(page.value + 1);

/// 底部分页要展示的页码列表。
/// - 总页数 <= 7:全部展开。
/// - 否则:始终包含 1 和最后一页 + current ± 2,中间用 … 补齐缺口。
/// 返回 (number | '…')[] 让模板直接 v-for。
const pageNumbers = computed<(number | '…')[]>(() => {
  const total = pageCount.value;
  const cur = page.value;
  if (total <= 0) return [];
  if (total <= 7) {
    return Array.from({ length: total }, (_, i) => i + 1);
  }
  const wanted = new Set<number>([1, total]);
  for (let i = cur - 2; i <= cur + 2; i++) {
    if (i >= 1 && i <= total) wanted.add(i);
  }
  const sorted = [...wanted].sort((a, b) => a - b);
  const out: (number | '…')[] = [];
  for (let i = 0; i < sorted.length; i++) {
    if (i > 0 && sorted[i] - sorted[i - 1] > 1) out.push('…');
    out.push(sorted[i]);
  }
  return out;
});

/// filter 变更时:回到第 1 页(否则 offset 越大越空)。三条都共用一个 handler。
function applyFilter(): void {
  page.value = 1;
  void reload();
}

function onBusinessChange(v: string) {
  filter.business = v === '' ? null : (v as AiCallBusiness);
  applyFilter();
}

function onStatusChange(v: string) {
  filter.status = v === '' ? null : (v as 'success' | 'failed');
  applyFilter();
}

function onModelConfigChange(v: string) {
  const n = v.trim() === '' ? null : Number(v);
  filter.model_config_id = n !== null && Number.isFinite(n) && n > 0 ? n : null;
  applyFilter();
}

function openDetail(id: number) {
  detailId.value = id;
}

function onClear() {
  clearConfirmOpen.value = true;
}

async function doClear() {
  try {
    await clearAiCallLogs();
    await reload();
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}


function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n) + '…' : s;
}

onMounted(async () => {
  // 拉模型列表 + 启动轮询 + 首屏数据 —— 并发即可,各自独立。
  void modelsStore.load().then(() => { models.value = modelsStore.models; });
  aiCallPoll.resume();
  void reload();
});
</script>

<style scoped>
/* table-wrap 让 useDynamicTableHeight 计算表格 div 在 main.app 内的偏移;
   不带 padding/margin,避免破坏 maxHeight 算式。 */
.table-wrap {
  /* 无样式,仅作为高度测量锚点 */
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.filter {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
}
.filter select,
.filter input {
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-size: 13px;
  font-family: inherit;
  outline: none;
}
.filter input { width: 90px; }
.meta {
  font-size: 12px;
  color: var(--text-muted);
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.pager {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 0 4px;
}
.pager .meta {
  font-size: 12px;
  color: var(--text-muted);
}
.pager-buttons {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.page-btn {
  min-width: 30px;
  height: 28px;
  padding: 0 10px;
  background: var(--color-sheet);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  font-size: 13px;
  font-family: inherit;
  cursor: pointer;
}
.page-btn:hover:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}
.page-btn.active {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--color-sheet);
}
.page-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.page-ellipsis {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 28px;
  color: var(--text-muted);
  font-size: 13px;
  user-select: none;
}
.alert {
  padding: 12px 16px;
  background: var(--danger-bg);
  color: var(--danger);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
  border: 1px solid var(--danger-border);
}
.empty {
  text-align: center;
  padding: 48px 24px;
  color: var(--text-muted);
  border: 1px dashed var(--border-rouge);
  border-radius: var(--radius-card);
  background: var(--color-sheet);
}
.empty-title {
  font-size: 16px;
  color: var(--text-primary);
  margin: 0 0 8px;
}
.empty-hint {
  font-size: 13px;
  margin: 0;
  line-height: 1.6;
}
.time {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  line-height: 1.35;
}
.time-hms {
  color: var(--text-primary);
}
.time-date {
  color: var(--text-muted);
  font-size: 11px;
}
.model-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.model-name {
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.model-url {
  font-size: 11px;
  color: var(--text-muted);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tokens {
  display: flex;
  align-items: center;
  gap: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
.tokens .arrow { color: var(--text-muted); }
.tokens .divider { color: var(--text-muted); margin: 0 2px; }
.tokens .missing { color: var(--text-muted); font-style: italic; }
.err-text {
  color: var(--danger);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
}
.muted {
  color: var(--text-muted);
  font-size: 12px;
}
.row-link {
  background: none;
  border: none;
  padding: 0;
  color: var(--text-secondary);
  font-size: 13px;
  font-family: inherit;
  cursor: pointer;
}
.row-link:hover {
  color: var(--text-primary);
  text-decoration: underline;
}
</style>
