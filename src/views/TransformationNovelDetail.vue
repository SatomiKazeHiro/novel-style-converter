<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useDynamicTableHeight } from '../composables/useDynamicTableHeight';
import { useRoute, useRouter } from 'vue-router';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { useWorkflowsStore } from '../stores/workflows';
import {
  getChapter as ipcGetChapter,
  getTransformationNovel,
  listDataAssets,
  listTransformationSourceChapters,
  listWorkflows,
  listWorkflowChapters,
  listChapterWorkflowResults,
} from '../ipc/commands';
import type {
  TransformationNovelSummary,
  DataAssetRow,
  WorkflowSummary,
  WorkflowChapterRow,
  ChapterWorkflowResultRow,
  Chapter,
  TransformStatus,
  WorkflowStatus,
  CreateWorkflowInput,
  SourceChapterRow,
} from '../ipc/types';
import Button from '../components/ui/Button.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import IconArrowLeft from '~icons/lucide/arrow-left';
import IconAlertTriangle from '~icons/lucide/alert-triangle';
import { countWords, formatTime, formatWordCount } from '../utils/format';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import CreateBatchDialog from '../components/CreateBatchDialog.vue';
import PromoteWorkflowDialog from '../components/PromoteWorkflowDialog.vue';
import AppendChaptersDialog from '../components/AppendChaptersDialog.vue';
import Dialog from '../components/ui/Dialog.vue';
import DataTable from '../components/ui/DataTable.vue';
import Tag from '../components/ui/Tag.vue';
import { formatBatchStatus, formatChapterStatus } from '../utils/status-locale';
import RegeneratePreviewDialog from '../components/RegeneratePreviewDialog.vue';
const route = useRoute();
const router = useRouter();
const tnId = computed(() => Number(route.params.tnId));
/// 当前 tn 摘要:onMounted 里拉一次,详情页生命周期内不会变(没编辑入口)。
/// 拉不到时不静默 fallback,直接报错让用户看到。
const tnSummary = ref<TransformationNovelSummary | null>(null);
const tnError = ref<string | null>(null);
const tnTitle = computed(() => tnSummary.value?.title ?? '');
/// 源数据资产(从 listDataAssets 里按 tnSummary.data_asset_id 查)给 meta-strip 用。
const sourceAssets = ref<DataAssetRow[]>([]);
const sourceAsset = computed<DataAssetRow | null>(() => {
  if (tnSummary.value === null) return null;
  return sourceAssets.value.find((a) => a.id === tnSummary.value!.data_asset_id) ?? null;
});
async function loadTn() {
  tnError.value = null;
  try {
    const [tn, assets] = await Promise.all([
      getTransformationNovel(tnId.value),
      listDataAssets(),
    ]);
    tnSummary.value = tn;
    sourceAssets.value = assets;
  } catch (e: unknown) {
    tnError.value = e instanceof Error ? e.message : String(e);
  }
}

const store = useWorkflowsStore();
const queryClient = useQueryClient();

const activeTab = ref<'chapters' | 'workflows'>('chapters');

/// 章节来源 tab 表格列(TanStack format)
const sourceColumns = [
  { id: 'pick', header: '', enableSorting: false },
  { accessorKey: 'idx', id: 'idx', header: '#', enableSorting: true },
  { accessorKey: 'title', header: '标题', enableSorting: true },
  { accessorKey: 'word_count', id: 'words', header: '字数', enableSorting: true },
  { accessorKey: 'non_empty_result_count', id: 'result_count', header: '已有结果数', enableSorting: true },
];
const sourceWidths: Record<string, number> = {
  pick: 40,
  idx: 60,
  title: 280,
  words: 100,
  result_count: 120,
};

/// 工作流 tab 表格列
const workflowColumns = [
  { accessorKey: 'label', header: '标签', enableSorting: true },
  { id: 'status', header: '状态', enableSorting: true },
  { accessorKey: 'total_count', id: 'total', header: '总章数', enableSorting: true },
  { accessorKey: 'done_count', id: 'done', header: '已完成', enableSorting: true },
  { accessorKey: 'failed_count', id: 'failed', header: '失败', enableSorting: true },
  { accessorKey: 'skipped_count', id: 'skipped', header: '已跳过', enableSorting: true },
  { accessorKey: 'created_at', id: 'created', header: '创建时间', enableSorting: true },
  { accessorKey: 'ended_at', id: 'ended', header: '结束时间', enableSorting: true },
  { id: 'actions', header: '操作', enableSorting: false },
];
const workflowWidths: Record<string, number> = {
  label: 180,
  status: 140,
  total: 80,
  done: 80,
  failed: 80,
  skipped: 80,
  created: 160,
  ended: 160,
  // 详情 + 补充章节 + 转正 + 删除 = 4 个 row-link + 3 个 separator,200 偏紧会换行。
  actions: 280,
};

/// 工作流详情表格列
const workflowChapterColumns = [
  { id: 'pick', header: '', enableSorting: false },
  { accessorKey: 'chapter_title', id: 'title', header: '标题', enableSorting: true },
  { id: 'status', header: '状态', enableSorting: true },
  { accessorKey: 'content_preview', id: 'preview', header: '结果预览', enableSorting: false },
  { id: 'actions', header: '操作', enableSorting: false },
];
const workflowChapterWidths: Record<string, number> = {
  pick: 40,
  title: 220,
  status: 110,
  preview: 320,
  actions: 200,
};

// 章节来源 tab
const selectedChapterIds = ref<Set<number>>(new Set());
const openSourceChapterId = ref<number | null>(null);

/// 章节来源 — vue-query 自动按 tnId 缓存,5s 轮询 non_empty_result_count 实时刷新进度。
const sourcesQuery = useQuery({
  queryKey: ['transformationSourceChapters', tnId],
  queryFn: () => listTransformationSourceChapters(tnId.value),
  refetchInterval: 5000,
});
const sources = computed<SourceChapterRow[]>(() => sourcesQuery.data.value ?? []);
const selectedCount = computed(() => selectedChapterIds.value.size);

/// 预览章节 id(selectedChapterIds 中 idx 最小的那个)。spec §6.2:固定为 idx 最小者,
/// 不暴露切换 UI(idx=0 没有前文是"空前文"场景,与 idx>0 时不一致——是有意取舍)。
const previewChapterId = computed<number | null>(() => {
  const ids = selectedChapterIds.value;
  if (ids.size === 0) return null;
  // sources 已是 idx ASC 顺序
  const match = sources.value.find((s) => ids.has(s.chapter_id));
  return match ? match.chapter_id : null;
});

function toggleSelect(chapterId: number, on: boolean) {
  const next = new Set(selectedChapterIds.value);
  if (on) next.add(chapterId); else next.delete(chapterId);
  selectedChapterIds.value = next;
}

