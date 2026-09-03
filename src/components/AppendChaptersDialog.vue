<template>
  <!-- 自持 overlay:本弹窗自带册页式 chrome(eyebrow/title/config 头部 + sticky footer),
       ui/Dialog.vue 的 header/footer 会被全部覆盖,故不复用它。z-index 仍走
       共享的 nextStack(),与其它 Dialog 同一递增序列,叠放行为一致。 -->
  <div
    v-if="open"
    class="lit-overlay"
    :style="{ zIndex: zIndexValue }"
    @click.self="open = false"
  >
    <div class="dialog-literary" data-role="dialog-root">
      <button class="close" type="button" title="关闭" @click="open = false">×</button>

      <!-- 头部:eyebrow(去哪个工作流)/ title(从第几章起补)/ config(怎么转) -->
      <header class="head">
        <div class="eyebrow" data-role="eyebrow">{{ eyebrowText }}</div>
        <h2 class="title" data-role="title">补充第 {{ firstAvailableIdx }} 章起 · 续作</h2>
        <div class="config" data-role="config">{{ configLine }}</div>
      </header>

      <!-- 状态条:已选 / 可补充 / 已在 batch,三段一行 -->
      <div class="status-strip">
        <div class="status-selected" data-role="status-selected">
          <span class="stat-num">{{ selectedChapterIds.size }}</span>
          <span class="stat-label">已选</span>
        </div>
        <div class="status-total" data-role="status-total">共 {{ availableSources.length }} 章可补充</div>
        <div class="status-inbatch" data-role="status-inbatch">{{ existingChapterIds.size }} 章已在 batch</div>
      </div>

      <div class="range-toolbar" data-role="range-toolbar">
        <span class="range-label">按 # 选</span>
        <input
          type="number"
          class="range-input"
          :class="{ 'has-error': rangeError !== null }"
          v-model.number="rangeFrom"
          :min="1"
          :max="allSources.length"
          placeholder="起"
          :disabled="allSources.length === 0"
          @keydown.enter="applyRange"
        />
        <span class="range-sep">—</span>
        <input
          type="number"
          class="range-input"
          :class="{ 'has-error': rangeError !== null }"
          v-model.number="rangeTo"
          :min="1"
          :max="allSources.length"
          placeholder="止"
          :disabled="allSources.length === 0"
          @keydown.enter="applyRange"
        />
        <select
          v-model="rangeMode"
          class="range-mode"
          :disabled="allSources.length === 0"
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
        >应用</Button>
        <Button
          size="small"
          :disabled="selectedChapterIds.size === 0"
          @click="clearSelection"
        >清空</Button>
      </div>

      <!-- loading / error -->
      <div v-if="loadErrorMessage" class="error">{{ loadErrorMessage }}</div>
      <div v-else-if="isLoading" class="loading">加载章节中...</div>
      <div v-else-if="availableSources.length === 0" class="empty">该工作流已经覆盖本转换工程下的全部章节,无可补充项。</div>

      <!-- 章节册页:全部源章节按 idx ASC 排,已在 batch 的仍在原位但 disabled -->
      <div v-else class="chapter-list">
        <label
          v-for="row in rowsForDisplay"
          :key="row.chapter_id"
          class="row"
          :class="{ 'is-selected': !row.inBatch && selectedChapterIds.has(row.chapter_id), 'in-batch': row.inBatch }"
          data-role="chapter-row"
          :data-in-batch="row.inBatch ? 'true' : 'false'"
        >
          <input
            type="checkbox"
            class="checkbox"
            :checked="row.inBatch ? false : selectedChapterIds.has(row.chapter_id)"
            :disabled="row.inBatch"
            :title="row.inBatch ? '已在该工作流中,不可重复选' : ''"
            @change="(e) => toggleSelect(row.chapter_id, (e.target as HTMLInputElement).checked)"
          />
          <span class="num" data-role="chapter-num">#{{ row.idx }}</span>
          <span class="chapter-title">
            <span v-if="row.inBatch" class="badge" data-role="in-batch-badge">已在 batch</span>
            {{ row.title }}
          </span>
          <span class="words">{{ row.word_count.toLocaleString('zh-Hans-CN') }} 字</span>
        </label>
      </div>

      <footer class="footer">
        <Button data-role="cancel-btn" @click="open = false">取消</Button>
        <Button
          kind="primary"
          data-role="confirm-btn"
          :loading="submitting"
          :disabled="selectedChapterIds.size === 0 || submitting"
          @click="onConfirm"
        >补充 {{ selectedChapterIds.size }} 章</Button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useQuery } from '@tanstack/vue-query';
