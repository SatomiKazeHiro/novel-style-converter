<template>
  <section class="upload">
    <PageHeader :title="filename || '加载中...'" size="small">
      <template #back>
        <Button aria-label="返回" @click="onBack">
          <IconArrowLeft :size="16" :stroke-width="1.5" />
        </Button>
      </template>
      <template #actions>
        <Button :loading="saving" :disabled="!dirty" @click="save">保存</Button>
        <Button
          :disabled="uploadId == null || !textLoaded || dirty"
          @click="openCleaning"
        >清洗</Button>
        <Button kind="primary" :disabled="uploadId == null || dirty" @click="goParse">转为数据资产</Button>
      </template>
    </PageHeader>
    <div v-if="error" class="alert">{{ error }}</div>
    <div v-if="uploadId != null" class="meta-strip">
      <div class="tags">
        <Tag>实体文件</Tag>
      </div>
      <span class="meta-text" :title="metaTooltip">
        <template v-if="textLoaded">{{ mbSize }} MB · {{ formatWordCount(wordCount) }}</template>
        <template v-else-if="totalBytes > 0">
          加载原文 {{ formatBytes(loadedBytes) }} / {{ formatBytes(totalBytes) }}
          ({{ Math.floor((loadedBytes / totalBytes) * 100) }}%)
        </template>
        <template v-else>{{ mbSize }} MB · {{ formatWordCount(wordCount) }}</template>
      </span>
    </div>
    <div class="body">
      <div v-if="textLoaded" ref="cmContainer" class="cm-host" />
      <div v-else class="cm-host cm-host--loading">
        <span v-if="error">{{ error }}</span>
        <span v-else-if="totalBytes > 0">
          原文加载中... {{ Math.floor((loadedBytes / totalBytes) * 100) }}%
        </span>
        <span v-else>原文加载中...</span>
      </div>
    </div>
    <CleaningDialog
      v-model:open="cleaningOpen"
      :source-text="rawText"
      @confirm="onCleaningConfirm"
    />

    <AlertDialog
      v-model:open="alertOpen"
      :title="alertTitle"
      :message="alertMessage"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue';
import type { EditorView as EditorViewType } from '@codemirror/view';
import { useRoute, useRouter } from 'vue-router';
import Button from '../components/ui/Button.vue';
import Tag from '../components/ui/Tag.vue';
import PageHeader from '../components/ui/PageHeader.vue';
import ConfirmDialog from '../components/ui/ConfirmDialog.vue';
import AlertDialog from '../components/ui/AlertDialog.vue';
import IconArrowLeft from '~icons/lucide/arrow-left';
import { getUpload, getUploadTextChunk, updateUploadText } from '../ipc/commands';
import { formatWordCount } from '../utils/format';
import type { UploadSummary } from '../ipc/types';
import CleaningDialog from '../components/CleaningDialog.vue';

const route = useRoute();
const router = useRouter();
const uploadId = ref<number | null>(null);
const filename = ref('');
const sha256 = ref('');
const byteSize = ref(0);
const uploadMeta = ref<UploadSummary | null>(null);
// rawText 可能十几 MB,用 shallowRef 跳过 Vue deep proxy,赋值/读取都直接走原生 string,省掉逐字符响应式追踪。
/// CodeMirror 6 view -- textarea not virtualized, lags on 1M+ chars.
/// EditorView renders only visible lines + buffer, DOM decoupled from text size.
let cmView: EditorViewType | null = null;
const cmContainer = ref<HTMLDivElement | null>(null);

const rawText = shallowRef('');
const textLoaded = ref(false);
// 大文件分块加载的进度反馈
const loadedBytes = ref(0);
const totalBytes = ref(0);
const CHUNK_STEP = 256 * 1024; // 256 KB / 块
const dirty = ref(false);
const saving = ref(false);
const error = ref<string | null>(null);
const cleaningOpen = ref(false);
const alertOpen = ref(false);
const alertTitle = ref('提示');
const alertMessage = ref('');

