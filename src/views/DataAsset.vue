<template>
  <section class="data-asset">
    <PageHeader :title="store.title || '加载中...'" size="small">
      <template #back>
        <Button aria-label="返回" @click="onBack">
          <IconArrowLeft :size="16" :stroke-width="1.5" />
        </Button>
      </template>
      <template #actions>
        <Button
          kind="danger"
          :disabled="store.tnCount > 0"
          :title="store.tnCount > 0 ? `有 ${store.tnCount} 个工程引用,请先删除工程` : ''"
          @click="onDelete"
        >删除资产</Button>
      </template>
    </PageHeader>

    <div v-if="store.error" class="alert">{{ store.error }}</div>

    <div class="meta-strip">
      <div class="tags">
        <span v-if="store.kind === 'promoted'" class="badge derived">派生资产</span>
        <span v-else class="badge">源资产</span>
        <span v-if="store.tnCount > 0" class="badge locked">有 {{ store.tnCount }} 个工程</span>
        <span v-else-if="store.kind === 'source'" class="badge">已解析</span>
      </div>
      <div class="meta-text">
        <span v-if="store.parsedAt">{{ formatTime(store.parsedAt) }}</span>
        <span v-if="store.sourceWorkflowId !== null" class="src">来自工作流 #{{ store.sourceWorkflowId }}</span>
        <span v-else-if="store.kind === 'promoted' && store.uploadId !== null" class="src" title="原派生自工作流,工作流已删除,数据资产本身保留">来自上传文件 #{{ store.uploadId }}</span>
      </div>
    </div>

    <ConfirmDialog
      v-model:open="confirmOpen"
      title="删除数据资产"
      :message="confirmMessage"
      kind="danger"
      confirm-text="删除"
      @confirm="doDelete"
    />

    <AlertDialog
      v-model:open="alertOpen"
      title="提示"
      :message="alertMessage"
    />

    <ConfirmDialog
      v-model:open="dirtyGuardOpen"
      title="未保存的修改"
      message="当前章节有未保存的修改,切换会丢弃。继续?"
      confirm-text="丢弃修改"
      kind="danger"
      @confirm="onConfirmDiscard"
      @cancel="onCancelDiscard"
    />

    <div class="panes">
      <div class="pane">
        <div class="pane-header">
          <div class="pane-title">章节 ({{ store.chapters.length }})</div>
        </div>
        <RecycleScroller
          v-if="store.chapters.length > 0"
          class="scroller"
          :items="chaptersWithIdx"
          :item-size="40"
          :key-field="'idx'"
        >
          <template #default="{ item, index }">
            <div
              class="chap-row"
              :class="{ active: store.selectedIdx === index }"
              @click="onChapterClick(index)"
            >
              <span class="idx">{{ index + 1 }}</span>
              <span class="title">{{ item.title }}</span>
              <span v-if="store.sourceKinds[index] === 'transformed'" class="kind-tag transformed" title="来自工作流转换结果">转换</span>
              <span v-else class="kind-tag original" title="原文(派生 da 失败章节)">原文</span>
              <span v-if="store.editedAts[index]" class="kind-tag edited" title="用户编辑过">已编辑</span>
              <span class="size">{{ item.word_count }} 字</span>
            </div>
          </template>
        </RecycleScroller>
        <div v-else class="empty">暂无章节</div>
      </div>
      <div class="pane">
        <div class="pane-header">
          <div class="pane-title">原文</div>
          <span v-if="!store.editing && selectedEditedAt" class="edited-meta">
            上次编辑 {{ formatTime(selectedEditedAt) }}
          </span>
          <div class="pane-actions">
            <template v-if="store.editing">
              <span class="editing-tag">编辑中</span>
              <span class="editing-draft-meta">{{ store.draftContent.length }} 字</span>
              <Button size="small" :disabled="store.saving" @click="onCancelEdit">取消</Button>
              <Button
                size="small"
                kind="primary"
                :disabled="!store.editingDirty || store.saving"
                :loading="store.saving"
                @click="onSave"
              >保存</Button>
            </template>
            <Button
              v-else
              size="small"
              :disabled="!canEdit"
              :title="editButtonTitle"
              @click="onEnterEdit"
            >编辑</Button>
          </div>
        </div>
        <textarea
          v-if="store.editing"
          :value="store.draftContent"
          class="content content-edit"
          spellcheck="false"
          @input="onDraftInput($event)"
        />
        <pre v-else class="content">{{ store.selectedContent }}</pre>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
