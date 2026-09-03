<template>
  <Dialog v-model:open="open" title="新建工作流" :width="880">
    <div class="dialog-grid">
      <div class="left-col">
        <div class="summary">
          已选 <strong>{{ selectedChapterIds.length }}</strong> 章
        </div>
        <div v-if="defaultMode" class="row">
          <label>默认模式</label>
          <span class="readonly">{{ formatPromptKind(defaultMode) }}</span>
        </div>
        <div class="row">
          <label>提示词模板 *</label>
          <select v-model="promptId" class="prompt-select">
            <option :value="0" disabled>
              {{ promptsLoading ? '加载中...' : (prompts.length === 0 ? '暂无 prompt（请到 Prompts 页面添加）' : '选择 prompt...') }}
            </option>
            <option v-for="p in filteredPrompts" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </div>
        <div class="row">
          <label>模型配置 *</label>
          <select v-model="modelConfigId" class="model-select">
            <option :value="0" disabled>
              {{ modelsLoading ? '加载中...' : (models.length === 0 ? '暂无 model（请到 Models 页面添加）' : '选择 model...') }}
            </option>
            <option v-for="m in models" :key="m.id" :value="m.id">{{ m.name }} ({{ m.model }})</option>
          </select>
        </div>
        <div class="row">
          <label>标签 *</label>
          <input v-model="label" placeholder="如 'v1 全量'" class="label-input" :class="{ 'has-error': labelError !== null }" @input="labelError = null" />
        </div>
        <div v-if="labelError" class="row-error">{{ labelError }}</div>
        <div class="row policy-row">
          <label>失败策略 *</label>
          <div class="policy-options">
            <label class="policy-opt" data-policy="pause_and_review" :class="{ 'is-active': onFailurePolicy === 'pause_and_review' }">
              <input type="radio" v-model="onFailurePolicy" value="pause_and_review" />
              <span class="policy-icon" aria-hidden="true"><IconPauseCircle /></span>
              <span class="policy-body">
                <strong>暂停与审阅</strong>
                <small>失败时停下来,等你决定</small>
              </span>
              <span class="policy-radio" aria-hidden="true" />
            </label>
            <label class="policy-opt" data-policy="skip_failed" :class="{ 'is-active': onFailurePolicy === 'skip_failed' }">
              <input type="radio" v-model="onFailurePolicy" value="skip_failed" />
              <span class="policy-icon" aria-hidden="true"><IconSkipForward /></span>
              <span class="policy-body">
                <strong>跳过问题章节</strong>
                <small>失败章节跳过,继续派下一章</small>
              </span>
              <span class="policy-radio" aria-hidden="true" />
            </label>
          </div>
        </div>
        <div class="hint">
          默认从转换小说继承 prompt / model;此处覆盖后仅作用于本次工作流。
        </div>
      </div>
      <div class="right-col">
        <div class="ctx-section">
          <h3 class="section-title">上下文</h3>
          <div class="ctx-toggles">
            <label class="toggle">
              <input type="checkbox" v-model="includePrev" />
              <span>带前文</span>
              <small>前文原文 + 前文转换</small>
            </label>
            <label class="toggle">
              <input type="checkbox" v-model="includeNext" />
              <span>带后文</span>
              <small>后文原文</small>
            </label>
          </div>
        </div>
        <div class="preview-pane">
          <div class="preview-header">
            预览章节
            <span v-if="previewMeta">#{{ previewMeta.idx }} · {{ previewMeta.title }} · {{ previewMeta.wordCount }} 字</span>
            <span v-else>未选章节</span>
          </div>

          <!-- 区 1：原文（只读） -->
          <label class="preview-label">原文</label>
          <textarea
            class="preview-original"
            :value="previewOriginal"
            readonly
            placeholder="(切换预览章节时自动加载)"
            rows="5"
          ></textarea>

          <!-- 区 2：预览（LLM 输出，可重生成） -->
          <label class="preview-label">
            预览
            <Button
              class="inline-gen-btn gen-preview-btn"
              kind="default"
              :loading="previewLoading"
              :disabled="!canPreview"
              @click="onGeneratePreview"
            >{{ previewOutput ? '重新生成' : '生成预览' }}</Button>
          </label>
          <textarea
            class="preview-output"
            :value="previewOutput"
            readonly
            placeholder="(点上方按钮生成)"
            rows="6"
          ></textarea>
          <div v-if="previewError" class="preview-error">{{ previewError }}</div>

          <!-- 区 3：转换结果（首章 seed，可空） -->
          <label class="preview-label">
            转换结果
            <span class="label-actions">
              <Button
                class="copy-btn"
                kind="default"
                :disabled="!previewLatest || !previewLatest.content.trim()"
                @click="onCopyFromPreview"
              >↑ 从预览复制</Button>
              <Button
                class="clear-btn"
                kind="default"
                :disabled="!seedContent.trim()"
                @click="onClearSeed"
              >清空</Button>
            </span>
          </label>
          <textarea
            class="seed-output"
            v-model="seedContent"
            @input="onSeedInput"
            placeholder="可手写 / 可点↑ 从预览复制 / 可保持空（首章走 LLM 队列）"
            rows="6"
          ></textarea>
          <div v-if="seedSource && seedContent.trim()" class="seed-source-hint">
            来源：
            <template v-if="seedSource.kind === 'llm'">LLM（消耗 {{ seedSource.tokens_in }}/{{ seedSource.tokens_out }} tokens）</template>
            <template v-else>手写（不消耗 tokens）</template>
          </div>
        </div>
      </div>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button
        class="create-btn"
        kind="primary"
        :loading="submitting"
        :disabled="!canSubmit"
        @click="onSubmit"
      >⚙ 创建</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import IconPauseCircle from '~icons/lucide/pause-circle';
