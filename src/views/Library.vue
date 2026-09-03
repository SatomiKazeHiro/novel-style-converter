<template>
  <section>
    <PageHeader :title="pageTitle" :subtitle="pageSubtitle">
      <template #actions>
        <Button v-if="page === 'uploads'" kind="primary" :loading="store.uploading" @click="uploadDialogOpen = true">上传 .txt</Button>
      </template>
    </PageHeader>

    <Transition name="toast">
      <div v-if="toast" class="toast">
        <span class="toast-msg">{{ toast.text }}</span>
        <button v-if="toast.action" type="button" class="toast-action" @click="onToastAction">{{ toast.actionLabel }}</button>
        <button type="button" class="toast-close" aria-label="关闭" @click="dismissToast">×</button>
      </div>
    </Transition>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <template v-if="page === 'uploads'">
      <div v-if="!store.loading && store.uploads.length === 0" class="empty">
        还没有文件,点击右上"上传 .txt"添加一个。
      </div>
      <DataTable
        v-else
        :columns="uploadColumns"
        :data="store.uploads"
        :row-key="(row) => row.id"
        :widths="uploadWidths"
        :numeric-columns="['size', 'words', 'assets']"
        :truncate-columns="['filename']"
        frozen-column="actions"
      >
        <template #cell-filename="{ row }">
          <Tooltip :text="row.filename"><span class="cell-truncate">{{ row.filename }}</span></Tooltip>
        </template>
        <template #cell-assets="{ row }">
          <Tag v-if="daCount(row.id) > 0" kind="success">{{ daCount(row.id) }} 个</Tag>
          <span v-else class="muted">—</span>
        </template>
        <template #cell-actions="{ row }">
          <button type="button" class="row-link" @click="goUpload(row.id)">查看</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link" @click="goParse(row.id)">解析章节</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link danger" @click="onDeleteUpload(row.id, row.filename)">删除</button>
        </template>
      </DataTable>
    </template>

    <template v-else-if="page === 'data-assets'">
      <div v-if="!store.loading && store.dataAssets.length === 0" class="empty">
        还没有数据资产。请到“上传原文”页面选择文件并解析章节。
      </div>
      <DataTable
        v-else
        :columns="daColumns"
        :data="store.dataAssets"
        :row-key="(row) => row.id"
        :widths="daWidths"
        :numeric-columns="['words', 'derived']"
        :truncate-columns="['title', 'source']"
        frozen-column="actions"
      >
        <template #cell-title="{ row }">
          <Tooltip :text="row.title"><span class="cell-truncate">{{ row.title }}</span></Tooltip>
        </template>
        <template #cell-kind="{ row }">
          <Tag v-if="row.kind === 'promoted'" kind="success">派生</Tag>
          <Tag v-else>源</Tag>
        </template>
        <template #cell-source="{ row }">
          <Tooltip :text="row.filename"><span class="cell-truncate">{{ row.filename }}</span></Tooltip>
        </template>
        <template #cell-derived="{ row }">
          <Tag v-if="row.promoted_count > 0" kind="success">{{ row.promoted_count }} 个</Tag>
          <span v-else class="muted">—</span>
        </template>
        <template #cell-status="{ row }">
          <Tag v-if="row.tn_count > 0" kind="warn">有 {{ row.tn_count }} 个工程</Tag>
          <Tag v-else kind="success">无引用</Tag>
        </template>
        <template #cell-actions="{ row }">
          <button type="button" class="row-link" @click="goDataAsset(row.id)">查看</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button type="button" class="row-link" @click="openCreateTn(row.id)">新建工程</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button
            type="button"
            class="row-link danger"
            :title="row.tn_count > 0 ? `有 ${row.tn_count } 个工程引用` : ''"
            @click="onDeleteDa(row.id, row.title, row.tn_count)"
          >删除</button>
        </template>
      </DataTable>
    </template>

    <template v-else>
      <div v-if="!store.loading && store.transformationNovels.length === 0" class="empty">
        还没有转换工程。请到“数据资产”页面选择资产并新建工程。
      </div>
      <DataTable
        v-else
        :columns="tnColumns"
        :data="store.transformationNovels"
        :row-key="(row: TransformationNovelSummary) => row.id"
        :widths="tnWidths"
        :numeric-columns="['workflow']"
        :truncate-columns="['source']"
        frozen-column="actions"
      >
        <template #cell-source="{ row }">
          <Tooltip :text="sourceAssetTitle(row.data_asset_id)">
            <button type="button" class="row-link source-link" @click="goDataAsset(row.data_asset_id)">
              {{ sourceAssetTitle(row.data_asset_id) }} · {{ row.chapters_count ?? 0 }} 章
            </button>
          </Tooltip>
        </template>
                <template #cell-workflow="{ row }">
          {{ row.workflow_count }}
          <Tag
            v-if="row.running_workflow_count > 0"
            kind="warn"
            :title="row.running_workflow_count + ' 个工作流进行中(running + paused)'"
          >{{ row.running_workflow_count }} 工作中</Tag>
        </template>
        <template #cell-title="{ row }">
          <Input v-if="renamingId === row.id" v-model="renameDraft" />
          <template v-else>{{ row.title }}</template>
        </template>
        <template #cell-actions="{ row }">
          <template v-if="renamingId === row.id">
            <Button size="small" kind="primary" @click="onSaveRename(row.id)">保存</Button>
            <Button size="small" @click="cancelRename">取消</Button>
          </template>
          <template v-else>
            <button type="button" class="row-link" @click="goDetail(row.id)">查看</button>
            <span class="row-sep" aria-hidden="true">·</span>
            <button type="button" class="row-link" @click="startRename(row.id, row.title)">重命名</button>
            <span class="row-sep" aria-hidden="true">·</span>
            <button type="button" class="row-link danger" @click="onDeleteTn(row.id, row.title)">删除</button>
          </template>
        </template>
      </DataTable>
    </template>

    <UploadDialog v-model:open="uploadDialogOpen" @submit="onUpload" />

    <TransformationNovelDialog
      v-model:open="tnDialogOpen"
      :data-asset-id="tnDialogDataAssetId"
      @submit="onCreateTn"
    />

    <ConfirmDialog
      v-model:open="deleteUploadConfirmOpen"
      title="删除上传原文"
      :message="deleteUploadMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDeleteUpload"
    />

    <ConfirmDialog
      v-model:open="deleteTnConfirmOpen"
      title="删除转换小说"
      :message="deleteTnMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDeleteTn"
    />

    <ConfirmDialog
      v-model:open="deleteDaConfirmOpen"
      title="删除数据资产"
      :message="deleteDaMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDeleteDa"
    />

    <AlertDialog
      v-model:open="alertOpen"
      :title="alertTitle"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useTimeoutFn } from '@vueuse/core';