/// 自定义范围选择:按 idx(#列序号)对一段章节做"覆盖"或"累加/取反"。
/// 用户视角就是 "#N 到 #M",内部映射回 chapter_id 写回 selectedChapterIds。
/// - replace:把选中集合直接替换为范围内的章节(经典覆盖)。
/// - toggle  :把范围内的每个 chapter 在当前选中集合中取反,范围外的不变
///            → 多次应用可累加:先 1~100,再 200~300,得到 1~300;再 50~150,
///            得到 1~49 + 151~300。这是用户实际选长篇小说的常用模式。
type RangeMode = 'replace' | 'toggle';
const rangeMode = ref<RangeMode>('replace');
const rangeFrom = ref<number | null>(null);
const rangeTo = ref<number | null>(null);
/// 输入合法性:不能 < 1,不能 > list.length,任一不满足给错误样式并禁用"应用"。
const rangeError = computed<string | null>(() => {
  const list = sources.value;
  if (list.length === 0) return null;
  if (rangeFrom.value === null || rangeTo.value === null) return null;
  if (rangeFrom.value < 1 || rangeTo.value < 1) return '序号需 ≥ 1';
  if (rangeFrom.value > list.length || rangeTo.value > list.length) {
    return `序号需 ≤ ${list.length}`;
  }
  return null;
});
function applyRange() {
  const list = sources.value;
  if (list.length === 0) return;
  if (rangeError.value !== null) return;
  if (rangeFrom.value === null || rangeTo.value === null) return;
  // to < from 自动交换,idx 单调。
  const lo = Math.min(rangeFrom.value, rangeTo.value);
  const hi = Math.max(rangeFrom.value, rangeTo.value);
  const targetIds = new Set<number>(
    list.filter((s) => s.idx >= lo && s.idx <= hi).map((s) => s.chapter_id),
  );
  if (rangeMode.value === 'replace') {
    selectedChapterIds.value = targetIds;
  } else {
    // toggle:范围内每个 id 在当前集合里 add/delete 翻转,范围外不动。
    const next = new Set(selectedChapterIds.value);
    for (const id of targetIds) {
      if (next.has(id)) next.delete(id); else next.add(id);
    }
    selectedChapterIds.value = next;
  }
}
function clearSelection() {
  selectedChapterIds.value = new Set();
}

function selectAll() {
  selectedChapterIds.value = new Set(sources.value.map((s) => s.chapter_id));
}
function selectNone() {
  selectedChapterIds.value = new Set();
}

// 工作流 tab
/// 工作流列表 — vue-query 5s 轮询;create/stop/delete/promote 后 store 自动 invalidate。
const workflowsQuery = useQuery({
  queryKey: ['workflows', tnId],
  queryFn: () => listWorkflows(tnId.value),
  refetchInterval: 5000,
});
const workflows = computed<WorkflowSummary[]>(() => workflowsQuery.data.value ?? []);
/// 章节来源 tab 表格自适应高度:监听 `main.app` 的尺寸变化,实时算出
/// 章节来源 Tab 表格 + 工作流 Tab 表格共用 composable,统一从 main.app 计算可用高度。

/// 章节来源 Tab 表格自适应高度
const chaptersTableEl = ref<HTMLElement | null>(null);
const { maxHeight: chaptersTableMaxHeight } = useDynamicTableHeight({
  tableEl: chaptersTableEl,
  minHeight: 300,
  deps: [() => sources.value.length, activeTab],
});

/// 工作流 Tab 表格自适应高度 —— 同样跟随 main.app + tab 切换重算
const workflowsTableEl = ref<HTMLElement | null>(null);
const { maxHeight: workflowsTableMaxHeight } = useDynamicTableHeight({
  tableEl: workflowsTableEl,
  minHeight: 300,
  deps: [() => workflows.value.length, activeTab],
});
const selectedWorkflowId = ref<number | null>(null);
/// 工作流章节 — vue-query 2s 轮询,enabled 跟随 selectedWorkflowId。
/// 章节级 2s 轮询 —— vue-query 的 refetchInterval 函数式声明:
/// - 任一章节 pending/running → 2s 轮询,UI 实时反映进度。
/// - 全 done/failed/skipped/cancelled → false 停,减少无意义 IPC。
/// 同时消除原 isBatchLive + useIntervalFn 手动 pause/resume 的状态机。
const selectedWorkflowChaptersQuery = useQuery({
  queryKey: ['workflowChapters', selectedWorkflowId],
  queryFn: () => listWorkflowChapters(selectedWorkflowId.value!),
  enabled: computed(() => selectedWorkflowId.value !== null),
  refetchInterval: (query) => {
    const data = query.state.data as WorkflowChapterRow[] | undefined;
    if (data === undefined) return 2000;
    const hasActive = data.some((c) => c.status === 'pending' || c.status === 'running');
    return hasActive ? 2000 : false;
  },
});
const selectedWorkflowChapters = computed<WorkflowChapterRow[]>(() => selectedWorkflowChaptersQuery.data.value ?? []);
const selectedWorkflow = computed<WorkflowSummary | null>(() => {
  if (selectedWorkflowId.value === null) return null;
  return workflows.value.find((w) => w.id === selectedWorkflowId.value) ?? null;
});


/// 章节Detail侧边栏(章节来源 tab 用)
/// on-demand:只在用户点击章节行打开侧栏时拉取,关闭后自动停。
/// vue-query 的 enabled: false 时不订阅、不缓存、不占用网络。
const openSourceResultsQuery = useQuery({
  queryKey: ['chapterWorkflowResults', tnId, openSourceChapterId],
  queryFn: () => listChapterWorkflowResults(tnId.value, openSourceChapterId.value!),
  enabled: computed(() => openSourceChapterId.value !== null),
});
const openSourceResults = computed<ChapterWorkflowResultRow[]>(() => openSourceResultsQuery.data.value ?? []);

const stopConfirmOpen = ref(false);
const stopTargetId = ref<number | null>(null);
const retrySelectedIds = ref<Set<number>>(new Set());

/// 通用错误/提示弹窗 —— 后端报错或前置校验失败时统一弹出,不再静默 console.error。
const alertOpen = ref(false);
const alertTitle = ref('提示');
const alertMessage = ref('');
function showAlert(title: string, message: string) {
  alertTitle.value = title;
  alertMessage.value = message;
  alertOpen.value = true;
}

// Toast —— 仿照 Library.vue:433 的写法,showToast(text, action, actionLabel?)。
// 用于转正成功后提示「已转正为「X」」并提供「查看」跳转入口。
const toast = ref<{ text: string; action: (() => void) | null; actionLabel: string } | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;
function stopToastTimer() {
  if (toastTimer !== null) { clearTimeout(toastTimer); toastTimer = null; }
}
function startToastTimer() {
  stopToastTimer();
  toastTimer = setTimeout(() => { toast.value = null; toastTimer = null; }, 5000);
}
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

/// 跳到 data-asset 详情页。仿照 Library.vue:524 的 goDataAsset。
function goDataAsset(id: number) {
  void router.push({ name: 'data-asset', params: { dataAssetId: id } });
}

/// 跳到数据资产列表 tab —— 用于「转正 × N」badge 点击后让用户看到所有 da。
function goDataAssetsList() {
  void router.push('/data-assets');
}

// 工作流转正 —— 触发源已移到外面的列表 actions 列,故不绑定 modal 的 selectedWorkflow。
// promoteTargetId 独立记录当前在转正的 workflow,转正期间 modal 是否打开无关。
const promoteTargetId = ref<number | null>(null);
const promoteTarget = computed<WorkflowSummary | null>(() => {
  if (promoteTargetId.value === null) return null;
  return workflows.value.find((w) => w.id === promoteTargetId.value) ?? null;
});
const promoteOpen = ref(false);
const promoteSubmitting = ref(false);
const promoteError = ref<string | null>(null);

function openPromoteDialog(w: WorkflowSummary) {
  promoteTargetId.value = w.id;
  promoteError.value = null;
  promoteOpen.value = true;
}
async function confirmPromote(title: string) {
  const tid = promoteTargetId.value;
  if (tid === null) return;
  promoteSubmitting.value = true;
  promoteError.value = null;
  try {
    const newDa = await store.promote(tid, title);
    promoteOpen.value = false;
    promoteTargetId.value = null;
    // 成功反馈:toast 提示 + 「查看」按钮跳到新 da 详情。仿照 Library.vue:453 的 onCreateTn。
    showToast(`已转正为「${newDa.title}」`, () => goDataAsset(newDa.id));
  } catch (e: unknown) {
    promoteError.value = e instanceof Error ? e.message : String(e);
  } finally {
    promoteSubmitting.value = false;
  }
}

