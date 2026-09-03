<template>
  <section class="chapters">
    <PageHeader title="章节解析" subtitle="调整章节 marker，提交为数据资产">
      <template #back>
        <Button aria-label="返回" @click="onBack">
          <IconArrowLeft :size="16" :stroke-width="1.5" />
        </Button>
      </template>
      <template #actions>
        <Button @click="onReset" :disabled="!store.dirty">重置 marker</Button>
        <Button
          v-if="store.committed && !store.dirty"
          title="丢弃已保存的章节,重新走 splitter"
          @click="onResplit"
        >重新切分</Button>
        <Button kind="primary" :loading="committing" :disabled="committing || store.workingChapters.length === 0" @click="onCommit">保存为数据资产</Button>
      </template>
    </PageHeader>

    <div v-if="store.error" class="alert">{{ store.error }}</div>
    <div v-else-if="store.committed && !store.dirty" class="info">
      当前是已保存的章节。如需重新解析，请在 DataAsset 页先删除已有数据资产。
    </div>

    <div class="panes">
      <div class="pane">
        <div class="pane-title">
          <span>章节列表({{ store.workingChapters.length }})</span>
          <span v-if="store.loading && store.workingChapters.length === 0" class="pane-hint">分析中...</span>
        </div>
        <DynamicScroller
          v-if="chaptersWithIdx.length > 0"
          class="scroller"
          :items="chaptersWithIdx"
          :min-item-size="48"
          :key-field="'idx'"
        >
          <template #default="{ item, active }">
            <DynamicScrollerItem
              :item="item"
              :active="active"
              :size-dependencies="[item?.title ?? '']"
            >
              <div v-if="segIdx(item) >= 0" class="seg-row" @click="onChapterClick(item)">
                <span class="seg-idx">{{ segIdx(item) + 1 }}</span>
                <input
                  class="seg-title"
                  :value="displayTitle(item.title)"
                  :title="displayTitle(item.title)"
                  @click.stop
                  @input="onTitleEdit(segIdx(item), ($event.target as HTMLInputElement).value)"
                />
                <span class="seg-size" :title="`${item.word_count} 字`">{{ formatWordCount(item.word_count) }}</span>
              </div>
            </DynamicScrollerItem>
          </template>
        </DynamicScroller>
        <div v-else-if="store.loading" class="pane-empty">
          <span>正在分析章节...</span>
        </div>
        <div v-else class="pane-empty">
          <span>暂无章节</span>
        </div>
      </div>

      <div class="pane">
        <div class="pane-title">原文</div>
        <div class="search-toolbar">
          <input
            class="search-input"
            placeholder="全文搜索"
            :value="searchQuery"
            @input="onSearchInput(($event.target as HTMLInputElement).value)"
          />
          <span class="search-counter">{{ counterText }}</span>
          <Button size="small" :disabled="hitCount === 0" @click="onPrevHit">‹</Button>
          <Button size="small" :disabled="hitCount === 0" @click="onNextHit">›</Button>
        </div>
        <div ref="cmHost" class="cm-host" />
      </div>
    </div>

    <Dialog v-model:open="commitDialogOpen" title="保存为数据资产">
      <p>请输入数据资产标题:</p>
      <input
        class="title-input"
        :value="pendingTitle"
        placeholder="例如:第 1 卷"
        @input="pendingTitle = ($event.target as HTMLInputElement).value"
        @keyup.enter="confirmCommit"
      />
      <p class="hint">提交后 data_asset 即被锁定,需要重新解析会创建新 data_asset。</p>
      <template #footer>
        <Button @click="cancelCommit">取消</Button>
        <Button kind="primary" :disabled="!pendingTitle.trim()" @click="confirmCommit">确认</Button>
      </template>
    </Dialog>

    <ConfirmDialog
      v-model:open="resplitConfirmOpen"
      title="重新解析"
      message="丢弃已保存的章节,重新走 splitter?"
      kind="danger"
      confirm-text="重新切分"
      @confirm="doResplit"
    />

    <AlertDialog
      v-model:open="alertOpen"
      title="提示"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import IconArrowLeft from '~icons/lucide/arrow-left';
