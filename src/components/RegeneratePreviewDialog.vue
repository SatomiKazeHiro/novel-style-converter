<template>
  <Dialog v-model:open="open" :title="dialogTitle" size="full">
    <div class="rpd">
      <!-- 左侧：原文 -->
      <section class="col col-original">
        <header class="col-header">原文</header>
        <div class="tabs">
          <button v-if="prevChapter" class="tab" :class="{ active: origTab === 'prev' }" @click="origTab = 'prev'">上一章</button>
          <button class="tab" :class="{ active: origTab === 'cur' }" @click="origTab = 'cur'">当前章</button>
          <button v-if="nextChapter" class="tab" :class="{ active: origTab === 'next' }" @click="origTab = 'next'">下一章</button>
        </div>
        <div class="content">
          <pre v-if="originalBody">{{ originalBody }}</pre>
          <div v-else class="empty">（无内容）</div>
        </div>
      </section>

      <!-- 中间：附加指令 + 草稿 -->
      <section class="col col-middle">
        <header class="col-header">生成 / 草稿</header>
        <div class="extra">
          <label>附加指令（≤2000 字，可选）</label>
          <textarea
            v-model="extraInput"
            :maxlength="2000"
            :disabled="generating"
            placeholder="例：再短一点 / 换个语气 / 强调某个细节..."
          />
          <div class="counter">{{ extraInput.length }} / 2000</div>
        </div>
        <div class="draft">
          <label>草稿（提交后写入该章节的转换结果）</label>
          <textarea
            v-model="draftContent"
            :disabled="committing"
            placeholder="点击 [使用此预览填充草稿] 拷入，或手动编辑..."
          />
        </div>
        <div class="actions">
          <Button :loading="generating" :disabled="generating" @click="onGenerate">生成（读附加指令）</Button>
          <Button
            kind="primary"
            :disabled="!canCommit"
            :loading="committing"
            :title="canCommit ? '' : '请先填充或编辑草稿'"
            @click="onCommit"
          >确认替换（读草稿）</Button>
        </div>
        <div v-if="lastError" class="error">{{ lastError }}</div>
      </section>

      <!-- 右侧：预览 -->
      <section class="col col-preview">
        <header class="col-header">预览（{{ previews.length }}）</header>
        <div v-if="previews.length === 0" class="empty empty-state">尚未生成预览</div>
        <template v-else>
          <div class="tabs">
            <button
              v-for="(p, i) in previews"
              :key="p.id"
              class="tab"
              :class="{ active: selectedPreviewId === p.id, [p.status]: true }"
              :title="previewTabTitle(p, i)"
              @click="selectedPreviewId = p.id"
            >
              <span>预览 {{ previews.length - i }}</span>
              <span class="status">{{ statusGlyph(p.status) }}</span>
            </button>
          </div>
          <div class="content">
            <div v-if="!currentPreview" class="empty">（无内容）</div>
            <div v-else-if="currentPreview.status === 'generating'" class="generating">生成中…</div>
            <pre v-else-if="currentPreview.status === 'failed'">{{ currentPreview.error ?? '生成失败' }}</pre>
            <pre v-else-if="currentPreview.preview_content">{{ currentPreview.preview_content }}</pre>
            <div v-else class="empty">（无内容）</div>
          </div>
          <div class="actions">
            <Button :disabled="!canUsePreview" @click="onUsePreview">使用此预览填充草稿</Button>
            <Button v-if="currentPreview" :disabled="discarding" @click="onDiscard(currentPreview.id)">放弃</Button>
          </div>
        </template>
      </section>
    </div>
  </Dialog>

    <ConfirmDialog
      v-model:open="commitConfirmOpen"
      title="确认替换"
      message="确认将草稿内容写入该章节的转换结果？此操作不可撤销。"
      confirm-text="确认替换"
      cancel-text="取消"
      @confirm="doCommit"
    />
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import ConfirmDialog from './ui/ConfirmDialog.vue';
import { getChapter as ipcGetChapter, listChapterPreviews, listTransformationSourceChapters } from '../ipc/commands';
import { useWorkflowsStore } from '../stores/workflows';
import type { ChapterPreviewRow, PreviewStatus, SourceChapterRow } from '../ipc/types';

type OrigTab = 'prev' | 'cur' | 'next';

const open = defineModel<boolean>('open', { required: true });

const props = defineProps<{
  batchId: number;
  chapterId: number;
  chapterIdx: number;
  chapterTitle: string;
  tnId: number;
}>();

const emit = defineEmits<{
  committed: [];
}>();

const store = useWorkflowsStore();

