<template>
  <Dialog v-model:open="open" title="清洗预览" :width="1100" size="full">
    <div class="body">
      <aside class="rules">
        <h3 class="rules-title">规则</h3>
        <ul class="rule-list">
          <li v-for="(rule, idx) in selectedRules" :key="rule.id" class="rule-item">
            <label>
              <input
                type="checkbox"
                :checked="rule.enabled"
                @change="toggleRule(rule.id, ($event.target as HTMLInputElement).checked)"
              />
              <span>{{ rule.label }}</span>
            </label>
            <div class="reorder">
              <button
                type="button"
                :disabled="idx === 0"
                @click="move(rule.id, -1)"
              >↑</button>
              <button
                type="button"
                :disabled="idx === selectedRules.length - 1"
                @click="move(rule.id, 1)"
              >↓</button>
            </div>
          </li>
        </ul>
        <div class="stats" v-if="preview">
          {{ preview.lines_delta >= 0 ? '增加' : '减少' }} {{ Math.abs(preview.lines_delta) }} 行 · Δ {{ preview.chars_delta >= 0 ? '+' : '' }}{{ preview.chars_delta }} 字符
        </div>
        <div class="error" v-if="error">{{ error }}</div>
      </aside>
      <div class="previews">
        <div class="pane">
          <h4 class="pane-title">原文本</h4>
          <div ref="sourceCmHost" class="cm-readonly-host" />
        </div>
        <div class="pane">
          <h4 class="pane-title">预览结果</h4>
          <div class="pane-body">
            <div ref="previewCmHost" class="cm-readonly-host" />
            <div v-if="cleaning" class="cleaning-overlay" role="status" aria-live="polite">
              <span class="cleaning-label">清洗中</span>
            </div>
          </div>
        </div>
      </div>
    </div>
    <template #footer>
      <Button @click="onCancel">取消</Button>
      <Button
        kind="primary"
        :disabled="!canConfirm"
        @click="onConfirm"
      >确认回填</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue';
import type { EditorView as EditorViewType } from '@codemirror/view';
import { useDebounceFn } from '@vueuse/core';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import { previewCleaning } from '../ipc/commands';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ confirm: [cleanedText: string] }>();
const props = defineProps<{ sourceText: string }>();

interface RuleRow {
  id:
    | 'add_indent_to_unindented'
    | 'merge_short_paragraphs'
    | 'collapse_blank_runs'
    | 'ensure_blank_line_between_paragraphs';
  label: string;
  enabled: boolean;
}

// 顺序必须合并先于缩进:加缩进后每行都以 　　 开头,merge 的 next.starts_with(INDENT)
// 守卫会跳过这些行 → 永远合并不上。跟 nsc-core 默认规则一致。
const initialRules: RuleRow[] = [
  { id: 'merge_short_paragraphs', label: '合并段落', enabled: true },
  { id: 'ensure_blank_line_between_paragraphs', label: '段落间空一行', enabled: true },
  { id: 'add_indent_to_unindented', label: '加缩进', enabled: true },
  { id: 'collapse_blank_runs', label: '折叠空行', enabled: true },
];

const selectedRules = ref<RuleRow[]>(initialRules.map((r) => ({ ...r })));
const preview = ref<{ cleaned_text: string; lines_delta: number; chars_delta: number } | null>(null);
const error = ref<string | null>(null);
/// IPC 在飞时遮罩预览面板,避免大文件清洗卡顿让用户误以为是 bug。
const cleaning = ref(false);

/// 两个只读 CM6 视图 —— 同样用动态 import 拆 chunk,只在打开此弹窗时才下载。
let sourceCmView: EditorViewType | null = null;
let previewCmView: EditorViewType | null = null;
const sourceCmHost = ref<HTMLDivElement | null>(null);
const previewCmHost = ref<HTMLDivElement | null>(null);

let skipNextRulesWatch = false;

/// 500ms 防抖预览 — vueuse useDebounceFn 自动随组件卸载清理。150/250ms 偏紧,快速勾规则时 preview IPC 还没跑完又触发下一轮,500ms 给后端留够时间。
const debouncedRunPreview = useDebounceFn(() => { void runPreview(); }, 500);

watch(selectedRules, () => {
  if (skipNextRulesWatch) {
    skipNextRulesWatch = false;
    return;
  }
  debouncedRunPreview();
}, { deep: true });

watch(open, async (v) => {
  if (v) {
    skipNextRulesWatch = true;
    selectedRules.value = initialRules.map((r) => ({ ...r }));
    preview.value = null;
    error.value = null;
    // Dialog 用 v-if 切 overlay,open=true 后才挂上 cm-host,需 nextTick 等 DOM 落地再 new EditorView
    await nextTick();
    await mountReadOnlyEditors();
    void runPreview();
  } else {
    // Dialog 关闭时 DOM 即将被 v-if 卸载,先 destroy cmView 避免引用脱离的 DOM
    destroyReadOnlyEditors();
  }
}, { immediate: true });