// 章节Detail侧边栏的源原文
const sourceChapterDetail = ref<Chapter | null>(null);
const sourceChapterText = ref<string>('');
const sourceChapterLoading = ref(false);

function toggleRetrySelection(tcId: number, on: boolean) {
  const next = new Set(retrySelectedIds.value);
  if (on) next.add(tcId); else next.delete(tcId);
  retrySelectedIds.value = next;
}

async function openChapterPanel(chapterId: number) {
  openSourceChapterId.value = chapterId;
  sourceChapterDetail.value = null;
  sourceChapterText.value = '';
  // 章节结果由 openSourceResultsQuery (enabled=true) 自动拉取
  sourceChapterLoading.value = true;
  try {
    const ch = await ipcGetChapter(chapterId);
    sourceChapterDetail.value = ch;
    sourceChapterText.value = ch.body;
  } catch (e: unknown) {
    console.error(e);
  } finally {
    sourceChapterLoading.value = false;
  }
}

function closeChapterPanel() {
  openSourceChapterId.value = null;
  sourceChapterDetail.value = null;
  sourceChapterText.value = '';
}

/// selectedWorkflowId 变更会自动触发 selectedWorkflowChaptersQuery (vue-query enabled + 2s 轮询),无需手动 loadChapters。
async function openWorkflowPanel(w: WorkflowSummary) {
  selectedWorkflowId.value = w.id;
  retrySelectedIds.value = new Set();
}

function closeWorkflowPanel() {
  selectedWorkflowId.value = null;
  retrySelectedIds.value = new Set();
}

// Chapter Detail modal (within Workflow Detail modal)
const detailChapter = ref<WorkflowChapterRow | null>(null);
const detailLoading = ref(false);
const detailTransformed = ref<string | null>(null);
/// 转换结果的状态 tag —— 与 detailTransformed 同步设置/清空。类型用 TransformStatus 让模板能用 formatChapterStatus 转中文。
const detailTransformedStatus = ref<TransformStatus | null>(null);
const detailSourceWordCount = computed<number>(() => sourceChapterDetail.value?.word_count ?? 0);
const detailTransformedWordCount = computed<number>(() => detailTransformed.value === null ? 0 : countWords(detailTransformed.value));
/// Chapter Detail 弹窗:在 Workflow Detail 弹窗内显示单章 source/transformed 对照。
/// queryClient.fetchQuery:打开时不订阅,只是同步拿到结果后填充 detailTransformed。
async function openChapterDetail(c: WorkflowChapterRow) {
  detailChapter.value = c;
  detailTransformed.value = null;
  detailTransformedStatus.value = null;
  sourceChapterDetail.value = null;
  sourceChapterText.value = '';
  detailLoading.value = true;
  sourceChapterLoading.value = true;
  try {
    const [, list] = await Promise.all([
      ipcGetChapter(c.chapter_id).then((ch) => {
        sourceChapterDetail.value = ch;
        sourceChapterText.value = ch.body;
      }),
      queryClient.fetchQuery<ChapterWorkflowResultRow[]>({
        queryKey: ['chapterWorkflowResults', tnId, c.chapter_id],
        queryFn: () => listChapterWorkflowResults(tnId.value, c.chapter_id),
      }),
    ]);
    const match = list.find((r) => r.batch_id === selectedWorkflowId.value);
    detailTransformed.value = match?.content ?? null;
    detailTransformedStatus.value = match?.status ?? null;
  } catch (e: unknown) {
    console.error(e);
  } finally {
    detailLoading.value = false;
    sourceChapterLoading.value = false;
  }
}

function closeChapterDetail() {
  detailChapter.value = null;
  detailTransformed.value = null;
  detailTransformedStatus.value = null;
}

async function retryFromDetail() {
  const blocked = batchRetryBlockedReason.value;
  if (blocked !== null) {
    showAlert('无法重试', blocked);
    return;
  }
  const c = detailChapter.value;
  if (c === null || selectedWorkflowId.value === null) return;
  retrySubmitting.value = true;
  try {
    // store.retry 自动 invalidate selectedWorkflowChapters,无需手动 loadChapters
    await store.retry(selectedWorkflowId.value, [c.chapter_id]);
    closeChapterDetail();
  } catch (e: unknown) {
    console.error(e);
  } finally {
    retrySubmitting.value = false;
  }
}

// “重新生成”对话框的目标章节——null 表示对话框关闭
const regenChapter = ref<WorkflowChapterRow | null>(null);
const regenOpenProxy = computed<boolean>({
  get: () => regenChapter.value !== null,
  set: (v: boolean) => {
    if (!v) regenChapter.value = null;
  },
});

function openRegenerateDialog(row: WorkflowChapterRow) {
  // 防御性校验：running/pending 不允许打开（与按钮 disabled 对齐）
  if (row.status === 'running' || row.status === 'pending') return;
  regenChapter.value = row;
}

async function onPreviewCommitted() {
  // store.commitPreview 已 invalidate [workflowChapters,batchId] + [workflows],无需手动 loadChapters。
  // 此函数保留为 RegeneratePreviewDialog 的事件 hook 出口。
  void selectedWorkflowId.value;
}

// 单章节重试（仅 failed/skipped + 空槽，与 canRetryChapter 一致）
async function retrySingleChapter(c: WorkflowChapterRow) {
  if (selectedWorkflowId.value === null) return;
  if (!canRetryChapter(c)) return;
  const blocked = batchRetryBlockedReason.value;
  if (blocked !== null) {
    showAlert('无法重试', blocked);
    return;
  }
  retrySubmitting.value = true;
  try {
    // store.retry 自动 invalidate selectedWorkflowChapters,无需手动 loadChapters
    await store.retry(selectedWorkflowId.value, [c.chapter_id]);
  } catch (e: unknown) {
    showAlert('重试失败', e instanceof Error ? e.message : String(e));
  } finally {
    retrySubmitting.value = false;
  }
}

// 失败详情弹窗 —— 与"详情"分开语义:详情看 source/transformed,失败详情只看错误。
const failureDetailChapter = ref<WorkflowChapterRow | null>(null);
function openFailureDetail(row: WorkflowChapterRow) {
  failureDetailChapter.value = row;
}
function closeFailureDetail() {
  failureDetailChapter.value = null;
}
async function retryFromFailureDetail() {
  const ch = failureDetailChapter.value;
  if (ch === null) return;
  await retrySingleChapter(ch);
  failureDetailChapter.value = null;
}

function askStopWorkflow(id: number) {
  stopTargetId.value = id;
  stopConfirmOpen.value = true;
}

// 工作流删除:仅 stopped/completed/terminated/cancelled 状态可删(running/pending/paused 由后端拒绝)。
// UI 层再做一次前置校验:不允许误触发正在跑的工作流。
const DELETEABLE_STATUSES = new Set(['stopped', 'completed', 'terminated', 'cancelled']);
const deleteConfirmOpen = ref(false);
const deleteTargetId = ref<number | null>(null);
const deleteTargetLabel = ref<string>('');
const deleteTargetPromotedCount = ref<number>(0);
const deleteSubmitting = ref(false);
const deleteError = ref<string | null>(null);

/// 删除确认弹窗的 message。promoted_count > 0 时重点提示:已派生 da 的来源会被抹掉。
const deleteConfirmMessage = computed<string>(() => {
  const n = deleteTargetPromotedCount.value;
  const label = deleteTargetLabel.value;
  const base = `确认删除 ${label}?\n此操作不可撤销 —— 工作流、所有章节结果、转换记录都会被删除。`;
  if (n > 0) {
    return base + `\n已有 ${n} 份数据资产从此工作流派生，删除后它们的来源字段会被清空(数据资产本身保留)。`;
  }
  return base + (deleteError.value ? `\n\n${deleteError.value}` : '');
});
function askDeleteWorkflow(w: WorkflowSummary) {
  if (!DELETEABLE_STATUSES.has(w.status)) return;
  deleteTargetId.value = w.id;
  deleteTargetLabel.value = w.label ?? `工作流 #${w.id}`;
  deleteTargetPromotedCount.value = w.promoted_count;
  deleteError.value = null;
  deleteConfirmOpen.value = true;
}