import IconSkipForward from '~icons/lucide/skip-forward';
import { listModels, listPrompts, previewFirstChapter, getChapter } from '../ipc/commands';
import type { ModelConfig, Prompt, CreateWorkflowInput, FirstChapterSeedSource } from '../ipc/types';
import { formatPromptKind } from '../utils/prompt-locale';

const props = defineProps<{
  tnId: number;
  defaultPromptId?: number | null;
  defaultModelConfigId?: number | null;
  defaultMode?: 'compress' | 'style' | null;
  selectedChapterIds: number[];
  /// 预览章节 id:selectedChapterIds 中 idx 最小者的 chapter_id;父组件负责算。
  previewChapterId: number | null;
}>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  submit: [CreateWorkflowInput];
}>();

const prompts = ref<Prompt[]>([]);
const models = ref<ModelConfig[]>([]);
/// IPC 加载完成标记 —— 区分"还在加载"和"加载完但空"
const promptsLoading = ref(true);
const modelsLoading = ref(true);
const promptId = ref(0);
const modelConfigId = ref(0);
const label = ref('');
const labelError = ref<string | null>(null);
const includePrev = ref(false);
const includeNext = ref(false);
const previewLoading = ref(false);
const previewError = ref<string | null>(null);
const previewOriginal = ref('');
const previewOutput = ref('');
/// 最新一次 previewFirstChapter IPC 的返回（含 tokens_in/out）。
/// previewLatest 在生成成功时即写入；"↑ 从预览复制"按钮读取它构建 seed。
const previewLatest = ref<{ content: string; tokens_in: number; tokens_out: number } | null>(null);
const previewMeta = ref<{ idx: number; title: string; wordCount: number } | null>(null);
/// "转换结果"区双向绑定。可空 —— 用户不填时，seed=null，首章走 LLM 队列。
const seedContent = ref('');
const seedSource = ref<FirstChapterSeedSource | null>(null);
const onFailurePolicy = ref<CreateWorkflowInput['on_failure_policy']>('pause_and_review');
const submitting = ref(false);
const error = ref<string | null>(null);