import { useRoute, useRouter } from 'vue-router';
import Button from '../components/ui/Button.vue';
import DataTable from '../components/ui/DataTable.vue';
import Input from '../components/ui/Input.vue';
import Tooltip from '../components/ui/Tooltip.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import Tag from '../components/ui/Tag.vue';
import UploadDialog from '../components/UploadDialog.vue';
import TransformationNovelDialog from '../components/TransformationNovelDialog.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import { useLibraryStore } from '../stores/library';
import { formatSize, formatTime, formatWordCount } from '../utils/format';
import type { UploadSummary, TransformationNovelSummary } from '../ipc/types';
import { previewUploadDeletion } from '../ipc/commands';

const route = useRoute();
const router = useRouter();
const store = useLibraryStore();
/// 按 upload_id 统计已生成的数据资产数量,O(N) 一次算好供表格行查。
const daCountByUpload = computed(() => {
  const m = new Map<number, number>();
  for (const d of store.dataAssets) {
    m.set(d.upload_id, (m.get(d.upload_id) ?? 0) + 1);
  }
  return m;
});
function daCount(uploadId: number): number {
  return daCountByUpload.value.get(uploadId) ?? 0;
}
/// 转换工程源列 —— 从 store.dataAssets 按 id 查标题。
/// 数据资产被删后 join 失败,fallback 到 `数据资产 #N`,不显示 "undefined"。
function sourceAssetTitle(dataAssetId: number): string {
  return store.dataAssets.find((a) => a.id === dataAssetId)?.title ?? `数据资产 #${dataAssetId}`;
}

