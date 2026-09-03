<template>
  <Dialog v-model:open="open" :title="title" :width="560">
    <div class="row">
      <label>名称 *</label>
      <Input v-model="nameRef" :placeholder="namePlaceholder" />
    </div>
    <div class="row">
      <label>kind *</label>
      <select v-model="kindRef" class="kind-select">
        <option value="compress">{{ formatPromptKind('compress') }}</option>
        <option value="style">{{ formatPromptKind('style') }}</option>
      </select>
    </div>
    <div class="row column">
      <label>template *</label>
      <textarea
        v-model="templateRef"
        class="template-area"
        rows="14"
        spellcheck="false"
      />
    </div>
    <div v-if="missingChapterContent" class="warn">
      该 prompt 未引用 <code>{{ CHAPTER_CONTENT_PLACEHOLDER }}</code>,LLM 将无法看到章节正文
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" :disabled="!canSubmit" :loading="submitting" @click="onSubmit">
        保存
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';
import type { Prompt, PromptInput } from '../ipc/types';
import { usePromptsStore } from '../stores/prompts';
import { formatPromptKind } from '../utils/prompt-locale';

const props = defineProps<{
  mode: 'create' | 'edit' | 'copy-from-builtin';
  initial?: Prompt;
}>();

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ saved: [] }>();

const store = usePromptsStore();

/// 章节正文占位符 —— 后端 prompts::render 在填模板时按此 key 注入章节切片。
/// 提示框用同 key 检测模板是否实际引用章节正文(没有 → LLM 看不到内容,直接 fail-fast)。
const CHAPTER_CONTENT_PLACEHOLDER = '{{chapter_content}}';
const nameRef = ref('');
const kindRef = ref<'compress' | 'style'>('compress');
const templateRef = ref('');
const submitting = ref(false);
const error = ref<string | null>(null);

const title = computed(() => ({
  create: '新建 prompt',
  edit: '编辑 prompt',
  'copy-from-builtin': '复制 builtin prompt',
}[props.mode]));

const namePlaceholder = computed(() => {
  if (props.mode === 'copy-from-builtin') return '原 builtin 名称 _copy';
  return '例如:compress_v2';
});

const canSubmit = computed(
  () =>
    nameRef.value.trim() !== '' &&
    templateRef.value.trim() !== '' &&
    templateRef.value.includes(CHAPTER_CONTENT_PLACEHOLDER) &&
    !submitting.value,
);

/// 模板未含 {{chapter_content}} 时给用户的二次提示。canSubmit 已经把这条件放在硬校验里,
/// 这里只是 UI 提示文案 —— 即使关掉警告,提交按钮也会被禁,避免把"没引用章节正文"的 prompt 漏到后端。
const missingChapterContent = computed(
  () => !templateRef.value.includes(CHAPTER_CONTENT_PLACEHOLDER),
);

function blank() {
  nameRef.value = '';
  kindRef.value = 'compress';
  templateRef.value = '';
  error.value = null;
  submitting.value = false;
}

function applyInitial(value: Prompt | undefined) {
  blank();
  if (!value) return;
  nameRef.value = value.name;
  kindRef.value = value.kind;
  templateRef.value = value.template;
  if (props.mode === 'copy-from-builtin' && !value.name.endsWith('_copy')) {
    nameRef.value = `${value.name}_copy`;
  }
}

watch(() => props.initial, (v) => applyInitial(v), { immediate: true });
watch(open, (v) => {
  if (v) applyInitial(props.initial);
});

async function onSubmit() {
  if (!canSubmit.value) return;
  submitting.value = true;
  error.value = null;
  try {
    const payload: PromptInput = {
      id: props.mode === 'edit' ? (props.initial?.id ?? 0) : 0,
      name: nameRef.value.trim(),
      kind: kindRef.value,
      template: templateRef.value,
    };
    await store.upsert(payload);
    emit('saved');
    open.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.row {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}
.row.column {
  flex-direction: column;
  align-items: stretch;
}
.row label {
  width: 100px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.kind-select {
  flex: 1;
  height: 32px;
  padding: 0 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-size: 14px;
  font-family: inherit;
  outline: none;
}
.kind-select:focus { border-color: var(--border-strong); }
.template-area {
  width: 100%;
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  box-sizing: border-box;
}
.template-area:focus { border-color: var(--border-strong); }
.warn {
  margin-top: 8px;
  padding: 8px 12px;
  background: #fff8e1;
  color: #8a6d3b;
  border-radius: var(--radius-pin);
  font-size: 12px;
}
.warn code {
  background: rgba(0, 0, 0, 0.05);
  padding: 1px 4px;
  border-radius: 3px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
.error {
  margin-top: 8px;
  color: var(--color-cinnabar-deep);
  font-size: 12px;
}
</style>