/// 给章节加 idx 当唯一 key; ChapterSegment 没有 id, content/title 可能重复
import { useRoute, useRouter } from 'vue-router';
import { RecycleScroller } from 'vue-virtual-scroller';
import Button from '../components/ui/Button.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import IconArrowLeft from '~icons/lucide/arrow-left';
import { useDataAssetStore } from '../stores/dataAsset';
import { useLibraryStore } from '../stores/library';
import { formatTime } from '../utils/format';

const route = useRoute();
const router = useRouter();
const store = useDataAssetStore();
const library = useLibraryStore();

const confirmOpen = ref(false);
const confirmMessage = computed(() => `确认删除数据资产 "${store.title}"？解析出的章节将一并删除，删除后可重新解析。`);
const alertOpen = ref(false);
const alertMessage = ref('');

/// dirty 守卫:章节编辑后切换章节/返回前的拦截
const dirtyGuardOpen = ref(false);
const pendingSelectIdx = ref<number | null>(null);
let pendingNavigation: (() => void) | null = null;
/// 返回按钮触发的导航,在 beforeRouteLeave 里拦;路由组件卸载前如果 dirty 则走弹窗。

const chaptersWithIdx = computed(() =>
  store.chapters.map((s, idx) => ({ ...s, idx })),
);

/// 编辑按钮 title:派生资产 / 无章节两种状态
/// 编辑按钮可点状态:仅看是否已选章节(任意 kind 都能编辑——数据资产是独立数据)
const canEdit = computed(() => store.selectedIdx !== null);

/// 编辑按钮 title:不可编辑原因(只剩两种)
const editButtonTitle = computed(() => {
  if (store.chapters.length === 0) return '暂无章节';
  if (store.selectedIdx === null) return '请先选中章节';
  return '';
});

/// 当前选中章节的 edited_at:给右侧面板头展示用
const selectedEditedAt = computed(() => {
  const i = store.selectedIdx;
  if (i == null) return null;
  return store.editedAts[i] ?? null;
});

onMounted(async () => {
  const raw = route.params.dataAssetId;
  const id = Number(raw);
  if (!Number.isFinite(id) || id <= 0) {
    store.error = `无效的 data_asset ID: ${String(raw)}`;
    return;
  }
  await store.load(id);
  store.selectFirstIfNone();
});

/// 离开页面前的全局守卫:有 dirty 编辑 → 弹 dirtyGuard,用户确认丢弃才放行
function tryLeave(next: () => void): boolean {
  if (!store.editing || !store.editingDirty) return true;
  pendingNavigation = next;
  dirtyGuardOpen.value = true;
  return false;
}

function onBack() {
  if (!tryLeave(() => void router.push('/data-assets'))) return;
  void router.push('/data-assets');
}

async function onDelete() {
  if (store.tnCount > 0) return;
  confirmOpen.value = true;
}

async function doDelete() {
  const id = store.dataAssetId;
  if (id == null) return;
  if (!tryLeave(() => void doDeleteActual(id))) return;
  await doDeleteActual(id);
}

async function doDeleteActual(id: number) {
  try {
    await library.removeDataAsset(id);
    void router.push('/data-assets');
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  }
}

function onEnterEdit() { store.enterEdit(); }
function onCancelEdit() { store.cancelEdit(); }
async function onSave() { await store.saveEdit(); }
function onDraftInput(e: Event) {
  const t = (e.target as HTMLTextAreaElement).value;
  store.onDraftInput(t);
}