const filteredPrompts = computed(() => {
  if (!props.defaultMode) return prompts.value;
  const want = props.defaultMode === 'compress' ? 'compress' : 'style';
  return prompts.value.filter((p) => p.kind === want);
});

const canSubmit = computed(() =>
  promptId.value !== 0 &&
  modelConfigId.value !== 0 &&
  label.value.trim() !== '' &&
  props.selectedChapterIds.length > 0 &&
  !submitting.value,
);

/// 「生成预览」按钮可用的条件:有预览章节 + 选了 prompt/model + 不在加载中。
const canPreview = computed(() =>
  props.previewChapterId !== null &&
  promptId.value !== 0 &&
  modelConfigId.value !== 0 &&
  !previewLoading.value,
);

watch(open, async (v) => {
  if (!v) return;
  error.value = null;
  submitting.value = false;
  promptId.value = props.defaultPromptId ?? 0;
  modelConfigId.value = props.defaultModelConfigId ?? 0;
  label.value = '';
  labelError.value = null;
  includePrev.value = false;
  includeNext.value = false;
  previewLatest.value = null;
  previewOutput.value = '';
  // previewOriginal / previewMeta 由 previewChapterId watcher 管理,不在这里 reset:
  // dialog 挂载时 watcher 已 immediate 触发拉过原文,open 切换不应清掉。
  previewError.value = null;
  seedContent.value = '';
  seedSource.value = null;
  onFailurePolicy.value = 'pause_and_review';
  promptsLoading.value = true;
  modelsLoading.value = true;
  try {
    const [pRes, mRes] = await Promise.all([listPrompts(), listModels()]);
    prompts.value = pRes;
    models.value = mRes;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    promptsLoading.value = false;
    modelsLoading.value = false;
  }
}, { immediate: true });

/// 监听 previewChapterId 变化:切换预览章节时清旧输出 + 拉新原文。
/// immediate: dialog 挂载时 sources 已经加载好,previewChapterId 已有值,需立即拉原文。
watch(() => props.previewChapterId, async (id) => {
  // 切换预览章节:丢掉旧预览结果(否则用户可能带着上一个章节的"满意"误传)
  previewLatest.value = null;
  previewOutput.value = '';
  seedContent.value = '';
  seedSource.value = null;
  previewError.value = null;
  if (id === null) {
    previewOriginal.value = '';
    previewMeta.value = null;
    return;
  }
  try {
    const ch = await getChapter(id);
    previewOriginal.value = ch.body;
    previewMeta.value = { idx: ch.idx, title: ch.title, wordCount: ch.word_count };
  } catch (e: unknown) {
    previewError.value = e instanceof Error ? e.message : String(e);
    previewOriginal.value = '';
    previewMeta.value = null;
  }
}, { immediate: true });

async function onGeneratePreview() {
  if (props.previewChapterId === null) return;
  if (promptId.value === 0 || modelConfigId.value === 0) {
    previewError.value = '请先选择 prompt 和 model';
    return;
  }
  // 重生成预览:覆盖 previewOutput;不动 seedContent(已写的内容不会被 LLM 覆盖)
  previewLatest.value = null;
  previewOutput.value = '';
  previewLoading.value = true;
  previewError.value = null;
  try {
    const out = await previewFirstChapter({
      tn_id: props.tnId,
      chapter_id: props.previewChapterId,
      prompt_id: promptId.value,
      model_config_id: modelConfigId.value,
      include_prev: includePrev.value,
      include_next: includeNext.value,
      custom_input: null,
    });
    previewLatest.value = out;
    previewOutput.value = out.content;
  } catch (e: unknown) {
    previewError.value = e instanceof Error ? e.message : String(e);
  } finally {
    previewLoading.value = false;
  }
}

