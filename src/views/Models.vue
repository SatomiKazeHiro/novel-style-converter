<template>
  <section>
    <PageHeader title="模型" subtitle="OpenAI 兼容 API 配置，可在此新增 / 编辑 / 软删">
      <template #actions>
        <Button @click="openCatalogUpdate">更新模型清单</Button>
        <Button kind="primary" @click="openCreate">新增模型</Button>
      </template>
    </PageHeader>

    <div class="toolbar">
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.includeArchived"
          @change="onToggleArchived(($event.target as HTMLInputElement).checked)"
        />
        <span>显示已归档</span>
      </label>
      <span class="catalog-status">
        模型清单：<strong>{{ catalogLabel }}</strong>
        <span v-if="catalogMetaLabel" class="muted">· {{ catalogMetaLabel }}</span>
      </span>
    </div>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div v-if="!store.loading && !store.hasAnyModel" class="empty">
      <p class="empty-title">尚未配置任何模型</p>
      <p class="empty-hint">
        模型是 LLM 转换的前提。点"新增模型"填一份 OpenAI 兼容 endpoint 的 base_url / api_key / model 即可。
      </p>
      <p class="empty-hint subtle">
        注：本应用不会从环境变量自动注入默认模型；一切模型配置都需要在此处显式新增。
      </p>
    </div>

    <div v-else ref="modelTableEl" class="table-wrap">
      <DataTable
        :columns="modelColumns"
        :data="store.models"
        empty-text="暂无模型"
        :row-key="(row) => row.id"
        :widths="modelWidths"
        :truncate-columns="['base_url']"
        :max-height="modelTableMaxHeight"
        frozen-column="actions"
      >
      <template #cell-name="{ row }">
        <span :class="{ archived: row.archived === 1 }">{{ row.name }}</span>
        <Tag v-if="row.archived === 1" kind="info" class="archived-tag">已归档</Tag>
      </template>
      <template #cell-model="{ row }">{{ row.model }}</template>
      <template #cell-base_url="{ row }">{{ row.base_url }}</template>
      <template #cell-concurrency="{ row }">{{ row.concurrency }}</template>
      <template #cell-actions="{ row }">
        <template v-if="row.archived === 1">
          <button type="button" class="row-link" @click="onRestore(row.id)">恢复</button>
        </template>
        <template v-else>
          <button type="button" class="row-link" @click="openEdit(row)">编辑</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link danger" @click="onDelete(row.id)">删除</button>
        </template>
      </template>
      </DataTable>
    </div>

    <ModelDialog
      v-model:open="dialogOpen"
      :initial="dialogInitial"
      @submit="onSubmit"
    />

    <ConfirmDialog
      v-model:open="deleteConfirmOpen"
      title="归档模型"
      message="归档后会清空 API key 并隐藏该行，历史转换结果仍可显示来源 model 名 / 端点 / 并发配置。确认归档？"
      kind="danger"
      confirm-text="归档"
      @confirm="doDelete"
    />

    <CatalogUpdateDialog
      v-model:open="catalogUpdateOpen"
      @updated="refreshCatalogStatus"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import DataTable from '../components/ui/DataTable.vue';
import { useDynamicTableHeight } from '../composables/useDynamicTableHeight';
import Tag from '../components/ui/Tag.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import ModelDialog from '../components/ModelDialog.vue';
import CatalogUpdateDialog from '../components/CatalogUpdateDialog.vue';
import { useModelsStore } from '../stores/models';
import type { ModelConfigInput } from '../ipc/types';
import { catalogStatus, type CatalogStatus } from '../ipc/commands';

const store = useModelsStore();

/// 表格自适应高度 —— 跟随窗口大小变化,跟随数据数量变化(显示/隐藏归档)重算
const modelTableEl = ref<HTMLElement | null>(null);
const { maxHeight: modelTableMaxHeight } = useDynamicTableHeight({
  tableEl: modelTableEl,
  minHeight: 300,
  deps: [() => store.models.length, () => store.includeArchived],
});
const dialogOpen = ref(false);
const dialogInitial = ref<ModelConfigInput | null>(null);
const deleteConfirmOpen = ref(false);
const deleteTargetId = ref<number | null>(null);