type Page = 'uploads' | 'data-assets' | 'transformations';
const page = computed<Page>(() => (route.meta.libraryPage as Page | undefined) ?? 'uploads');
const pageTitle = computed(() => ({
  uploads: '上传原文',
  'data-assets': '数据资产',
  transformations: '转换工程',
})[page.value]);

const pageSubtitle = computed(() => ({
  uploads: '导入 .txt 原文 · 解析章节 · 生成数据资产',
  'data-assets': '浏览 / 编辑 / 删除已解析的章节数据 · 转换工程的输入',
  transformations: '对数据资产应用 prompt · 派生新的转换数据',
})[page.value]);

const uploadDialogOpen = ref(false);
const tnDialogOpen = ref(false);
const tnDialogDataAssetId = ref(0);
const renamingId = ref<number | null>(null);
const renameDraft = ref('');

const deleteUploadConfirmOpen = ref(false);
const deleteUploadMessage = ref('');
const deleteUploadId = ref<number | null>(null);

const deleteTnConfirmOpen = ref(false);
const deleteTnMessage = ref('');
const deleteTnId = ref<number | null>(null);

const deleteDaConfirmOpen = ref(false);
const deleteDaMessage = ref('');
const deleteDaId = ref<number | null>(null);

const alertOpen = ref(false);
const alertTitle = ref('提示');
const alertMessage = ref('');

function showAlert(title: string, message: string) {
  alertTitle.value = title;
  alertMessage.value = message;
  alertOpen.value = true;
}

const uploadColumns = [
  { accessorKey: 'filename', header: '文件名', enableSorting: true },
  {
    accessorKey: 'byte_size',
    id: 'size',
    header: '大小',
    enableSorting: true,
    cell: (info: any) => formatSize(info.getValue() as number),
  },
  {
    accessorKey: 'word_count',
    id: 'words',
    header: '字数',
    enableSorting: true,
    cell: (info: any) => formatWordCount(info.getValue() as number),
  },
  {
    accessorKey: 'uploaded_at',
    id: 'uploaded',
    header: '上传时间',
    enableSorting: true,
    cell: (info: any) => formatTime(info.getValue() as string),
  },
  { id: 'assets', header: '数据资产', enableSorting: false },
  { id: 'actions', header: '操作', enableSorting: false },
];
const uploadWidths: Record<string, number> = {
  filename: 260,
  size: 100,
  words: 100,
  uploaded: 180,
  assets: 100,
  actions: 200,
};

/// DataTable(TanStack)列定义:accessorKey 直接读 DataAssetRow 字段,
/// 需要自定义渲染的列(kind/derived/status)留空 cell 走 <template #cell-*> 插槽。
const daColumns = [
  { accessorKey: 'title', header: '标题', enableSorting: true },
  { accessorKey: 'kind', header: '类型', enableSorting: true },
  { accessorKey: 'filename', id: 'source', header: '来源', enableSorting: true },
  {
    accessorKey: 'word_count',
    id: 'words',
    header: '字数',
    enableSorting: true,
    cell: (info: any) => formatWordCount(info.getValue() as number),
  },
  { accessorKey: 'promoted_count', id: 'derived', header: '派生数', enableSorting: true },
  { accessorKey: 'tn_count', id: 'status', header: '状态', enableSorting: true },
  {
    accessorKey: 'parsed_at',
    id: 'parsed',
    header: '解析时间',
    enableSorting: true,
    cell: (info: any) => formatTime(info.getValue() as string),
  },
  { id: 'actions', header: '操作', enableSorting: false },
];
const daWidths: Record<string, number> = {
  title: 220,
  kind: 90,
  source: 240,
  words: 100,
  derived: 90,
  status: 120,
  parsed: 180,
  actions: 200,
};