/// "↑ 从预览复制"按钮。previewOutput 空时按钮禁用;
/// seedContent 已非空时弹 confirm 决定追加或替换。
function onCopyFromPreview() {
  const out = previewLatest.value;
  if (!out || !out.content.trim()) return;
  if (!seedContent.value.trim()) {
    seedContent.value = out.content;
    seedSource.value = { kind: 'llm', tokens_in: out.tokens_in, tokens_out: out.tokens_out };
    return;
  }
  const append = window.confirm(
    '转换结果区已有内容。\n确定=追加到末尾（保留现有内容）\n取消=替换当前内容',
  );
  if (append) {
    seedContent.value = seedContent.value + '\n\n' + out.content;
    // 追加: 保留原 seedSource(混合内容近似按原 source 标注)。
  } else {
    seedContent.value = out.content;
    seedSource.value = { kind: 'llm', tokens_in: out.tokens_in, tokens_out: out.tokens_out };
  }
}

function onClearSeed() {
  if (!seedContent.value.trim()) return;
  if (!window.confirm('清空转换结果区？')) return;
  seedContent.value = '';
  seedSource.value = null;
}

/// 用户手写输入 → 标记 source 为 manual（无 LLM 调用）。
/// 仅在 seedSource 仍为 null 时设置，避免覆盖已通过 "↑ 复制" 选中的 LLM 标签。
function onSeedInput() {
  if (!seedSource.value && seedContent.value.trim()) {
    seedSource.value = { kind: 'manual' };
  }
}