/// DataTable(TanStack)列定义 —— name / id / base_url 在模板里用 slot 处理 archived 样式,
/// 列定义只声明 header + id。concurrency 是数字列,UI 端用 tabular-nums 右对齐。
/// id 列已移除:从前用户填模型 id 时方便对照,现在 AiCalls 用下拉选模型名,
/// 表格里再单独列一个数字 id 属于冗余。
const modelColumns = [
  { accessorKey: 'name', id: 'name', header: '名称', enableSorting: true },
  { accessorKey: 'model', id: 'model', header: '模型', enableSorting: true },
  { accessorKey: 'base_url', id: 'base_url', header: 'Base URL', enableSorting: false },
  { accessorKey: 'concurrency', id: 'concurrency', header: '并发', enableSorting: true },
  { id: 'actions', header: '操作', enableSorting: false },
];
const modelWidths: Record<string, number> = {
  name: 160,
  model: 180,
  base_url: 280,
  concurrency: 80,
  actions: 180,
};

// 模型清单（models.dev）状态 —— 只读展示,不参与表格渲染 / 不干预布局
const catalogStatusState = ref<CatalogStatus | null>(null);
const catalogUpdateOpen = ref(false);

const catalogLabel = computed(() => {
  const s = catalogStatusState.value;
  if (!s) return '加载中…';
  return s.source === 'cache' ? 'cache（用户目录）' : 'bundled（内置）';
});

const catalogMetaLabel = computed(() => {
  const m = catalogStatusState.value?.meta;
  if (!m) return null;
  const src = m.origin === 'drop' ? '拖入' : m.origin === 'http' ? '远端拉取' : '内置';
  const ts = m.fetched_at ? m.fetched_at.replace('T', ' ').slice(0, 19) : '';
  const sizeMB = (m.size_bytes / (1024 * 1024)).toFixed(2);
  return src + ' · ' + sizeMB + ' MB · ' + ts;
});

async function refreshCatalogStatus(): Promise<void> {
  try {
    catalogStatusState.value = await catalogStatus();
  } catch (e) {
    // status 拉失败不致命 —— 显示加载中即可,用户仍可点 "更新模型清单"
    console.error('[catalog] status failed:', e);
  }
}

function openCatalogUpdate(): void {
  catalogUpdateOpen.value = true;
}

onMounted(async () => {
  await Promise.all([store.load(), refreshCatalogStatus()]);
});

function openCreate() {
  dialogInitial.value = null;
  dialogOpen.value = true;
}

function openEdit(row: ModelConfigInput) {
  dialogInitial.value = { ...row };
  dialogOpen.value = true;
}

async function onSubmit(input: ModelConfigInput) {
  await store.save(input);
}

function onDelete(id: number) {
  deleteTargetId.value = id;
  deleteConfirmOpen.value = true;
}

async function doDelete() {
  const id = deleteTargetId.value;
  if (id == null) return;
  await store.remove(id);
}

async function onRestore(id: number) {
  await store.restore(id);
}

async function onToggleArchived(v: boolean) {
  await store.setIncludeArchived(v);
}
</script>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
}
.catalog-status {
  font-size: 12px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
}
.catalog-status strong {
  color: var(--text-primary);
  font-weight: 500;
}
.catalog-status .muted {
  margin-left: 4px;
}
/* table-wrap 让 useDynamicTableHeight 计算表格 div 在 main.app 内的偏移;
   不带 padding/margin,避免破坏 maxHeight 算式。 */
.table-wrap {
  /* 无样式,仅作为高度测量锚点 */
}

.alert {
  padding: 12px 16px;
  background: var(--danger-bg);
  color: var(--danger);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
  border: 1px solid var(--danger-border);
  font-family: var(--font-serif);
}
.empty {
  text-align: center;
  padding: 48px 24px;
  color: var(--text-muted);
  border: 1px dashed var(--border-rouge);
  border-radius: var(--radius-card);
  background: var(--color-sheet);
  font-family: var(--font-serif);
}
.empty-title {
  font-size: 18px;
  color: var(--text-primary);
  margin: 0 0 12px;
}
.empty-hint {
  font-size: 14px;
  margin: 0 0 6px;
  line-height: 1.6;
}
.empty-hint.subtle {
  color: var(--text-muted);
  font-style: italic;
  font-size: 12px;
}
.archived {
  color: var(--text-muted);
  text-decoration: line-through;
}
.archived-tag {
  margin-left: 6px;
}
</style>