import { DynamicScroller, DynamicScrollerItem } from 'vue-virtual-scroller';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import Button from '../components/ui/Button.vue';
import Dialog from '../components/ui/Dialog.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import { useChaptersStore } from '../stores/chapters';
import { useParseEditor } from '../composables/useParseEditor';
import { formatWordCount } from '../utils/format';
import type { ChapterSegment } from '../ipc/types';

const route = useRoute();
const router = useRouter();
const store = useChaptersStore();

const committing = ref(false);

const commitDialogOpen = ref(false);
const pendingTitle = ref('');

const resplitConfirmOpen = ref(false);
const alertOpen = ref(false);
const alertMessage = ref('');


/// 章节列表按数组下标加 idx, 给 DynamicScroller 当唯一 key。
/// (ChapterSegment 本身没有 id;title+content 可能撞。)
const chaptersWithIdx = computed(() =>
  store.workingChapters.map((s, idx) => ({ ...s, idx })),
);
const boundarySet = computed(() => store.chapterSplits);

/// CM6 行按钮回调。CM6 给的是 1-based 行号,store 用 0-based lineKey。
function onBoundaryToggle(line1based: number) {
  store.toggleChapterSplit(String(line1based - 1));
}

const searchQuery = ref<string>('');
const cmHost = ref<HTMLDivElement | null>(null);
const cmEditor = useParseEditor({
  boundarySet,
  host: cmHost,
  onBoundaryToggle,
});
const hitCount = computed(() => cmEditor.hitCount.value);
const currentHitIndex = computed(() => cmEditor.currentHitIndex.value);

const counterText = computed(() => {
  const total = hitCount.value;
  if (total === 0) return '0 / 0';
  return `${currentHitIndex.value} / ${total}`;
});

function scrollToActiveHit() {
  // composable's nextHit/prevHit already scrollIntoView; nothing to do here.
}

// 路由 uploadId 变化时重新拉数据;immediate:true 让首次挂载也跑。
// 用 watch 而非 onMounted:同组件复用(路由 param 变)onMounted 不会再触发。
// 仅触发 store.load;mount 在 rawText 落地后再挂(下方第二个 watch)。
watch(
  () => Number(route.params.uploadId),
  (id) => {
    if (Number.isFinite(id) && id > 0) {
      void store.load(id);
    }
  },
  { immediate: true },
);

// rawText 一旦从 IPC 拉回来 → 挂 EditorView。原先的 nextTick 检查
// 永远跑不到 mount,因为 IPC 比 nextTick 慢一拍,那时 rawText 还是 ''。
watch(
  () => store.rawText,
  (text) => {
    if (!text) return;
    void cmEditor.mount(text);
    // store stores 0-based line keys; CM6 doc.line() is 1-based, so +1.
    cmEditor.setMarkers(new Set([...store.chapterSplits].map((m) => Number(m) + 1)));
  },
);

watch(
  () => store.chapterSplits,
  (splits) => {
    // store stores 0-based line keys; CM6 doc.line() is 1-based, so +1.
    const lines1based = [...splits].map((m) => Number(m) + 1);
    cmEditor.setMarkers(new Set(lines1based));
  },
  { deep: false },
);

/// 离开 parse 页时清空 store:释放 rawText 等大对象内存,避免 watch 防抖悬挂。
onUnmounted(() => {
  cmEditor.destroy();
  store.unload();
});

function onBack() {
  void router.push('/uploads');
}

/// 点击章节行 → 跳转到右侧原文对应位置。
/// Button 和 input 上的 @click.stop 已拦住冒泡,这里只处理"点空白处"。
function onChapterClick(item: ChapterSegment) {
  const line = item.title_line;
  if (line < 0) return;
  void nextTick(() => { cmEditor.scrollToLine(line); });
}