async function confirmDeleteWorkflow() {
  const id = deleteTargetId.value;
  if (id === null) return;
  deleteSubmitting.value = true;
  deleteError.value = null;
  try {
    const res = await store.deleteWorkflow(id);
    if (selectedWorkflowId.value === id) closeWorkflowPanel();
    deleteConfirmOpen.value = false;
    deleteTargetId.value = null;
    if (res.promoted_data_asset_count > 0) {
      console.info(`[delete_workflow] 已抹掉 ${res.promoted_data_asset_count} 份数据资产的来源工作流字段`);
    }
  } catch (e: unknown) {
    deleteError.value = e instanceof Error ? e.message : String(e);
  } finally {
    deleteSubmitting.value = false;
  }
}
async function confirmStopWorkflow() {
  const id = stopTargetId.value;
  if (id === null) return;
  try {
    // store.stop 自动 invalidate [workflowChapters,batchId] + [workflows],无需手动 loadChapters
    await store.stop(id);
  } catch (e: unknown) {
    console.error(e);
  }
  stopTargetId.value = null;
}

// 「补充章节」对话框状态 —— 仅 stopped batch 可 append(spec:stopped-batch-append-chapters)。
// 父组件保存当前在 append 的 batch + dialog 开关;Dialog 内部走 store.appendChapters 实际提交。
const appendOpen = ref(false);
const appendTarget = ref<WorkflowSummary | null>(null);

function askAppendChapters(w: WorkflowSummary) {
  // 仅 stopped 可 append —— 其他状态后端会拒,UI 层先关掉入口。
  if (w.status !== 'stopped') return;
  appendTarget.value = w;
  appendOpen.value = true;
}

async function onAppendConfirm(payload: { batchId: number; chapterIds: number[] }) {
  try {
    // store.appendChapters 内部已 invalidate ['workflowChapters', batchId] + ['workflows'],
    // 父组件无需手动刷新。
    await store.appendChapters(payload);
    appendOpen.value = false;
    appendTarget.value = null;
  } catch (e: unknown) {
    showAlert('补充失败', e instanceof Error ? e.message : String(e));
  }
}

function canRetryChapter(c: WorkflowChapterRow): boolean {
  return c.status === 'failed' || c.status === 'skipped';
}

/// 后端 retry_empty_slots 允许集:Stopped / (Running|Paused 且 in-flight=0)。
/// Terminated/Cancelled/Completed/Pending 或还有 in-flight 时,后端会抛 Validation。
/// UI 提前禁用并说明原因,避免点了再被后端拒绝(bugfix: 重试按钮在 Terminated 等终态可点)。
/// in-flight 判定以章节 status='running' 计数,与 batch_scheduler.rs 的 SQL 一致。
const BATCH_RETRY_BLOCK_REASON: Record<WorkflowStatus, string | null> = {
  pending:    '工作流尚未开始,无可重试章节',
  running:    null, // 由 in-flight 进一步判断
  stopped:    null,
  paused:     null, // 由 in-flight 进一步判断
  completed:  '工作流已完成,无可重试章节',
  terminated: '工作流已终止,无法重试。如需重新转换,请新建工作流',
  cancelled:  '工作流已取消,无法重试。如需重新转换,请新建工作流',
};
const batchRetryBlockedReason = computed<string | null>(() => {
  const w = selectedWorkflow.value;
  if (w === null) return null;
  const staticReason = BATCH_RETRY_BLOCK_REASON[w.status];
  if (staticReason !== null) return staticReason;
  // running / paused: 仅当无 in-flight 时可重试
  if (w.status === 'running' || w.status === 'paused') {
    const hasInFlight = selectedWorkflowChapters.value.some((c) => c.status === 'running');
    return hasInFlight ? '有章节仍在转换中,请等待完成后再试' : null;
  }
  return null;
});

const retrySubmitting = ref(false);
/// 显示/启用"重试所选"按钮的前提:batch 状态允许重试(由 batchRetryBlockedReason 决定)。
/// 之前写死 batch.status === 'stopped' 太严,会漏掉 running/paused 无 in-flight 的合法场景。
const canRetrySelection = computed<boolean>(() => {
  if (batchRetryBlockedReason.value !== null) return false;
  return retrySelectedIds.value.size > 0;
});
async function doRetry() {
  if (selectedWorkflowId.value === null) return;
  const blocked = batchRetryBlockedReason.value;
  if (blocked !== null) {
    showAlert('无法重试', blocked);
    return;
  }
  const chapterIds = selectedWorkflowChapters.value
    .filter((c) => retrySelectedIds.value.has(c.tc_id))
    .map((c) => c.chapter_id);
  if (chapterIds.length === 0) return;
  retrySubmitting.value = true;
  try {
    // store.retry 自动 invalidate selectedWorkflowChapters,无需手动 loadChapters
    await store.retry(selectedWorkflowId.value, chapterIds);
    retrySelectedIds.value = new Set();
  } catch (e: unknown) {
    showAlert('重试失败', e instanceof Error ? e.message : String(e));
  } finally {
    retrySubmitting.value = false;
  }
}

function fmtTime(s: string | null | undefined): string {
  return formatTime(s);
}

// 新建工作流弹窗
const createBatchOpen = ref(false);
const createBatchDefaults = ref<{
  default_prompt_id: number | null;
  default_model_config_id: number | null;
  default_mode: 'compress' | 'style' | null;
}>({ default_prompt_id: null, default_model_config_id: null, default_mode: null });
// 当前 tn 的默认值(从最近一个 workflow / 自身 default 取),简化处理:从已有 workflow 继承。
function openCreateBatch() {
  // 简单复用最近一个 workflow 的 prompt/model 字段作默认,缺失时为 null。
  const recent = workflows.value[0];
  createBatchDefaults.value = {
    default_prompt_id: null,
    default_model_config_id: null,
    default_mode: null,
    ...(recent ? {
      // 这里没有 workflow row 本身的 prompt/model 字段(见 WorkflowSummary),
      // 所以保留 null,由用户挑选;prompt list 仍会按 default_mode 过滤。
    } : {}),
  };
  createBatchOpen.value = true;
  createBatchError.value = null;
}

const createBatchError = ref<string | null>(null);

async function onCreateBatch(payload: CreateWorkflowInput) {
  try {
    const w = await store.create(payload);
    selectedChapterIds.value = new Set();
    activeTab.value = 'workflows';
    await openWorkflowPanel(w);
    createBatchError.value = null;
  } catch (e: unknown) {
    createBatchError.value = e instanceof Error ? e.message : String(e);
  } finally {
    createBatchOpen.value = false;
  }
}

/// 章节来源表格表头全选/全不选。
function onToggleAll(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  if (checked) selectAll();
  else selectNone();
}

/// 工作流章节:只勾选"可重试"且"空槽位"的行,这样按钮才有效。
const retryableCount = computed<number>(() =>
  selectedWorkflowChapters.value.filter((c) => canRetryChapter(c) && c.is_empty_slot).length,
);
function onToggleAllRetry(e: Event) {
  const checked = (e.target as HTMLInputElement).checked;
  const next = new Set<number>();
  if (checked) {
    for (const c of selectedWorkflowChapters.value) {
      if (canRetryChapter(c) && c.is_empty_slot) next.add(c.tc_id);
    }
  }
  retrySelectedIds.value = next;
}