async function onSubmit() {
  if (!canSubmit.value) {
    if (label.value.trim() === '') labelError.value = '请填写标签';
    return;
  }
  // mode 由所选 prompt 的 kind 决定(后端 create_workflow 会再次校验)。
  const selectedPrompt = prompts.value.find((p) => p.id === promptId.value);
  const mode = selectedPrompt?.kind;
  if (mode !== 'compress' && mode !== 'style') {
    error.value = '请选择一个 prompt 以确定 mode。';
    return;
  }
  submitting.value = true;
  error.value = null;
  try {
    emit('submit', {
      tn_id: props.tnId,
      label: label.value.trim(),
      chapter_ids: [...props.selectedChapterIds],
      prompt_id: promptId.value,
      model_config_id: modelConfigId.value,
      mode,
      ctx_prev_original: includePrev.value ? 1 : 0,
      ctx_prev_transformed: includePrev.value ? 1 : 0,
      ctx_next_original: includeNext.value ? 1 : 0,
      on_failure_policy: onFailurePolicy.value,
      // 构造 FirstChapterSeed: seedContent 空 → null（首章走 LLM 队列）;
//   非空 → { content, source }。
      preview_first_chapter: seedContent.value.trim()
        ? {
            content: seedContent.value.trim(),
            source: seedSource.value ?? { kind: 'manual' },
          }
        : null,
    });
    open.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.summary {
  margin-bottom: 12px;
  padding: 10px 14px;
  background: var(--bg-section);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  font-size: 14px;
  color: var(--text-secondary);
}
.summary strong {
  color: var(--color-cinnabar);
  font-size: 16px;
  font-family: var(--font-mono);
  margin: 0 4px;
}
.row { display: flex; align-items: center; margin-bottom: 12px; gap: 12px; }
.row > label { width: 100px; font-size: 14px; color: var(--text-secondary); flex-shrink: 0; }
.row select, .row input { flex: 1; height: 32px; }
.label-input {
  padding: 0 8px; border: 1px solid var(--border-color); border-radius: var(--radius-pin);
  background: var(--color-sheet); color: var(--text-primary);
}
.label-input.has-error { border-color: var(--danger); }
.row-error { color: var(--danger); font-size: 12px; margin: -8px 0 12px 112px; }
.error { color: var(--danger); font-size: 12px; margin-top: 8px; }
.hint { color: var(--text-muted); font-size: 12px; margin-top: 8px; line-height: 1.5; }
.dialog-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  min-height: 460px;
}
.left-col, .right-col {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.right-col .summary { margin-bottom: 0; }
.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 4px 0;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border-soft);
}
.ctx-toggles { display: flex; flex-direction: column; gap: 8px; }
.toggle {
  display: flex; flex-direction: column; gap: 2px; cursor: pointer;
  padding: 8px 12px;
  border: 1px solid var(--border-soft); border-radius: var(--radius-pin);
  background: var(--color-sheet);
  transition: border-color 0.15s, background 0.15s;
}
.toggle:hover { border-color: var(--border-color); background: var(--bg-hover); }
.toggle:has(input:checked) {
  border-color: var(--color-cinnabar);
  background: var(--color-cinnabar-light);
}
.toggle > span { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.toggle > small { font-size: 11px; color: var(--text-muted); }
.toggle input[type='checkbox'] {
  position: absolute; opacity: 0; pointer-events: none; width: 0; height: 0;
}
.preview-pane {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-height: 0;
}
.preview-header {
  font-size: 12px;
  color: var(--text-muted);
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border-soft);
}
.preview-header > span {
  margin-left: 6px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
}
.preview-label {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 11px;
  color: var(--text-muted);
}
.label-actions {
  display: flex;
  gap: 4px;
}
.inline-gen-btn {
  font-size: 12px;
  padding: 2px 8px;
}
.preview-original,
.preview-output,
.seed-output {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.55;
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--bg-section);
  color: var(--text-primary);
  resize: vertical;
  min-height: 60px;
  width: 100%;
  box-sizing: border-box;
}
.preview-original { max-height: 180px; font-family: var(--font-mono); }
.preview-output { flex: 1; min-height: 140px; font-family: var(--font-mono); }
.seed-output {
  font-family: var(--font-serif);
  font-size: 13px;
}
.seed-source-hint {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 4px;
}
.preview-error {
  color: var(--danger);
  font-size: 12px;
  padding: 6px 8px;
  background: var(--color-cinnabar-light);
  border-radius: var(--radius-pin);
}
.row.policy-row { align-items: flex-start; }
.policy-options { display: flex; flex-direction: column; gap: 8px; flex: 1; }
.policy-opt {
  display: flex; align-items: center; gap: 12px; cursor: pointer;
  font-size: 13px; padding: 10px 14px;
  border: 1px solid var(--border-soft); border-radius: var(--radius-pin);
  background: var(--color-sheet);
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
}
.policy-opt:hover {
  border-color: var(--border-color);
  background: var(--bg-hover);
}
.policy-opt input[type='radio'] {
  position: absolute; opacity: 0; pointer-events: none; width: 0; height: 0;
}
.policy-icon {
  display: inline-flex; align-items: center; justify-content: center;
  width: 28px; height: 28px;
  border-radius: 50%;
  background: var(--bg-section);
  color: var(--text-muted);
  flex-shrink: 0;
  transition: background 0.15s ease, color 0.15s ease;
}
.policy-icon svg { width: 16px; height: 16px; }
.policy-body { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
.policy-body strong { color: var(--text-primary); font-weight: 600; line-height: 1.3; }
.policy-body small { color: var(--text-muted); font-size: 11px; line-height: 1.4; }
.policy-radio {
  width: 16px; height: 16px; border-radius: 50%;
  border: 2px solid var(--border-color);
  flex-shrink: 0;
  position: relative;
  box-sizing: border-box;
  transition: border-color 0.15s ease;
}
.policy-opt.is-active {
  border-color: var(--color-cinnabar);
  background: var(--color-cinnabar-light);
  box-shadow: 0 0 0 1px var(--color-cinnabar) inset;
}
.policy-opt.is-active .policy-radio { border-color: var(--color-cinnabar); }
.policy-opt.is-active .policy-radio::after {
  content: ''; position: absolute;
  top: 2px; left: 2px;
  width: 8px; height: 8px; border-radius: 50%;
  background: var(--color-cinnabar);
}
.policy-opt.is-active .policy-icon { background: var(--color-sheet); }
.policy-opt.is-active[data-policy='pause_and_review'] .policy-icon { color: var(--color-tangerine); }
.policy-opt.is-active[data-policy='skip_failed'] .policy-icon { color: var(--color-vivid-green); }
</style>
