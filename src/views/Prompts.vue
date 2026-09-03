<template>
  <section>
    <PageHeader title="提示词" subtitle="压缩与文风转换的 prompt 模板,内置 / 自定义均可">
      <template #actions>
        <Button kind="primary" @click="openCreate">新建 prompt</Button>
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
    </div>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div v-if="!store.loading && store.prompts.length === 0" class="empty">
      <p class="empty-title">
        {{ store.includeArchived ? '没有任何提示词(包括归档)' : '还没有提示词' }}
      </p>
      <p v-if="!store.includeArchived" class="empty-hint">
        点击右上"新建 prompt"创建一条;内置 prompt 可用"复制"派生用户版后再编辑。
      </p>
    </div>
    <div v-else ref="promptTableEl" class="table-wrap">
      <DataTable
        :columns="promptColumns"
        :data="store.prompts"
        :row-key="(row) => row.id"
        :widths="promptWidths"
        :max-height="promptTableMaxHeight"
        frozen-column="actions"
      >
      <template #cell-name="{ row }">
        <span :class="{ archived: row.archived === 1 }">{{ row.name }}</span>
        <Tag v-if="row.archived === 1" kind="info" class="archived-tag">已归档</Tag>
      </template>
      <template #cell-kind="{ row }">
        <Tag :kind="row.kind === 'compress' ? 'info' : 'success'">
          {{ formatPromptKind(row.kind) }}
        </Tag>
      </template>
      <template #cell-builtin="{ row }">
        <Tag v-if="row.is_builtin" kind="info">内置</Tag>
        <span v-else class="muted">用户</span>
      </template>
      <template #cell-actions="{ row }">
        <template v-if="row.archived === 1">
          <button type="button" class="row-link" @click="onRestore(row.id)">恢复</button>
        </template>
        <template v-else>
          <button v-if="row.is_builtin" type="button" class="row-link" @click="openView(row)">查看</button>
          <button v-else type="button" class="row-link" @click="openEdit(row)">编辑</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link" @click="openCopy(row)">复制</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link danger" @click="onDelete(row)">删除</button>
        </template>
      </template>
      </DataTable>
    </div>

    <PromptEditDialog
      v-model:open="dialogOpen"
      :mode="dialogMode"
      :initial="dialogInitial"
      @saved="onSaved"
    />

    <PromptViewDialog
      v-if="viewTarget"
      v-model:open="viewOpen"
      :initial="viewTarget"
    />

    <ConfirmDialog
      v-model:open="deleteConfirmOpen"
      title="归档提示词"
      :message="deleteConfirmMessage"
      kind="danger"
      confirm-text="归档"
      @confirm="doDelete"
    />

    <AlertDialog
      v-model:open="alertOpen"
      title="提示"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import Button from '../components/ui/Button.vue';
import DataTable from '../components/ui/DataTable.vue';
import { useDynamicTableHeight } from '../composables/useDynamicTableHeight';
import Tag from '../components/ui/Tag.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import PromptEditDialog from '../components/PromptEditDialog.vue';
import PromptViewDialog from '../components/PromptViewDialog.vue';
import { usePromptsStore } from '../stores/prompts';
import { formatPromptKind } from '../utils/prompt-locale';
import type { Prompt } from '../ipc/types';

const store = usePromptsStore();

/// 表格自适应高度 —— 跟随窗口大小变化,跟随数据数量变化(显示/隐藏归档)重算
const promptTableEl = ref<HTMLElement | null>(null);
const { maxHeight: promptTableMaxHeight } = useDynamicTableHeight({
  tableEl: promptTableEl,
  minHeight: 300,
  deps: [() => store.prompts.length, () => store.includeArchived],
});

type DialogMode = 'create' | 'edit' | 'copy-from-builtin';

/// DataTable(TanStack)列定义。kind/builtin 在模板里用 slot + Tag 渲染(列定义
/// 只声明 header + id,具体渲染交给模板保持灵活性)。
const promptColumns = [
  { accessorKey: 'name', id: 'name', header: '名称', enableSorting: true },
  { id: 'kind', header: '类型', enableSorting: false },
  { id: 'builtin', header: '来源', enableSorting: false },
  { id: 'actions', header: '操作', enableSorting: false },
];
const promptWidths: Record<string, number> = {
  name: 240,
  kind: 100,
  builtin: 120,
  actions: 280,
};

const dialogOpen = ref(false);
const dialogMode = ref<DialogMode>('create');
const dialogInitial = ref<Prompt | undefined>(undefined);

const viewOpen = ref(false);
const viewTarget = ref<Prompt | null>(null);

/// 删除确认 —— 用 ConfirmDialog(与 Models 一致)代替原内嵌 Dialog,
/// 提前查引用计数,文案直接拼到 message,避免自定义 footer。
const deleteConfirmOpen = ref(false);
const deleteTarget = ref<Prompt | null>(null);
const deleteConfirmMessage = ref('');
const alertOpen = ref(false);
const alertMessage = ref('');

onMounted(() => void store.load());

function openCreate() {
  dialogMode.value = 'create';
  dialogInitial.value = undefined;
  dialogOpen.value = true;
}

function openEdit(row: Prompt) {
  dialogMode.value = 'edit';
  dialogInitial.value = row;
  dialogOpen.value = true;
}

function openView(row: Prompt) {
  viewTarget.value = row;
  viewOpen.value = true;
}

function openCopy(row: Prompt) {
  dialogMode.value = 'copy-from-builtin';
  dialogInitial.value = row;
  dialogOpen.value = true;
}

function onSaved() {
}

async function onDelete(row: Prompt) {
  let usage = 0;
  try {
    usage = await store.countUsage(row.id);
  } catch {
    usage = 0;
  }
  deleteTarget.value = row;
  deleteConfirmMessage.value = usage > 0
    ? `确认删除提示词"${row.name}"?该 prompt 当前被 ${usage} 个转换结果引用,删除(归档)后这些结果仍保留历史引用,但新建转换时无法再选用。`
    : `确认删除提示词"${row.name}"?删除为软删(归档),可在此页勾选"显示已归档"后恢复。`;
  deleteConfirmOpen.value = true;
}

async function doDelete() {
  const target = deleteTarget.value;
  if (!target) return;
  try {
    await store.remove(target.id);
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  } finally {
    deleteTarget.value = null;
  }
}

async function onRestore(id: number) {
  try {
    await store.restore(id);
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  }
}

async function onToggleArchived(v: boolean) {
  await store.setIncludeArchived(v);
}
</script>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
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
/* table-wrap 让 useDynamicTableHeight 计算表格 div 在 main.app 内的偏移;
   不带 padding/margin,避免破坏 maxHeight 算式。 */
.table-wrap {
  /* 无样式,仅作为高度测量锚点 */
}

.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
}
.empty {
  text-align: center;
  padding: 48px 24px;
  color: var(--text-secondary);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
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
.muted {
  color: var(--text-secondary);
  font-size: 13px;
}
.archived {
  color: var(--text-muted);
  text-decoration: line-through;
}
.archived-tag {
  margin-left: 6px;
}
</style>