const extraInput = ref('');
const draftContent = ref('');
const origTab = ref<OrigTab>('cur');
const selectedPreviewId = ref<number | null>(null);
const generating = ref(false);
const committing = ref(false);
const commitConfirmOpen = ref(false);
const discarding = ref(false);
const lastError = ref<string | null>(null);
const originalBody = ref('');

const dialogTitle = computed(() => `重新生成章节 #${props.chapterIdx} - ${props.chapterTitle}`);

/// preview 列表 —— vue-query 自动按 [batchId, chapterId] 缓存。
/// regeneratePreview/commitPreview/discardPreview 在 store 内自动 invalidate 该 key。
/// 1.5s 轮询只在有 preview 处于 generating 时触发;终态自动停。
/// 替代原 onGenerate 内手写 setTimeout 循环。
const previewsQuery = useQuery({
  queryKey: ['chapterPreviews', props.batchId, props.chapterId],
  queryFn: () => listChapterPreviews(props.batchId, props.chapterId),
  refetchInterval: (query) => {
    const data = query.state.data as ChapterPreviewRow[] | undefined;
    if (data === undefined) return false;
    return data.some((p) => p.status === 'generating') ? 1500 : false;
  },
});
const previews = computed<ChapterPreviewRow[]>(() => previewsQuery.data.value ?? []);

const currentPreview = computed<ChapterPreviewRow | null>(
  () => previews.value.find(p => p.id === selectedPreviewId.value) ?? previews.value[0] ?? null,
);

/// 章节上下文(上一章/下一章) —— 与父页同一 cache key,自动共享 + 实时同步。
const sourcesQuery = useQuery({
  queryKey: ['transformationSourceChapters', props.tnId],
  queryFn: () => listTransformationSourceChapters(props.tnId),
});
const sources = computed<SourceChapterRow[]>(() => sourcesQuery.data.value ?? []);

const currentSource = computed(() =>
  sources.value.find(s => s.chapter_id === props.chapterId) ?? null,
);

const prevChapter = computed(() => {
  const cur = currentSource.value;
  if (!cur) return null;
  let prev: SourceChapterRow | null = null;
  for (const s of sources.value) {
    if (s.idx < cur.idx && (!prev || s.idx > prev.idx)) prev = s;
  }
  return prev;
});

const nextChapter = computed(() => {
  const cur = currentSource.value;
  if (!cur) return null;
  return sources.value.find(s => s.idx > cur.idx) ?? null;
});

const canCommit = computed(() => draftContent.value.trim().length > 0 && !committing.value);
const canUsePreview = computed(() => {
  const p = currentPreview.value;
  return !!p && p.status === 'done' && !!p.preview_content;
});

async function loadOriginalBody(chapterId: number): Promise<void> {
  try {
    const ch = await ipcGetChapter(chapterId);
    originalBody.value = ch?.body ?? '';
  } catch (e: unknown) {
    originalBody.value = '';
    lastError.value = e instanceof Error ? e.message : String(e);
  }
}

watch(open, async (v) => {
  if (!v) return;
  lastError.value = null;
  draftContent.value = '';
  extraInput.value = '';
  origTab.value = 'cur';
  // previewsQuery 自动订阅(打开对话框时 enabled),无需手动 loadPreviews。
  try {
    selectedPreviewId.value = previews.value[0]?.id ?? null;
    await loadOriginalBody(props.chapterId);
  } catch (e: unknown) {
    lastError.value = e instanceof Error ? e.message : String(e);
  }
}, { immediate: true });

watch(origTab, async (tab) => {
  if (!open.value) return;
  const id =
    tab === 'prev' ? prevChapter.value?.chapter_id :
    tab === 'next' ? nextChapter.value?.chapter_id :
    props.chapterId;
  if (id) await loadOriginalBody(id);
});

async function onGenerate(): Promise<void> {
  if (generating.value) return;
  generating.value = true;
  lastError.value = null;
  try {
    await store.regeneratePreview(
      props.batchId,
      props.chapterId,
      extraInput.value.trim() || null,
    );
    selectedPreviewId.value = previews.value[0]?.id ?? null;
    // previewsQuery.refetchInterval 在有 generating 时自动 1.5s 轮询,无需手写 setTimeout 循环
  } catch (e: unknown) {
    lastError.value = e instanceof Error ? e.message : String(e);
  } finally {
    generating.value = false;
  }
}

function onUsePreview(): void {
  const content = currentPreview.value?.preview_content;
  if (!content) return;
  if (!draftContent.value.trim()) {
    draftContent.value = content;
    return;
  }
  const append = window.confirm(
    '草稿区已有内容。\n点击"确定"=追加到末尾（保留现有内容）\n点击"取消"=替换当前内容',
  );
  if (append) draftContent.value = draftContent.value + '\n\n' + content;
  else draftContent.value = content;
}