import Button from './ui/Button.vue';
import { nextStack } from './ui/dialog-stack';
import { listTransformationSourceChapters, listWorkflowChapters } from '../ipc/commands';
import { formatPromptKind } from '../utils/prompt-locale';
import type { SourceChapterRow, WorkflowChapterRow } from '../ipc/types';

/// 「补充章节」对话框 spec(stopped-batch-append-chapters):
/// - 目标 batch 已 stopped,用户从这里选若干 source 章节追加进去。
/// - 已在 batch 中的章节不允许重复选(显示但 disabled,标记「已在 batch」)。
/// - 章节选择 UI 复用章节来源 tab 的 rangeFrom/To + rangeMode(replace/toggle),逻辑从
///   TransformationNovelDetail.vue 复制 —— Tasks 8/9 之后会一起抽。
const props = defineProps<{
  batchId: number;
  transformationNovelId: number;
  promptName: string;
  modelDisplayName: string;
  mode: 'compress' | 'style';
  ctxPrevOriginal: number;
  ctxPrevTransformed: number;
  ctxNextOriginal: number;
  /// 可选:workflow 自定义 label(由父组件从 WorkflowSummary.label 传入)。
  /// 旧 batch path 可传 null/undefined → 回退到「工作流 #batchId」形态。
  workflowLabel?: string | null;
}>();

const open = defineModel<boolean>('open', { required: true });

const emit = defineEmits<{
  confirm: [{ batchId: number; chapterIds: number[] }];
}>();

const selectedChapterIds = ref<Set<number>>(new Set());
const submitting = ref(false);

/// 源章节列表 —— 与 TransformationNovelDetail.vue 共用 queryKey,自动共享缓存 + 实时同步。
const sourcesQuery = useQuery({
  queryKey: ['transformationSourceChapters', props.transformationNovelId],
  queryFn: () => listTransformationSourceChapters(props.transformationNovelId),
});
const allSources = computed<SourceChapterRow[]>(() => sourcesQuery.data.value ?? []);

/// 当前 batch 已包含的章节 —— vue-query 按 [batchId] 缓存,跟 TransformationNovelDetail 里的
/// selectedWorkflowChaptersQuery 是同一个 key(命中已存在的缓存,这里不再发起请求)。
const workflowChaptersQuery = useQuery({
  queryKey: ['workflowChapters', props.batchId],
  queryFn: () => listWorkflowChapters(props.batchId),
});
const batchChapters = computed<WorkflowChapterRow[]>(() => workflowChaptersQuery.data.value ?? []);

/// 已存在于该 batch 的 chapter_id 集合 —— 用于过滤掉不可重复选的章节。
const existingChapterIds = computed<Set<number>>(
  () => new Set(batchChapters.value.map((c) => c.chapter_id)),
);

/// 可补充的章节 = 全部 source - 已存在 batch 的章节。
const availableSources = computed<SourceChapterRow[]>(() => {
  const ex = existingChapterIds.value;
  return allSources.value.filter((s) => !ex.has(s.chapter_id));
});

/// 标题用:第一个还能补的章节序号。全在 batch 里 / 还没加载出来时退化成 1。
const firstAvailableIdx = computed<number>(() => availableSources.value[0]?.idx ?? 1);