function onChapterClick(idx: number) {
  if (store.editing && store.editingDirty) {
    pendingSelectIdx.value = idx;
    dirtyGuardOpen.value = true;
    return;
  }
  if (store.editing) store.cancelEdit();
  store.selectChapter(idx);
}

function onConfirmDiscard() {
  dirtyGuardOpen.value = false;
  if (pendingSelectIdx.value !== null) {
    store.cancelEdit();
    store.selectChapter(pendingSelectIdx.value);
    pendingSelectIdx.value = null;
    return;
  }
  if (pendingNavigation) {
    const nav = pendingNavigation;
    pendingNavigation = null;
    store.cancelEdit();
    nav();
  }
}

function onCancelDiscard() {
  dirtyGuardOpen.value = false;
  pendingSelectIdx.value = null;
  pendingNavigation = null;
}
</script>

<style scoped>
.data-asset {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.badge {
  padding: 2px 8px;
  background: var(--bg-hover);
  border-radius: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}
.badge.locked {
  background: var(--color-paper-mist);
  color: var(--color-cinnabar-deep);
}
.src {
  font-size: 12px;
  color: var(--text-secondary);
}
.meta-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
}
.tags {
  display: flex;
  align-items: center;
  gap: 6px;
}
.meta-text {
  display: flex;
  align-items: center;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.meta-text span + span::before {
  content: '·';
  padding: 0 4px;
}
.alert {
  margin-top: 12px;
  padding: 8px 12px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
.panes {
  display: flex;
  gap: 16px;
  flex: 1;
  min-height: 0;
  margin-top: 12px;
}
.pane:first-child {
  flex: 0 0 280px;
}
.pane {
  flex: 1;
  width: 50%;
  display: flex;
  flex-direction: column;
  background: var(--color-sheet);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  overflow: hidden;
}
.pane-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 4px 4px 4px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.pane-title {
  padding: 4px 0;
  font-size: 13px;
  color: var(--text-secondary);
}
.pane-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-right: 8px;
}
.editing-tag {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  margin-right: 4px;
  background: var(--color-cinnabar);
  color: #faf6ee;
  border-radius: 4px;
  font-size: 11px;
  font-weight: var(--font-weight-medium);
  letter-spacing: 0.02em;
}
.editing-draft-meta {
  color: var(--text-secondary);
  font-size: 12px;
  margin-right: 4px;
  font-variant-numeric: tabular-nums;
}
.scroller {
  flex: 1;
  overflow-y: auto;
}
.chap-row {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 40px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border-color);
  cursor: pointer;
}
.chap-row:hover { background: var(--bg-hover); }
.chap-row.active {
  background: var(--color-paper-mist);
}
.idx {
  color: var(--text-secondary);
  font-size: 12px;
  min-width: 32px;
  flex-shrink: 0;
}
.title {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.size {
  color: var(--text-secondary);
  font-size: 12px;
  flex-shrink: 0;
}
.content {
  flex: 1;
  margin: 0;
  padding: 12px;
  font-family: ui-monospace, monospace;
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-y: auto;
  color: var(--text-primary);
  background: var(--color-sheet);
}
.content-edit {
  border: 1px solid var(--color-cinnabar);
  outline: none;
  resize: none;
  border-radius: 4px;
}
.empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary);
  font-size: 13px;
}
.badge.derived {
  background: #e8f5e9;
  color: #2e7d32;
  border-color: #c8e6c9;
}
.kind-tag {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 11px;
  font-weight: 500;
  margin: 0 6px;
  flex-shrink: 0;
}
.kind-tag.transformed {
  background: #e8f5e9;
  color: #2e7d32;
}
.kind-tag.original {
  background: #f5f5f5;
  color: #757575;
}
.kind-tag.edited {
  background: #fff3e0;
  color: #e65100;
}
.edited-meta {
  flex: 1;
  margin-left: 12px;
  color: var(--text-secondary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
</style>