onMounted(async () => {
  // sources/workflows 由 vue-query 自动订阅(refetchInterval: 5s),无需手动加载。
  // 表格高度由 useDynamicTableHeight composable 自动监听 main.app 尺寸变化,无需手动管理。
  await loadTn();
});

watch(() => workflows.value, (list) => {
  if (selectedWorkflowId.value === null) return;
  if (!list.find((w) => w.id === selectedWorkflowId.value)) {
    selectedWorkflowId.value = null;
  }
});

// spec §9.1: 默认Select All。sources 第一次加载后初始化 selectedChapterIds,
// 后续用户手动Actions不会被覆盖。
let didInitSelection = false;
watch(() => sources.value, (list) => {
  if (didInitSelection || list.length === 0) return;
  selectedChapterIds.value = new Set(list.map((s) => s.chapter_id));
  didInitSelection = true;
}, { immediate: true });
</script>

<template>
  <section class="tn-detail">
    <PageHeader :title="tnTitle || '加载中...'" size="small">
      <template #back>
        <Button aria-label="返回" @click="router.back()">
          <IconArrowLeft :size="16" :stroke-width="1.5" />
        </Button>
      </template>
      <template #actions>
        <!-- 占位:未来若加"重命名"等动作放这里 -->
      </template>
    </PageHeader>

    <Transition name="toast">
      <div v-if="toast" class="toast">
        <span class="toast-msg">{{ toast.text }}</span>
        <button v-if="toast.action" type="button" class="toast-action" @click="onToastAction">{{ toast.actionLabel }}</button>
        <button type="button" class="toast-close" aria-label="关闭" @click="dismissToast">×</button>
      </div>
    </Transition>

    <div v-if="tnError" class="alert">{{ tnError }}</div>

    <div v-if="tnSummary" class="meta-strip">
      <div class="tags">
        <Tag>转换工程</Tag>
        <span v-if="sourceAsset" class="badge">来自「{{ sourceAsset.title }}」</span>
      </div>
      <div class="meta-text">
        <span>{{ formatTime(tnSummary.created_at) }}</span>
        <span v-if="tnSummary.chapters_count > 0" class="src">共 {{ tnSummary.chapters_count }} 章</span>
        <span v-if="tnSummary.note" class="src" :title="tnSummary.note">备注:{{ tnSummary.note }}</span>
      </div>
    </div>

    <div class="tabs">
      <button :class="{ active: activeTab === 'chapters' }" @click="activeTab = 'chapters'">
        章节来源
      </button>
      <button :class="{ active: activeTab === 'workflows' }" @click="activeTab = 'workflows'">
        工作流
      </button>
    </div>

    <!-- 章节来源 tab -->
    <template v-if="activeTab === 'chapters'">
      <div class="actions">
        <div class="range-pick">
          <span class="range-label">按 # 选</span>
          <input
            type="number"
            class="range-input"
            :class="{ 'has-error': rangeError !== null }"
            v-model.number="rangeFrom"
            :min="1"
            :max="sources.length"
            placeholder="起"
            :disabled="sources.length === 0"
            @keydown.enter="applyRange"
          />
          <span class="range-sep">—</span>
          <input
            type="number"
            class="range-input"
            :class="{ 'has-error': rangeError !== null }"
            v-model.number="rangeTo"
            :min="1"
            :max="sources.length"
            placeholder="止"
            :disabled="sources.length === 0"
            @keydown.enter="applyRange"
          />
          <select
            v-model="rangeMode"
            class="range-mode"
            :disabled="sources.length === 0"
            :title="rangeMode === 'toggle' ? '范围内每项取反选中状态,范围外不变 — 多次应用可累加' : '直接把选中集合替换为范围内的章节'"
          >
            <option value="toggle">累加/取反</option>
            <option value="replace">覆盖</option>
          </select>
          <Button
            size="small"
            :disabled="rangeFrom === null || rangeTo === null || rangeError !== null"
            :title="rangeError ?? ''"
            @click="applyRange"
          >
            应用
          </Button>
          <Button
            size="small"
            :disabled="selectedCount === 0"
            @click="clearSelection"
          >
            清空
          </Button>
        </div>
        <Button
          kind="primary"
          size="small"
          :disabled="selectedCount === 0"
          @click="openCreateBatch"
        >
          新建工作流 ({{ selectedCount }} 章）
        </Button>
      </div>
      <div ref="chaptersTableEl" class="table-wrap">
        <DataTable
          v-if="sources.length > 0"
          :columns="sourceColumns"
          :data="sources"
          :row-key="(row: SourceChapterRow) => row.chapter_id"
          :widths="sourceWidths"
          :numeric-columns="['idx', 'words', 'result_count']"
          :max-height="chaptersTableMaxHeight"
          empty-text="暂无章节"
        >
        <template #header-pick>
          <input
            type="checkbox"
            :checked="selectedChapterIds.size === sources.length && sources.length > 0"
            :indeterminate.prop="selectedChapterIds.size > 0 && selectedChapterIds.size < sources.length"
            aria-label="全选章节"
            @click.stop
            @change="onToggleAll($event)"
          />
        </template>
        <template #cell-pick="{ row }">
          <input
            type="checkbox"
            :checked="selectedChapterIds.has(row.chapter_id)"
            @change="(e) => toggleSelect(row.chapter_id, (e.target as HTMLInputElement).checked)"
          />
        </template>
        <template #cell-title="{ row }">
          <button class="link-btn" @click="openChapterPanel(row.chapter_id)">{{ row.title }}</button>
        </template>
        </DataTable>
        <div v-else class="empty">暂无章节</div>
      </div>
    </template>

    <!-- 工作流 tab -->
    <template v-else>
      <div v-if="workflows.length > 0" ref="workflowsTableEl" class="table-wrap">
      <DataTable
        :columns="workflowColumns"
        :data="workflows"
        :row-key="(row: WorkflowSummary) => row.id"
        :widths="workflowWidths"
        :numeric-columns="['total', 'done', 'failed', 'skipped']"
        :max-height="workflowsTableMaxHeight"
        frozen-column="actions"
        empty-text="尚无工作流"
      >
        <template #cell-status="{ row }">
          <div class="cell-status">
<span class="status" :class="row.status">{{ formatBatchStatus(row.status) }}</span>
            <button
              v-if="row.promoted_count > 0"
              type="button"
              class="promoted-tag promoted-tag-link"
              :title="`已基于此工作流转正 ${row.promoted_count} 份数据资产,点击查看`"
              @click="goDataAssetsList"
            >转正 × {{ row.promoted_count }}</button>
          </div>
        </template>
        <template #cell-created="{ row }">
          {{ fmtTime(row.created_at) }}
        </template>
        <template #cell-ended="{ row }">
          {{ fmtTime(row.ended_at) }}
        </template>
        <template #cell-actions="{ row }">
          <button type="button" class="row-link" @click="openWorkflowPanel(row)">详情</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <!-- 补充章节:仅 stopped 可 append,沿用 batch 同质配置(prompt / model / ctx)。
               askAppendChapters 在 UI 层再做一次前置校验,与按钮 disabled 对齐。 -->
          <button
            type="button"
            class="row-link"
            :disabled="row.status !== 'stopped'"
            :title="row.status === 'stopped' ? '从 source data_asset 选若干章节追加到此 batch' : '该工作流尚未停止,无法追加章节'"
            @click="askAppendChapters(row)"
          >补充章节</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button
            type="button"
            class="row-link"
            :disabled="row.status !== 'stopped'"
            :title="row.status === 'stopped' ? '' : '该工作流尚未停止,无法转正'"
            @click="openPromoteDialog(row)"
          >转正</button>
          <span class="row-sep" aria-hidden="true">·</span>
          <button
            type="button"
            class="row-link danger"
            :disabled="!DELETEABLE_STATUSES.has(row.status)"
            :title="DELETEABLE_STATUSES.has(row.status) ? '' : '该工作流尚在处理,无法删除'"
            @click="askDeleteWorkflow(row)"
          >删除</button>
        </template>
      </DataTable>
      </div>
      <div v-if="createBatchError" class="error-banner">
        <span>新建工作流失败：{{ createBatchError }}</span>
        <button type="button" class="dismiss" aria-label="关闭" @click="createBatchError = null">×</button>
      </div>
      <div v-if="workflows.length === 0 && !createBatchError" class="empty">暂无工作流</div>
    </template>

    <!-- 章节Detail侧边面板 -->
    <div v-if="openSourceChapterId !== null" class="side-panel">
      <div class="panel-header">
        <h3>Chapter #{{ openSourceChapterId }} 的工作流结果</h3>
        <Button size="small" @click="closeChapterPanel">关闭</Button>
      </div>
      <section class="original-section">
        <h4>源原文</h4>
        <div v-if="sourceChapterLoading" class="hint">加载中...</div>
        <pre v-else-if="sourceChapterText" class="result-content">{{ sourceChapterText }}</pre>
        <div v-else class="hint">暂无原文</div>
      </section>
      <section class="results-section">
        <h4>本章节的转换结果</h4>
        <div v-if="openSourceResults.length === 0" class="empty">暂无结果</div>
        <ul v-else class="result-list">
          <li v-for="r in openSourceResults" :key="r.batch_id" class="result-item">
            <div class="result-meta">
              <span>工作流 #{{ r.batch_id }} · {{ r.batch_label ?? '—' }}</span>
              <span class="status" :class="r.batch_status">{{ r.status }} / {{ r.batch_status }}</span>
            </div>
            <pre class="result-content">{{ r.content ?? '(空)' }}</pre>
          </li>
        </ul>
      </section>
    </div>

    <!-- Workflow Detail 弹窗 -->
    <Dialog
      v-if="selectedWorkflow !== null"
      :open="true"
      :title="selectedWorkflow.label ? `工作流 #${selectedWorkflow.id} · ${selectedWorkflow.label}` : `工作流 #${selectedWorkflow.id}`"
      :width="960"
      @update:open="closeWorkflowPanel"
    >
      <div class="wf-status-strip">
        <div class="wf-status-left">
          <span class="status" :class="selectedWorkflow.status">
            <span v-if="selectedWorkflow.status === 'running'" class="dot dot-running" />
            {{ formatBatchStatus(selectedWorkflow.status) }}
          </span>
          <span class="wf-counts">
            共 <strong>{{ selectedWorkflow.total_count }}</strong> 章
            <span class="dot-sep">·</span>
            <span class="text-success">已完成 {{ selectedWorkflow.done_count }}</span>
            <span class="dot-sep">·</span>
            <span :class="{ 'has-failed': selectedWorkflow.failed_count > 0 }">失败 {{ selectedWorkflow.failed_count }}</span>
            <span class="dot-sep">·</span>
            <span>已跳过 {{ selectedWorkflow.skipped_count }}</span>
          </span>
          <button
            v-if="selectedWorkflow.status === 'stopped' && selectedWorkflow.promoted_count > 0"
            type="button"
            class="promoted-tag promoted-tag-link"
            :title="`已基于此工作流转正 ${selectedWorkflow.promoted_count} 份数据资产,点击查看`"
            @click="goDataAssetsList"
          >已转正 × {{ selectedWorkflow.promoted_count }}</button>
        </div>
        <div class="wf-status-right">
          <span class="wf-time">创建 {{ fmtTime(selectedWorkflow.created_at) }}</span>
          <span v-if="selectedWorkflow.ended_at" class="wf-time">结束 {{ fmtTime(selectedWorkflow.ended_at) }}</span>
          <span v-else class="wf-time muted">尚未结束</span>
        </div>
      </div>

      <div class="wf-actions">
        <div class="wf-actions-left">
          <Button
            v-if="selectedWorkflow.status === 'running'"
            kind="danger"
            size="small"
            @click="askStopWorkflow(selectedWorkflow.id)"
          >停止工作流</Button>
          <Button
            v-if="canRetrySelection"
            kind="primary"
            size="small"
            :disabled="retrySelectedIds.size === 0 || retrySubmitting || batchRetryBlockedReason !== null"
            :title="batchRetryBlockedReason ?? ''"
            :loading="retrySubmitting"
            @click="doRetry"
          >重试所选 ({{ retrySelectedIds.size }})</Button>
        </div>
        <!-- 转正 / 删除已统一到外层列表 actions 列,modal 内不再放。 -->
      </div>
      <DataTable
        v-if="selectedWorkflowChapters.length > 0"
        :columns="workflowChapterColumns"
        :data="selectedWorkflowChapters"
        :row-key="(row: WorkflowChapterRow) => row.tc_id"
        :widths="workflowChapterWidths"
        :max-height="'calc(90vh - 200px)'"
        :truncate-columns="['title', 'preview']"
        frozen-column="actions"
        empty-text="暂无章节"
      >
        <template #header-pick>
          <input
            type="checkbox"
            :checked="retrySelectedIds.size === retryableCount && retryableCount > 0"
            :indeterminate.prop="retrySelectedIds.size > 0 && retrySelectedIds.size < retryableCount"
            aria-label="全选可重试章节"
            :disabled="retryableCount === 0"
            @click.stop
            @change="onToggleAllRetry($event)"
          />
        </template>
        <template #cell-pick="{ row }">
          <!-- batch 状态允许重试时才显示 checkbox:整列勾选不可用时单选也没意义 -->
          <input
            v-if="canRetryChapter(row) && batchRetryBlockedReason === null"
            type="checkbox"
            :disabled="!row.is_empty_slot"
            :checked="retrySelectedIds.has(row.tc_id)"
            @change="(e) => toggleRetrySelection(row.tc_id, (e.target as HTMLInputElement).checked)"
          />
        </template>
        <template #cell-status="{ row }">
          <span v-if="row.status === 'running'" class="dot dot-running" />
          <span v-else-if="row.status === 'pending'" class="dot dot-pending" />
          <span v-else-if="row.status === 'failed' || row.status === 'skipped'" class="status-warn-mark" :title="row.error ?? ''">
            <IconAlertTriangle class="warn-icon" />
          </span>
          <span class="status" :class="row.status">{{ formatChapterStatus(row.status) }}</span>
        </template>
        <template #cell-actions="{ row }">
          <!-- 详情：始终可见（看 source/transformed） -->
          <button type="button" class="row-link" @click="openChapterDetail(row)">详情</button>
          <!-- done: 已转换但用户想再试 -->
          <template v-if="row.status === 'done'">
            <span class="row-sep" aria-hidden="true">·</span>
            <button type="button" class="row-link" @click="openRegenerateDialog(row)">重新生成</button>
          </template>
          <!-- failed: 失败信息单独弹框 + 重试 -->
          <template v-else-if="row.status === 'failed'">
            <span class="row-sep" aria-hidden="true">·</span>
            <button type="button" class="row-link" @click="openFailureDetail(row)">失败详情</button>
            <span class="row-sep" aria-hidden="true">·</span>
            <!-- 重试:is_empty_slot=false 或 batch 状态不允许时禁用 -->
            <button
              type="button"
              class="row-link"
              :disabled="!row.is_empty_slot || batchRetryBlockedReason !== null"
              :title="batchRetryBlockedReason ?? (!row.is_empty_slot ? '该章节结果槽非空,无法重试' : '')"
              @click="retrySingleChapter(row)"
            >重试</button>
          </template>
          <!-- skipped: 不展示"失败详情"(skip 是有意跳过 / 策略跳过 / 用户停止,非失败);
               仅重试入口,允许把空槽补回去。 -->
          <template v-else-if="row.status === 'skipped'">
            <span class="row-sep" aria-hidden="true">·</span>
            <button
              type="button"
              class="row-link"
              :disabled="!row.is_empty_slot || batchRetryBlockedReason !== null"
              :title="batchRetryBlockedReason ?? (!row.is_empty_slot ? '该章节结果槽非空,无法重试' : '')"
              @click="retrySingleChapter(row)"
            >重试</button>
          </template>
          <!-- pending/running/cancelled/terminated: 只剩详情 -->
        </template>
      </DataTable>
      <div v-else class="empty">暂无章节</div>
    </Dialog>

    <!-- Chapter Detail modal (within Workflow Detail) -->
    <Dialog v-if="detailChapter !== null" :open="true" title="章节详情" :width="1200" @update:open="closeChapterDetail">
      <!-- 失败/跳过的章节:错误信息去"失败详情"弹窗看(职责分开,避免两个地方重复展示) -->
      <div class="detail-grid">
        <section>
          <h4>
            原文
            <span v-if="!sourceChapterLoading && sourceChapterText" class="word-count">{{ formatWordCount(detailSourceWordCount) }}</span>
          </h4>
          <div v-if="sourceChapterLoading" class="hint">加载中...</div>
          <pre v-else-if="sourceChapterText" class="result-content">{{ sourceChapterText }}</pre>
          <div v-else class="hint">暂无原文</div>
        </section>
        <section>
          <h4>
            转换结果
            <span v-if="detailTransformedStatus" class="status" :class="detailTransformedStatus">{{ formatChapterStatus(detailTransformedStatus) }}</span>
            <span v-if="!detailLoading && detailTransformed" class="word-count">{{ formatWordCount(detailTransformedWordCount) }}</span>
          </h4>
          <div v-if="detailLoading" class="hint">加载中...</div>
          <pre v-else-if="detailTransformed" class="result-content">{{ detailTransformed }}</pre>
          <div v-else-if="detailChapter.status === 'failed' || detailChapter.status === 'skipped'" class="hint">未生成结果</div>
          <div v-else class="hint">尚未转换</div>
        </section>
      </div>
      <template #footer>
        <Button
          v-if="detailChapter !== null && canRetryChapter(detailChapter)"
          kind="primary"
          size="small"
          :disabled="!detailChapter.is_empty_slot || retrySubmitting || batchRetryBlockedReason !== null"
          :title="batchRetryBlockedReason ?? ''"
          :loading="retrySubmitting"
          @click="retryFromDetail"
        >重试</Button>
        <Button size="small" @click="closeChapterDetail">关闭</Button>
      </template>
    </Dialog>

    <!-- 失败详情弹窗 —— 与"详情"分开语义:只看错误 + 重试入口 -->
    <Dialog
      v-if="failureDetailChapter !== null"
      :open="true"
      :title="`失败详情 · 第${failureDetailChapter.chapter_idx} 章 · ${failureDetailChapter.chapter_title}`"
      :width="600"
      @update:open="closeFailureDetail"
    >
      <div class="failure-detail">
        <div class="failure-meta">
          <span class="status" :class="failureDetailChapter.status">{{ formatChapterStatus(failureDetailChapter.status) }}</span>
          <span class="failure-meta-text">工作流 #{{ selectedWorkflowId }} · 章节 ID {{ failureDetailChapter.chapter_id }}</span>
        </div>
        <h4 class="failure-h">错误信息</h4>
        <pre v-if="failureDetailChapter.error" class="failure-body">{{ failureDetailChapter.error }}</pre>
        <div v-else class="failure-empty">未提供错误信息</div>
      </div>
      <template #footer>
        <Button
          kind="primary"
          size="small"
          :disabled="!failureDetailChapter.is_empty_slot || retrySubmitting || batchRetryBlockedReason !== null"
          :title="batchRetryBlockedReason ?? ''"
          :loading="retrySubmitting"
          @click="retryFromFailureDetail"
        >重试</Button>
        <Button size="small" @click="closeFailureDetail">关闭</Button>
      </template>
    </Dialog>

    <!-- 工作流删除确认弹窗(自带 deleteError 展示) -->
    <ConfirmDialog
      v-model:open="deleteConfirmOpen"
      title="删除工作流"
      :message="deleteConfirmMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="confirmDeleteWorkflow"
    />
    <div v-if="deleteSubmitting" class="hint center">删除中...</div>
<PromoteWorkflowDialog
      v-if="promoteTarget !== null"
      v-model:open="promoteOpen"
      :workflow-label="promoteTarget.label ?? `工作流 #${promoteTarget.id}`"
      :source-data-asset-title="tnTitle"
      :success-count="promoteTarget.done_count"
      :fail-count="promoteTarget.failed_count"
      :skip-count="promoteTarget.skipped_count"
      :external-error="promoteError"
      @confirm="confirmPromote"
    />

    <CreateBatchDialog
      v-model:open="createBatchOpen"
      :tn-id="tnId"
      :default-prompt-id="createBatchDefaults.default_prompt_id"
      :default-model-config-id="createBatchDefaults.default_model_config_id"
      :default-mode="createBatchDefaults.default_mode"
      :selected-chapter-ids="Array.from(selectedChapterIds)"
      :preview-chapter-id="previewChapterId"
      @submit="onCreateBatch"
    />

    <ConfirmDialog
      v-model:open="stopConfirmOpen"
      title="Stop Workflow"
      :message="'停止后当前章节会完成,后续章节标记为已跳过。确定停止吗?'"
      kind="danger"
      confirm-text="停止"
      @confirm="confirmStopWorkflow"
    />

    <!-- 通用提示弹窗：后端报错或前置校验失败时统一弹出 -->
    <Dialog
      v-model:open="alertOpen"
      :title="alertTitle"
      :width="420"
    >
      <p class="message">{{ alertMessage }}</p>
      <template #footer>
        <Button @click="alertOpen = false">确定</Button>
      </template>
    </Dialog>

    <RegeneratePreviewDialog
      v-if="regenChapter !== null"
      v-model:open="regenOpenProxy"
      :batch-id="selectedWorkflowId ?? 0"
      :chapter-id="regenChapter.chapter_id"
      :chapter-idx="regenChapter.chapter_idx"
      :chapter-title="regenChapter.chapter_title"
      :tn-id="tnId"
      @committed="onPreviewCommitted"
    />

    <!-- 补充章节对话框:仅 stopped batch 可触发;状态/上下文展示由父组件传入
         (appendTarget.prompt_name 等),Dialog 内部只做章节挑选 + emit confirm。 -->
    <AppendChaptersDialog
      v-if="appendOpen && appendTarget !== null"
      v-model:open="appendOpen"
      :batch-id="appendTarget.id"
      :transformation-novel-id="tnId"
      :prompt-name="appendTarget.prompt_name"
      :model-display-name="appendTarget.model_display_name"
      :mode="appendTarget.mode"
      :ctx-prev-original="appendTarget.ctx_prev_original"
      :ctx-prev-transformed="appendTarget.ctx_prev_transformed"
      :ctx-next-original="appendTarget.ctx_next_original"
      :workflow-label="appendTarget.label"
      @confirm="onAppendConfirm"
    />
  </section>
</template>

<style scoped>
.meta-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  margin-bottom: 8px;
}
.tags {
  display: flex;
  align-items: center;
  gap: 6px;
}
.badge {
  padding: 2px 8px;
  background: var(--bg-hover);
  border-radius: var(--radius-pin);
  font-size: 11px;
  color: var(--text-secondary);
}
.meta-text {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}
.meta-text .src {
  color: var(--text-secondary);
}
.alert {
  margin-top: 12px;
  padding: 8px 12px;
  background: var(--color-paper-mist);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
.tabs { display: flex; gap: 8px; margin-bottom: 16px; border-bottom: 1px solid var(--border-color); }
.tabs button {
  padding: 8px 16px;
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  font-family: inherit;
  font-size: 14px;
  color: var(--text-secondary);
}
.tabs button.active {
  color: var(--color-primary);
  border-bottom-color: var(--color-primary);
}
.actions {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
  align-items: center;
}
.range-pick {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
}
.range-label {
  white-space: nowrap;
}
.range-sep {
  color: var(--text-muted);
}
.range-input {
  width: 64px;
  padding: 4px 8px;
  font: inherit;
  font-size: 13px;
  text-align: center;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  color: var(--text-primary);
  outline: none;
}
.range-input:focus {
  border-color: var(--border-strong);
}
.range-input:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.range-input.has-error {
  border-color: var(--danger, #d64545);
  background: var(--danger-bg, rgba(214, 69, 69, 0.06));
}
.range-mode {
  height: 28px;
  padding: 0 6px;
  font: inherit;
  font-size: 13px;
  background: var(--color-sheet);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  color: var(--text-primary);
  outline: none;
  cursor: pointer;
}
.range-mode:focus {
  border-color: var(--border-strong);
}
.range-mode:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.link-btn {
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  color: var(--color-primary);
  font: inherit;
  text-align: left;
}
.link-btn:hover { text-decoration: underline; }
.empty {
  text-align: center;
  padding: 48px 0;
  color: var(--text-muted);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
}
.cell-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}
.status {
  display: inline-block;
  padding: 1px 8px;
  border-radius: var(--radius-pin);
  font-size: 11px;
  border: 1px solid var(--border-soft);
}
.status.running { background: var(--bg-section); color: var(--text-secondary); }
.status.stopped { background: var(--warn-bg); color: var(--warn); border-color: var(--warn-border); }
.chapter-row.running { background: rgba(196, 92, 60, 0.06); }
.detail-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; min-height: 400px; }
.detail-grid section { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.detail-grid h4 { margin: 0; font-size: 14px; color: var(--text-secondary); display: flex; align-items: center; gap: 8px; }
.detail-grid h4 .word-count {
  font-size: 12px;
  color: var(--text-muted);
  font-weight: var(--font-weight-regular);
  font-family: var(--font-mono);
  margin-left: auto;
}
.detail-grid .result-content { background: var(--bg-section); border: 1px solid var(--border-soft); border-radius: var(--radius-pin); padding: 12px; white-space: pre-wrap; word-break: break-word; max-height: 60vh; overflow: auto; font-family: var(--font-mono); font-size: 13px; line-height: 1.6; }
.row-actions { text-align: right; }
.dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 6px;
  border-radius: 50%;
}
.dot-running { background: var(--color-cinnabar); animation: pulse 1.2s ease-in-out infinite; }
.dot-pending { background: var(--text-muted); opacity: 0.55; }
/* 失败/跳过行的状态列前 ⚠️ 标识 —— 视觉上快速定位,完整错误去 Chapter Detail 看。 */
.status-warn-mark { display: inline-flex; align-items: center; color: var(--danger); margin-right: 4px; }
.warn-icon { width: 14px; height: 14px; flex-shrink: 0; }
/* 失败详情弹窗 —— 紧凑布局,主体错误信息。 */
.failure-detail { display: flex; flex-direction: column; gap: 12px; }
.failure-meta { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.failure-meta-text { color: var(--text-muted); font-size: 12px; font-family: var(--font-mono); }
.failure-h { margin: 0; font-size: 13px; color: var(--text-secondary); font-weight: 600; }
.failure-body {
  margin: 0;
  padding: 12px 14px;
  background: var(--bg-section);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 360px;
  overflow: auto;
  color: var(--text-primary);
}
.failure-empty { color: var(--text-muted); font-size: 12px; padding: 8px 0; }

.chapter-row.running .status.running { background: var(--color-cinnabar); color: #faf6ee; border-color: var(--color-cinnabar); }
.spinner-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 6px;
  border-radius: 50%;
  background: var(--color-cinnabar);
  animation: pulse 1.2s ease-in-out infinite;
  vertical-align: middle;
}
@keyframes pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }

.error-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
  padding: 10px 12px;
  background: var(--danger-bg, rgba(214, 69, 69, 0.12));
  border: 1px solid var(--danger, #d64545);
  border-radius: var(--radius-pin, 6px);
  color: var(--danger, #d64545);
  font-size: 13px;
}
.error-banner .dismiss {
  background: transparent;
  border: none;
  color: inherit;
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
  padding: 0 4px;
}

.side-panel {
  position: fixed;
  top: 0;
  right: 0;
  width: 420px;
  height: 100vh;
  background: var(--color-sheet);
  border-left: 1px solid var(--border-color);
  padding: 16px;
  overflow: auto;
  box-shadow: -2px 0 12px rgba(0, 0, 0, 0.08);
  z-index: 10;
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.panel-header h3 { margin: 0; font-size: 16px; }
.wf-status-strip {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding-bottom: 16px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--border-soft);
  gap: 16px;
}
.wf-status-left {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  flex: 1;
  min-width: 0;
}
.wf-status-right {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  font-size: 12px;
  flex-shrink: 0;
}
.wf-time { color: var(--text-secondary); }
.wf-time.muted { color: var(--text-muted); font-style: italic; }
.wf-counts {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
}
.wf-counts strong {
  font-family: var(--font-mono);
  font-weight: var(--font-weight-medium);
  color: var(--text-primary);
}
.wf-counts .text-success { color: #2e7d32; }
.wf-counts .has-failed { color: var(--danger, #d64545); }
.wf-counts .dot-sep { color: var(--text-muted); opacity: 0.55; }
.wf-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.wf-actions-left, .wf-actions-right {
  display: flex;
  gap: 8px;
  align-items: center;
}
.preview {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary);
  font-size: 13px;
}
.error {
  color: var(--danger);
  font-size: 12px;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.result-list { list-style: none; padding: 0; margin: 0; }
.result-item {
  padding: 10px 0;
  border-bottom: 1px solid var(--border-soft);
}
.result-meta {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 6px;
}
.result-content {
  background: var(--bg-section);
  padding: 8px;
  border-radius: var(--radius-pin);
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 13px;
  font-family: var(--font-mono);
  margin: 0;
  max-height: 240px;
  overflow: auto;
}
.original-section, .results-section { margin-bottom: 16px; }
.original-section h4, .results-section h4 {
  margin: 0 0 8px;
  font-size: 13px;
  color: var(--text-muted);
  font-weight: var(--font-weight-regular);
}
.hint {
  font-size: 12px;
  color: var(--text-muted);
  padding: 8px 0;
}
.promoted-tag {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 10px;
  background: #e8f5e9;
  color: #2e7d32;
  font-size: 12px;
  font-weight: 500;
  margin-right: 6px;
  cursor: default;
  white-space: nowrap;
}
.promoted-tag-link {
  border: 1px solid #c8e6c9;
  cursor: pointer;
  font: inherit;
  font-size: 12px;
  font-weight: 500;
}
.promoted-tag-link:hover {
  background: #c8e6c9;
  color: #1b5e20;
}

/* Toast —— 仿 Library.vue:545 */
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
}
.toast-action:hover { color: var(--color-cinnabar); }
.toast-close {
  background: transparent;
  border: 0;
  font-size: 18px;
  line-height: 1;
  color: var(--text-muted);
  cursor: pointer;
  padding: 0 4px;
}
.toast-close:hover { color: var(--text-primary); }
.toast-enter-active, .toast-leave-active { transition: opacity 0.2s, transform 0.2s; }
.toast-enter-from, .toast-leave-to { opacity: 0; transform: translateY(-4px); }
</style>