/// eyebrow 单行 —— 有 label 用「label」,没 label 用「工作流 #batchId」。
/// CSS 走 ellipsis,不在此处截断字符串(保留完整 label 给可访问性 / 调试)。
const eyebrowText = computed<string>(() => {
  const target = props.workflowLabel
    ? `「${props.workflowLabel}」`
    : `工作流 #${props.batchId}`;
  return `续${target} · ${props.modelDisplayName}`;
});

/// 头部配置单行 mono —— 模式 / 提示词 / 三路上下文,用 · 串起来。
const configLine = computed<string>(() => [
  formatPromptKind(props.mode),
  props.promptName,
  `前文原文 ×${props.ctxPrevOriginal}`,
  `前文转换 ×${props.ctxPrevTransformed}`,
  `后文原文 ×${props.ctxNextOriginal}`,
].join(' · '));

const isLoading = computed<boolean>(
  () => sourcesQuery.isLoading.value || workflowChaptersQuery.isLoading.value,
);

/// vue-query 的 error 是响应式 Ref;在 template 里直接展示两路任一错误即可。
const loadErrorMessage = computed<string | null>(() => {
  const err = sourcesQuery.error.value ?? workflowChaptersQuery.error.value;
  if (!err) return null;
  return err instanceof Error ? err.message : String(err);
});

/// 章节册页:全部源章节按 idx ASC 一次排完(册页目录形态),已在 batch 的留在原位置
/// 标 disabled —— 用户按"第几章"找章节,不该因为已补过就被挪到末尾。
type DisplayRow = SourceChapterRow & { inBatch: boolean };
const rowsForDisplay = computed<DisplayRow[]>(() =>
  allSources.value.map((s) => ({ ...s, inBatch: existingChapterIds.value.has(s.chapter_id) })),
);

function toggleSelect(chapterId: number, on: boolean) {
  const next = new Set(selectedChapterIds.value);
  if (on) next.add(chapterId); else next.delete(chapterId);
  selectedChapterIds.value = next;
}

function clearSelection() {
  selectedChapterIds.value = new Set();
}

// range 选择 —— 与 TransformationNovelDetail.vue 同形代码。Task 8/9 后整体抽。
// 注意:idx 是全 source 列表的序号(1..allSources.length),不是 availableSources 的序号。
// rangeError 需按全源数量校验(用户视角的"第 10 章"),applyRange 过滤时再跳过已存在 batch 的。
type RangeMode = 'replace' | 'toggle';
const rangeMode = ref<RangeMode>('replace');
const rangeFrom = ref<number | null>(null);
const rangeTo = ref<number | null>(null);
const rangeError = computed<string | null>(() => {
  const total = allSources.value.length;
  if (total === 0) return null;
  if (rangeFrom.value === null || rangeTo.value === null) return null;
  if (rangeFrom.value < 1 || rangeTo.value < 1) return '序号需 ≥ 1';
  if (rangeFrom.value > total || rangeTo.value > total) {
    return `序号需 ≤ ${total}`;
  }
  return null;
});
function applyRange() {
  const list = availableSources.value;
  if (list.length === 0) return;
  if (rangeError.value !== null) return;
  if (rangeFrom.value === null || rangeTo.value === null) return;
  const lo = Math.min(rangeFrom.value, rangeTo.value);
  const hi = Math.max(rangeFrom.value, rangeTo.value);
  const targetIds = new Set<number>(
    list.filter((s) => s.idx >= lo && s.idx <= hi).map((s) => s.chapter_id),
  );
  if (rangeMode.value === 'replace') {
    selectedChapterIds.value = targetIds;
  } else {
    const next = new Set(selectedChapterIds.value);
    for (const id of targetIds) {
      if (next.has(id)) next.delete(id); else next.add(id);
    }
    selectedChapterIds.value = next;
  }
}

/// 弹窗层级:每次打开取一次全局递增 z-index,保证后开的弹窗压在先开的之上
/// (计数器在 ui/dialog-stack.ts,与 ui/Dialog.vue 共用同一序列)。
const zIndexValue = ref(1000);