/// 转换工程列表(TanStack format)。
/// - 不显示 id 列 —— 标题已是主标识,横向 60px 留给标题更划算。
/// - 源列点数据资产标题(从 store.dataAssets join),可点击跳转对应数据资产页。
const tnColumns = [
  { accessorKey: 'title', header: '标题', enableSorting: true },
  {
    accessorKey: 'data_asset_id',
    id: 'source',
    header: '源',
    enableSorting: true,
  },
  {
    accessorKey: 'created_at',
    id: 'created',
    header: '创建时间',
    enableSorting: true,
    cell: (info: any) => formatTime(info.getValue() as string),
  },
  {
    accessorKey: 'workflow_count',
    id: 'workflow',
    header: '工作流',
    enableSorting: true,
    numeric: true,
  },
  { id: 'actions', header: '操作', enableSorting: false },
];
const tnWidths: Record<string, number> = {
  title: 260,
  source: 260,
  workflow: 160,
  created: 170,
  actions: 200,
};

onMounted(() => store.load());
/// Library 同一组件实例服务 3 个 tab(uploads / data-assets / transformations),
/// vue-router 切换 path 不重新 mount,onMounted 只跑一次。
/// 用户在 DataAsset.vue 删除后再切到 /data-assets,得 reload 才能拿到新数据。
watch(() => route.path, () => store.load());

function openCreateTn(dataAssetId: number) {
  tnDialogDataAssetId.value = dataAssetId;
  tnDialogOpen.value = true;
}