onMounted(async () => {
  const id = Number(route.params.uploadId);
  if (!Number.isFinite(id) || id <= 0) {
    error.value = `无效的上传 ID: ${String(route.params.uploadId)}`;
    return;
  }
  uploadId.value = id;
  // meta 先拿:标题/字节/字数立即可用,不等大文本
  try {
    const meta = await getUpload(id);
    filename.value = meta?.filename ?? '';
    sha256.value = meta?.sha256 ?? '';
    byteSize.value = meta?.byte_size ?? 0;
    uploadMeta.value = meta ?? null;
    // dirty 在 load 后保持 false,首次编辑时才置 true
    watch(rawText, () => { dirty.value = true; });
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
    return;
  }
  // 大文本分块拉:meta 已先出,原文按 256 KB / 块串行拉,进度条同步更新。
  // 全部到位后一次性 join + 单次 cmView 渲染(避免 57 次 O(n^2) 拼接),这一步本身会卡几秒到十几秒,是浏览器硬件限制。
  totalBytes.value = byteSize.value;
  loadedBytes.value = 0;
  rawText.value = '';
  textLoaded.value = false;
  try {
    await loadUploadTextInChunks(id);
    textLoaded.value = true;
    // wait for v-if to mount cmContainer before constructing EditorView
    await nextTick();
    await mountEditor(rawText.value);
    // 程序赋值(rawText 从 '' → 完整原文)也会触发 dirty watcher,加载完成后重置,
    // 避免一进页面"保存"按钮就亮起。
    dirty.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
});

/// 串行按字节区间拉原文,先积到数组里不触发 cmView 重渲染,
/// 全部到位后再一次性 join 写入 rawText(避免 57 次字符串拼接变 O(n^2))。
async function loadUploadTextInChunks(id: number): Promise<void> {
  let offset = 0;
  const total = totalBytes.value;
  const parts: string[] = [];
  while (offset < total) {
    const chunk = await getUploadTextChunk(id, offset, CHUNK_STEP);
    if (chunk.length === 0) break;
    parts.push(chunk);
    offset += new TextEncoder().encode(chunk).length;
    if (offset > total) offset = total;
    loadedBytes.value = offset;
  }
  // 一次性 join + 单次 rawText 赋值
  rawText.value = parts.join('');
}

// 字数直接用 DB 里的 word_count(upload_file 时一次性算好存表),不重复从 rawText 算
// rawText 可能十几 MB,逐字符统计会卡住首屏
const wordCount = computed(() => uploadMeta.value?.word_count ?? 0);
const mbSize = computed(() => (byteSize.value / 1024 / 1024).toFixed(2));
const metaTooltip = computed(() => (sha256.value ? `SHA256: ${sha256.value}` : ''));
/// 与后端 byte_size 对齐的字节数展示,大文件用 MB 单位,小文件用 KB / B。
function formatBytes(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(2)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${n} B`;
}

function onBack() {
  void router.push('/uploads');
}

async function save() {
  if (uploadId.value == null || !dirty.value) return;
  saving.value = true;
  error.value = null;
  try {
    await updateUploadText(uploadId.value, rawText.value);
    dirty.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    saving.value = false;
  }
}

function goParse() {
  if (uploadId.value == null) return;
  void router.push({ name: 'parse-wizard', params: { uploadId: uploadId.value } });
}

async function openCleaning() {
  if (uploadId.value == null) return;
  if (rawText.value.length > 10 * 1024 * 1024) {
    alertMessage.value = '文本过大,请先手动精箁';
    alertOpen.value = true;
    return;
  }
  // 新设计:清洗只改 uploads.original_text,不影响已有 chapters.body,无需二次确认。
  cleaningOpen.value = true;
}

function onCleaningConfirm(cleanedText: string) {
  rawText.value = cleanedText;
  replaceEditorText(cleanedText);
  // watch(rawText) 触发 → dirty 置 true;与手动编辑走同一保存路径。
}

/// 构造 EditorView —— 仅引 state / view / commands 三个包,无高亮/搜索/补全。
/// 主题用 CSS 变量,亮/暗主题切换无需 JS 干预。
async function mountEditor(initialText: string): Promise<void> {
  if (!cmContainer.value) return;
  cmView?.destroy();
  // 动态 import —— Vite 把 CM6 拆为独立 chunk,只在进入此页时才下载。
  const [{ EditorState }, { EditorView, keymap, drawSelection }, { defaultKeymap, history, historyKeymap }] = await Promise.all([
    import('@codemirror/state'),
    import('@codemirror/view'),
    import('@codemirror/commands'),
  ]);
  cmView = new EditorView({
    state: EditorState.create({
      doc: initialText,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        drawSelection(),
        EditorView.lineWrapping,
        EditorView.theme({
          '&': {
            height: '100%',
            fontSize: '13px',
            fontFamily: 'var(--font-mono), ui-monospace, monospace',
            color: 'var(--text-primary)',
            backgroundColor: 'var(--color-sheet)',
            border: '1px solid var(--border-color)',
            borderRadius: 'var(--radius-pin)',
          },
          '&.cm-focused': {
            outline: 'none',
            borderColor: 'var(--border-strong)',
          },
          '.cm-content': {
            padding: '12px',
            caretColor: 'var(--color-cinnabar)',
          },
          '.cm-scroller': { fontFamily: 'inherit' },
          '.cm-gutters': { display: 'none' },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged && cmView) {
            rawText.value = cmView.state.doc.toString();
          }
        }),
      ],
    }),
    parent: cmContainer.value,
  });
}

/// 一次性把新文本塞回 EditorView(等价于给 textarea v-model 赋整段值)
function replaceEditorText(next: string): void {
  if (!cmView) return;
  cmView.dispatch({
    changes: { from: 0, to: cmView.state.doc.length, insert: next },
  });
}

onUnmounted(() => {
  cmView?.destroy();
  cmView = null;
});
</script>

<style scoped>
.upload {
  display: flex;
  flex-direction: column;
  height: 100%;
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
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
  cursor: default;
}
.body {
  display: flex;
  gap: 16px;
  flex: 1;
  min-height: 0;
  margin-top: 8px;
}
.cm-host {
  flex: 1;
  min-height: 0;
  border-radius: var(--radius-pin);
  overflow: hidden;
}
.cm-host--loading {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  font-size: 13px;
  font-style: italic;
  font-family: var(--font-serif);
  background: var(--color-sheet);
  border: 1px solid var(--border-color);
}
.alert {
  margin-top: 12px;
  padding: 8px 12px;
  background: var(--color-paper-mist);
  color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin);
  font-size: 13px;
}
</style>