watch(open, (v) => {
  if (!v) return;
  zIndexValue.value = nextStack();
  submitting.value = false;
  selectedChapterIds.value = new Set();
  rangeFrom.value = null;
  rangeTo.value = null;
  rangeMode.value = 'replace';
  // sources / batchChapters 由 vue-query 自动订阅 + 自动重试;loadErrorMessage 是
  // computed,会自动跟着 query 的 error 状态变化,无需在这里手动同步。
}, { immediate: true });

function onConfirm() {
  if (selectedChapterIds.value.size === 0) return;
  if (submitting.value) return;
  submitting.value = true;
  try {
    emit('confirm', {
      batchId: props.batchId,
      chapterIds: [...selectedChapterIds.value],
    });
    // 不在这里关 dialog —— 父组件成功后再关,失败保留选项让用户改。
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
/*
  册页形态(literary / catalog):
  - 头部三层 —— mono eyebrow(工作流身份)/ 宋体大标题(从第几章起)/ mono 单行配置。
  - 章节行 = 目录条目:大号 mono 序号 + 宋体章名 + 右对齐 mono 字数。
  - 选中 = 朱砂红 4px 左边线(印章语义,不是警告)。
  - 配色一律走项目 token(纸/墨/朱砂),不写死 hex —— 否则 dark 主题下白卡黑字会炸。
*/
.lit-overlay {
  position: fixed;
  inset: 0;
  background: rgba(26, 23, 20, 0.45);
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-literary {
  /* 局部 palette:统一映射到全局 token,方便这套视觉整体调 */
  --lit-sheet: var(--color-sheet);
  --lit-paper: var(--color-paper);
  --lit-ink: var(--text-primary);
  --lit-secondary: var(--text-secondary);
  --lit-muted: var(--text-muted);
  --lit-rule: var(--border-color);
  --lit-rule-soft: var(--border-soft);
  --lit-vermillion: var(--color-cinnabar);
  --lit-chapter-num: 1em;

  position: relative;
  width: 900px;
  max-width: calc(100vw - 48px);
  max-height: 88vh;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  background: var(--lit-sheet);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-card);
  box-shadow: 0 12px 36px rgba(26, 23, 20, 0.18);
  overflow: hidden;
}
.dialog-literary::before {
  /* 顶部一道窄朱印条 —— 与 ui/Dialog 保持同一册页语汇 */
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  height: 2px;
  background: var(--lit-vermillion);
}
.close {
  position: absolute;
  top: 10px;
  right: 14px;
  background: none;
  border: none;
  padding: 0;
  line-height: 1;
  font-size: 22px;
  font-family: var(--font-serif);
  color: var(--lit-muted);
  cursor: pointer;
}
.close:hover {
  color: var(--lit-vermillion);
}

/* ── 头部 ─────────────────────────────────────────── */
.head {
  padding: 26px 32px 20px;
  border-bottom: 1px solid var(--lit-rule-soft);
}
.eyebrow {
  font-family: var(--font-mono);
  font-size: var(--text-micro);
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--lit-muted);
  /* 长 label 不撑爆头部 —— CSS 截断,不在 JS 里改字符串(保留完整 label) */
  max-width: 100%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.title {
  margin: 10px 0 0;
  font-family: var(--font-serif);
  font-size: var(--text-h1);
  font-weight: var(--font-weight-semibold);
  line-height: var(--leading-tight);
  letter-spacing: -0.01em;
  color: var(--lit-ink);
}
.config {
  margin-top: 10px;
  font-family: var(--font-mono);
  font-size: var(--text-caption);
  color: var(--lit-secondary);
}

/* ── 状态条 ───────────────────────────────────────── */
.status-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-5);
  padding: 14px 32px;
  background: var(--lit-paper);
  border-bottom: 1px solid var(--lit-rule-soft);
}
.status-selected {
  display: flex;
  align-items: baseline;
  gap: var(--space-3);
}
.stat-num {
  font-family: var(--font-mono);
  font-size: var(--text-h1);
  line-height: 1;
  font-variant-numeric: tabular-nums;
  color: var(--lit-vermillion);
}
.stat-label {
  font-size: var(--text-caption);
  color: var(--lit-secondary);
}
.status-total {
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--lit-secondary);
}
.status-inbatch {
  font-family: var(--font-mono);
  font-size: var(--text-micro);
  padding: 3px 10px;
  border: 1px solid var(--lit-rule);
  border-radius: var(--radius-pill);
  background: var(--lit-sheet);
  color: var(--lit-muted);
  white-space: nowrap;
}