// 监听 props.sourceText / preview.cleaned_text 变化 -> 同步推到 cmView
watch(() => props.sourceText, (next) => { setCmText(sourceCmView, next); });
watch(() => preview.value?.cleaned_text ?? '', (next) => { setCmText(previewCmView, next); });

onUnmounted(() => {
  destroyReadOnlyEditors();
});

async function runPreview() {
  const enabled = selectedRules.value.filter((r) => r.enabled).map((r) => r.id);
  if (enabled.length === 0) {
    preview.value = { cleaned_text: props.sourceText, lines_delta: 0, chars_delta: 0 };
    error.value = null;
    return;
  }
  cleaning.value = true;
  try {
    const result = await previewCleaning(props.sourceText, enabled);
    preview.value = result;
    error.value = null;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    cleaning.value = false;
  }
}

function toggleRule(id: RuleRow['id'], checked: boolean) {
  const r = selectedRules.value.find((x) => x.id === id);
  if (r) r.enabled = checked;
}

function move(id: RuleRow['id'], delta: number) {
  const idx = selectedRules.value.findIndex((x) => x.id === id);
  const next = idx + delta;
  if (idx < 0 || next < 0 || next >= selectedRules.value.length) return;
  const arr = selectedRules.value;
  [arr[idx], arr[next]] = [arr[next], arr[idx]];
}

const canConfirm = computed(() =>
  preview.value !== null && preview.value.cleaned_text !== props.sourceText,
);

function onConfirm() {
  if (!canConfirm.value || preview.value === null) return;
  emit('confirm', preview.value.cleaned_text);
  open.value = false;
}

function onCancel() {
  open.value = false;
}

/// 构造两个只读 EditorView —— 只读意味着不需要 history / keymap(无法编辑/撤销);
/// 仍保留 drawSelection 让用户能选中对比复制,lineWrapping 让长行折行。
async function mountReadOnlyEditors(): Promise<void> {
  if (sourceCmView || previewCmView) return;  // 重复挂载防护
  const [{ EditorState }, { EditorView, drawSelection }] = await Promise.all([
    import('@codemirror/state'),
    import('@codemirror/view'),
  ]);
  const themeExt = EditorView.theme({
    '&': {
      height: '100%',
      fontSize: '12px',
      fontFamily: 'var(--font-mono), ui-monospace, monospace',
      color: 'var(--text-primary)',
      backgroundColor: 'var(--color-sheet)',
    },
    '.cm-content': { padding: '8px 10px' },
    '.cm-scroller': { fontFamily: 'inherit' },
    '.cm-gutters': { display: 'none' },
  });
  const baseExts = [
    EditorState.readOnly.of(true),
    drawSelection(),
    EditorView.lineWrapping,
    themeExt,
  ];
  if (sourceCmHost.value) {
    sourceCmView = new EditorView({
      state: EditorState.create({ doc: props.sourceText, extensions: baseExts }),
      parent: sourceCmHost.value,
    });
  }
  if (previewCmHost.value) {
    previewCmView = new EditorView({
      state: EditorState.create({
        doc: preview.value?.cleaned_text ?? '',
        extensions: baseExts,
      }),
      parent: previewCmHost.value,
    });
  }
}

function destroyReadOnlyEditors(): void {
  sourceCmView?.destroy();
  sourceCmView = null;
  previewCmView?.destroy();
  previewCmView = null;
}

function setCmText(view: EditorViewType | null, text: string): void {
  if (!view) return;
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
  });
}
</script>

<style scoped>
.body {
  display: flex;
  gap: 16px;
  height: 60vh;
}
.rules {
  width: 220px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.rules-title { margin: 0; font-size: 13px; color: var(--text-secondary); font-weight: var(--font-weight-medium); }
.rule-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 8px; }
.rule-item { display: flex; align-items: center; gap: 8px; }
.rule-item label { display: inline-flex; align-items: center; gap: 6px; flex: 1; cursor: pointer; }
.reorder { display: flex; gap: 4px; }
.reorder button { width: 24px; height: 24px; padding: 0; }
.stats { font-size: 12px; color: var(--text-secondary); margin-top: auto; }
.error { font-size: 12px; color: var(--danger); }
.previews { flex: 1; display: grid; grid-template-columns: 1fr 1fr; gap: 12px; min-height: 0; }
.pane { display: flex; flex-direction: column; min-height: 0; }
.pane-body {
  flex: 1;
  min-height: 0;
  display: flex;
  position: relative;
}
.cleaning-overlay {
  position: absolute;
  inset: 0;
  background: var(--overlay-bg);
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-pin);
  pointer-events: none;
  z-index: 1;
}
.cleaning-label {
  font-family: var(--font-serif);
  font-size: 14px;
  color: var(--text-secondary);
  letter-spacing: 0.1em;
  animation: cleaning-pulse 1.5s ease-in-out infinite;
}
@keyframes cleaning-pulse {
  0%, 100% { opacity: 0.4; }
  50% { opacity: 1; }
}
.pane-title { margin: 0 0 6px; font-size: 12px; color: var(--text-secondary); }
.cm-readonly-host {
  flex: 1;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  overflow: hidden;
  background: var(--color-sheet);
}
</style>