function onCommit(): void {
  if (!canCommit.value) return;
  commitConfirmOpen.value = true;
}

async function doCommit(): Promise<void> {
  committing.value = true;
  lastError.value = null;
  try {
    await store.commitPreview({
      batch_id: props.batchId,
      chapter_id: props.chapterId,
      draft_content: draftContent.value,
      source_preview_id: selectedPreviewId.value,
    });
    emit('committed');
    open.value = false;
  } catch (e: unknown) {
    lastError.value = e instanceof Error ? e.message : String(e);
  } finally {
    committing.value = false;
  }
}

async function onDiscard(previewId: number): Promise<void> {
  if (discarding.value) return;
  if (!window.confirm('放弃这个预览？')) return;
  discarding.value = true;
  try {
    await store.discardPreview(previewId);
    if (selectedPreviewId.value === previewId) {
      selectedPreviewId.value = previews.value[0]?.id ?? null;
    }
  } catch (e: unknown) {
    lastError.value = e instanceof Error ? e.message : String(e);
  } finally {
    discarding.value = false;
  }
}

function statusGlyph(s: PreviewStatus): string {
  switch (s) {
    case 'generating': return '⋯';
    case 'done': return '✓';
    case 'failed': return '✗';
  }
}

function previewTabTitle(p: ChapterPreviewRow, i: number): string {
  const idx = previews.value.length - i;
  return `预览 ${idx} · ${p.status}${p.updated_at ? ' · ' + p.updated_at : ''}`;
}
</script>

<style scoped>
.rpd {
  display: grid;
  grid-template-columns: 1fr 1.1fr 1fr;
  gap: 16px;
  height: calc(100vh - 160px);
  min-height: 520px;
}
.col {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-card);
  padding: 12px;
  background: var(--color-sheet);
  overflow: hidden;
  min-width: 0;
}
.col-header {
  font-family: var(--font-serif);
  font-size: 14px;
  font-weight: var(--font-weight-medium);
  color: var(--text-secondary);
  margin-bottom: 8px;
}
.tabs {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
  margin-bottom: 8px;
}
.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-secondary);
  font-size: 13px;
  font-family: inherit;
  cursor: pointer;
  position: relative;
  transition: background 0.1s, color 0.1s, border-color 0.1s;
}
.tab:hover { background: var(--bg-hover); color: var(--text-primary); }
.tab.active {
  background: var(--color-cinnabar-light);
  color: var(--color-cinnabar-deep);
  font-weight: var(--font-weight-medium);
  border-color: var(--color-cinnabar);
}
.tab.active::before {
  content: '';
  position: absolute;
  left: -1px;
  top: 6px;
  bottom: 6px;
  width: 2px;
  background: var(--color-cinnabar);
}
.tab.failed { color: var(--danger); border-color: var(--danger-border); }
.tab .status { font-size: 12px; font-variant-numeric: tabular-nums; }
.content {
  flex: 1;
  overflow: auto;
  background: var(--color-paper);
  border-radius: var(--radius-pin);
  padding: 12px;
}
.content pre {
  margin: 0;
  white-space: pre-wrap;
  font-family: var(--font-serif);
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-primary);
}
.extra,
.draft {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 12px;
}
.extra label,
.draft label {
  font-size: 12px;
  color: var(--text-secondary);
}
.extra textarea,
.draft textarea {
  width: 100%;
  min-height: 80px;
  padding: 8px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--color-paper);
  color: var(--text-primary);
  font-family: var(--font-serif);
  font-size: 14px;
  resize: vertical;
  box-sizing: border-box;
}
.draft textarea { min-height: 260px; }
.extra textarea:disabled,
.draft textarea:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.counter {
  font-size: 11px;
  color: var(--text-muted);
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 8px;
  flex-wrap: wrap;
}
.empty {
  color: var(--text-muted);
  padding: 24px;
  text-align: center;
  font-size: 13px;
}
.empty-state {
  background: var(--color-paper);
  border-radius: var(--radius-pin);
  margin-bottom: 12px;
}
.generating {
  color: var(--text-secondary);
  font-style: italic;
  padding: 12px;
  text-align: center;
}
.error {
  color: var(--danger);
  padding: 8px 10px;
  background: var(--danger-bg);
  border: 1px solid var(--danger-border);
  border-radius: var(--radius-pin);
  margin-top: 8px;
  font-size: 13px;
  word-break: break-word;
}
</style>