/* ── range 工具条 ─────────────────────────────────── */
.range-toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  padding: 10px 32px;
  border-bottom: 1px solid var(--lit-rule-soft);
}
.range-label {
  margin-right: 2px;
  font-size: var(--text-caption);
  color: var(--lit-muted);
}
.range-input {
  width: 56px;
  height: 28px;
  padding: 0 6px;
  border: 1px solid var(--lit-rule);
  border-radius: var(--radius-pin);
  background: var(--lit-sheet);
  color: var(--lit-ink);
  font-family: var(--font-mono);
  font-size: var(--text-caption);
  box-sizing: border-box;
}
.range-input.has-error {
  border-color: var(--danger);
}
.range-input:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.range-sep {
  color: var(--lit-muted);
  font-size: 13px;
}
.range-mode {
  height: 28px;
  padding: 0 6px;
  border: 1px solid var(--lit-rule);
  border-radius: var(--radius-pin);
  background: var(--lit-sheet);
  color: var(--lit-ink);
  font-size: var(--text-caption);
  font-family: inherit;
}

/* ── 章节册页 ─────────────────────────────────────── */
.chapter-list {
  flex: 1;
  min-height: 300px;
  overflow: auto;
  padding: 4px 32px 8px;
}
.row {
  display: grid;
  grid-template-columns: 18px 84px 1fr auto;
  align-items: center;
  gap: var(--space-5);
  padding: 6px 12px;
  border-bottom: 1px solid var(--lit-rule-soft);
  cursor: pointer;
}
.row:last-child {
  border-bottom: none;
}
.row:hover:not(.in-batch) {
  background: var(--bg-hover);
}
.row.is-selected {
  background: var(--bg-active);
  box-shadow: inset 4px 0 0 var(--lit-vermillion);
}
.row.in-batch {
  opacity: 0.5;
  cursor: not-allowed;
}
.checkbox {
  accent-color: var(--lit-vermillion);
  cursor: inherit;
}
.num {
  font-family: var(--font-mono);
  font-size: var(--lit-chapter-num);
  line-height: 1.1;
  font-variant-numeric: tabular-nums;
  text-align: right;
  color: var(--lit-muted);
}
.row.is-selected .num {
  color: var(--lit-vermillion);
}
.chapter-title {
  /* 宋体章名 —— 中文不用 italic(合成斜体会把字面压歪) */
  font-family: var(--font-serif);
  font-size: 16px;
  color: var(--lit-ink);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.words {
  font-family: var(--font-mono);
  font-size: var(--text-caption);
  font-variant-numeric: tabular-nums;
  color: var(--lit-muted);
}
.badge {
  margin-right: var(--space-3);
  padding: 2px 8px;
  border: 1px solid var(--lit-rule);
  border-radius: var(--radius-pin);
  background: var(--lit-paper);
  color: var(--lit-muted);
  font-family: var(--font-mono);
  font-size: var(--text-micro);
  vertical-align: 2px;
}

/* ── 状态提示 ─────────────────────────────────────── */
.error {
  margin: 16px 32px;
  padding: 8px 10px;
  border: 1px solid var(--danger-border);
  border-radius: var(--radius-pin);
  background: var(--danger-bg);
  color: var(--danger);
  font-size: 13px;
}
.loading,
.empty {
  flex: 1;
  padding: 48px 32px;
  text-align: center;
  font-size: 13px;
  color: var(--lit-muted);
}

/* ── footer ───────────────────────────────────────── */
.footer {
  position: sticky;
  bottom: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-3);
  padding: 16px 32px;
  background: var(--lit-sheet);
  border-top: 1px solid var(--lit-rule);
}
</style>