function onTitleEdit(idx: number, value: string) {
  const seg = store.workingChapters[idx];
  if (!seg) return;
  store.updateTitle(String(seg.title_line), value);
}

function displayTitle(t: unknown): string {
  if (typeof t !== 'string' || t.length === 0) return '(无标题)';
  return t;
}

function segIdx(item: ChapterSegment | null | undefined): number {
  if (!item) return -1;
  // chaptersWithIdx 里 item.idx 是数组下标
  return (item as { idx?: number }).idx ?? -1;
}

function onSearchInput(value: string) {
  searchQuery.value = value;
  cmEditor.runSearch(value);
}

function onNextHit() { cmEditor.nextHit(); }
function onPrevHit() { cmEditor.prevHit(); }

function onReset() {
  store.reset();
}

async function onResplit() {
  resplitConfirmOpen.value = true;
}

async function doResplit() {
  await store.reSplit();
}

function onCommit() {
  pendingTitle.value = '';
  commitDialogOpen.value = true;
}

function cancelCommit() {
  commitDialogOpen.value = false;
}

async function confirmCommit() {
  const title = pendingTitle.value.trim();
  if (!title) return;
  committing.value = true;
  commitDialogOpen.value = false;
  try {
    const newDataAssetId = await store.commit(title);
    void router.push(`/library/data/${newDataAssetId}`);
  } catch (e: unknown) {
    alertMessage.value = e instanceof Error ? e.message : String(e);
    alertOpen.value = true;
  } finally {
    committing.value = false;
  }
}
</script>

<style scoped>
.chapters {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.alert {
  padding: 12px 16px;
  background: var(--bg-hover);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  margin-bottom: 12px;
}
.info {
  padding: 8px 16px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  border-radius: var(--radius-pin);
  margin-bottom: 12px;
  font-size: 13px;
}
.panes {
  display: flex;
  gap: 16px;
  flex: 1;
  min-height: 0;
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
.pane-title {
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  font-size: 13px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.scroller {
  flex: 1;
  overflow-y: auto;
}
/* Container for the EditorView; CM6-child CSS lives in the composable's
   EditorView.theme() so Vue's scoped attribute doesn't shadow it. */
.cm-host {
  flex: 1;
  min-height: 0;
  border-top: 1px solid var(--border-color);
  overflow: hidden;
}
.seg-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border-color);
  cursor: pointer;
}
.seg-row:hover { background: var(--bg-hover); }
.seg-idx {
  color: var(--text-secondary);
  font-size: 12px;
  min-width: 32px;
  flex-shrink: 0;
}
.seg-title {
  flex: 1;
  min-width: 0;
  padding: 0 8px;
  height: 28px;
  border: 1px solid transparent;
  border-radius: var(--radius-pin);
  font-size: 13px;
  font-family: inherit;
  background: transparent;
  outline: none;
  color: var(--text-primary);
}
.seg-title:hover { border-color: var(--border-color); }
.seg-title:focus { border-color: var(--border-strong); }
.seg-size {
  color: var(--text-secondary);
  font-size: 12px;
  flex-shrink: 0;
}
.search-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.search-input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-pin);
  font-size: 13px;
  font-family: inherit;
  outline: none;
  background: transparent;
  color: var(--text-primary);
}
.search-input:hover { border-color: var(--border-color); }
.search-input:focus { border-color: var(--border-strong); }
.search-counter {
  font-size: 12px;
  color: var(--text-secondary);
  min-width: 64px;
  text-align: center;
  flex-shrink: 0;
}
.pane-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.pane-hint {
  font-size: 11px;
  color: var(--text-muted);
  font-style: italic;
}
.pane-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  font-size: 13px;
  font-style: italic;
  font-family: var(--font-serif);
}
.hint {
  margin: 12px 0 0;
  font-size: 12px;
  color: var(--text-secondary);
}
.title-input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  font-size: 14px;
  font-family: inherit;
  background: var(--color-sheet);
  color: var(--text-primary);
  outline: none;
  box-sizing: border-box;
}
.title-input:focus { border-color: var(--border-strong); }
</style>