// onUpload is the translation layer between Vue-camelCase dialog emit
// and snake-case IPC DTO. Dialog emits { filePath, filename }; we re-pack
// to { file_path, filename } before handing to the store / IPC.
async function onUpload(input: { filePath: string; filename: string }) {
  try {
    await store.upload({ file_path: input.filePath, filename: input.filename });
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onDeleteUpload(id: number, filename: string) {
  deleteUploadId.value = id;
  try {
    const preview = await previewUploadDeletion(id);
    const list = preview.derived_data_assets;
    if (list.length === 0) {
      deleteUploadMessage.value = 'Confirm delete upload "' + filename + '"?';
    } else {
      const lines = ['This upload produced the following data assets (will become orphans, delete them from the DataAssets tab if needed):'];
      for (const item of list) {
        lines.push('  - #' + item.id + ' ' + item.title + ' (' + item.chapters_count + ' chapters, ' + item.tn_count + ' workflows)');
      }
      deleteUploadMessage.value = lines.join('\n');
    }
  } catch (e: unknown) {
    deleteUploadMessage.value = 'Confirm delete upload "' + filename + '"?';
  }
  deleteUploadConfirmOpen.value = true;
}

async function doDeleteUpload() {
  const id = deleteUploadId.value;
  if (id == null) return;
  try {
    await store.removeUpload(id);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

const toast = ref<{ text: string; action: (() => void) | null; actionLabel: string } | null>(null);
/// 5s 自动消失 toast — vueuse useTimeoutFn 自动随组件卸载清理。
const { start: startToastTimer, stop: stopToastTimer } = useTimeoutFn(() => {
  toast.value = null;
}, 5000, { immediate: false });

function showToast(text: string, action: (() => void) | null = null, actionLabel = '查看') {
  stopToastTimer();
  toast.value = { text, action, actionLabel };
  startToastTimer();
}

function dismissToast() {
  stopToastTimer();
  toast.value = null;
}

function onToastAction() {
  const a = toast.value?.action;
  dismissToast();
  if (a) a();
}

async function onCreateTn(input: { data_asset_id: number; title: string }) {
  try {
    const newId = await store.createTransformationNovel(input);
    showToast(`已创建转换工程 "${input.title}"`, () => goDetail(newId));
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

function startRename(id: number, title: string) {
  renamingId.value = id;
  renameDraft.value = title;
}

function cancelRename() {
  renamingId.value = null;
}

async function onSaveRename(id: number) {
  const t = renameDraft.value.trim();
  if (t === '') {
    showAlert('标题不能为空', '请输入转换小说标题。');
    return;
  }
  try {
    await store.renameTransformationNovel({ id, title: t });
    renamingId.value = null;
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onDeleteTn(id: number, title: string) {
  deleteTnId.value = id;
  deleteTnMessage.value = `确认删除转换小说 "${title}"？历史转换结果一并删除。`;
  deleteTnConfirmOpen.value = true;
}

async function doDeleteTn() {
  const id = deleteTnId.value;
  if (id == null) return;
  try {
    await store.removeTransformationNovel(id);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

async function onDeleteDa(id: number, title: string, tnCount: number) {
  deleteDaId.value = id;
  if (tnCount > 0) {
    deleteDaMessage.value = `确认删除数据资产 "${title}"？

该资产被 ${tnCount} 个转换工程引用，删除将会连带删除这些工程及其全部工作流结果。为避免误删，请先去转换工程页删除。`;
  } else {
    deleteDaMessage.value = `确认删除数据资产 "${title}"？解析出的章节将一并删除。`;
  }
  deleteDaConfirmOpen.value = true;
}

async function doDeleteDa() {
  const id = deleteDaId.value;
  if (id == null) return;
  try {
    await store.removeDataAsset(id);
  } catch (e: unknown) {
    showAlert('提示', e instanceof Error ? e.message : String(e));
  }
}

function goUpload(id: number) {
  void router.push({ name: 'upload', params: { uploadId: id } });
}

function goDataAsset(id: number) {
  void router.push({ name: 'data-asset', params: { dataAssetId: id } });
}

function goParse(uploadId: number) {
  void router.push({ name: 'parse-wizard', params: { uploadId } });
}

function goDetail(tnId: number) {
  void router.push({ name: 'transformation-detail', params: { tnId: String(tnId) } });
}
</script>

<style scoped>
.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 16px;
}
.toast {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  margin-bottom: 12px;
  background: var(--color-paper);
  border: 1px solid var(--border-soft);
  border-left: 3px solid var(--color-cinnabar);
  border-radius: var(--radius-pin);
  box-shadow: var(--shadow);
  font-size: 13px;
}
.toast-msg {
  flex: 1;
  color: var(--text-primary);
}
.toast-action {
  background: transparent;
  border: 0;
  padding: 4px 8px;
  font: inherit;
  font-size: 13px;
  color: var(--color-slate);
  cursor: pointer;
  text-decoration: underline transparent;
  text-underline-offset: 3px;
  transition: color 120ms ease, text-decoration-color 120ms ease;
}
.toast-action:hover {
  color: var(--accent);
  text-decoration-color: currentColor;
}
.toast-close {
  background: transparent;
  border: 0;
  padding: 0 4px;
  font: inherit;
  font-size: 18px;
  line-height: 1;
  color: var(--text-muted);
  cursor: pointer;
}
.toast-close:hover {
  color: var(--text-primary);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 200ms ease, transform 200ms ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
.empty {
  text-align: center;
  padding: 56px 0;
  color: var(--text-secondary);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
}
.muted {
  color: var(--text-secondary);
  font-size: 12px;
}
.source-link {
  color: inherit;
}
.cell-truncate {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
