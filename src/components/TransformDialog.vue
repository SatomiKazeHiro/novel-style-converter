<template>
  <Dialog v-model:open="open" title="重新转换" :width="520">
    <div class="row">
      <label>tn *</label>
      <select v-model="tnId" class="tn-select">
        <option :value="0" disabled>选择 tn...</option>
        <option v-for="tn in tns" :key="tn.id" :value="tn.id">{{ tn.title }} ({{ tn.chapters_count }} 章)</option>
      </select>
    </div>
    <div class="row">
      <label>model *</label>
      <select v-model="modelConfigId" class="model-select">
        <option :value="0" disabled>选择 model...</option>
        <option v-for="m in models" :key="m.id" :value="m.id">{{ m.name }} ({{ m.model }})</option>
      </select>
    </div>
    <div class="row">
      <label>prompt</label>
      <span class="readonly">#{{ promptId || '?' }} (继承自上次转换)</span>
    </div>
    <div class="mode-info">支持模式: {{ formatPromptKind('compress') }} / {{ formatPromptKind('style') }}</div>
    <div class="row ctx">
      <div>
        <label>前文原文</label>
        <NumberInput v-model="ctxPrevOriginal" :min="0" :max="20" class="ctx-prev-original" />
      </div>
      <div>
        <label>前文转换</label>
        <NumberInput v-model="ctxPrevTransformed" :min="0" :max="20" class="ctx-prev-transformed" />
      </div>
      <div>
        <label>后文原文</label>
        <NumberInput v-model="ctxNextOriginal" :min="0" :max="20" class="ctx-next-original" />
      </div>
    </div>
    <div class="ctx-hint">
      给 LLM 的上下文窗口大小（章）。一般只设"前文转换" 1~3,
      让模型参考前面已经转换好的章节学文风;原文带多了浪费 token。
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <div v-if="!promptId" class="hint">该章节还没有历史转换,无法确定 prompt。请先在 Library 转换 tab 用「新建转换」完成首次转换。</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button
        kind="primary"
        class="submit"
        :loading="submitting"
        :disabled="!canSubmit"
        @click="onSubmit"
      >⚙ 提交</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import NumberInput from './ui/NumberInput.vue';
import {
  listTransformationNovels as ipcListTns,
  listModels as ipcListModels,
  enqueueTransformationChapters as ipcEnqueueTns,
} from '../ipc/commands';
import type { TransformationNovelSummary, ModelConfig } from '../ipc/types';
import { formatPromptKind } from '../utils/prompt-locale';

const props = defineProps<{
  dataAssetId: number;
  chapterId: number;
  defaultPromptId?: number;
  defaultModelConfigId?: number;
  defaultCtxPrevOriginal?: number;
  defaultCtxPrevTransformed?: number;
  defaultCtxNextOriginal?: number;
}>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submitted: [number[]] }>();

const tns = ref<TransformationNovelSummary[]>([]);
const models = ref<ModelConfig[]>([]);
const tnId = ref(0);
const modelConfigId = ref(0);
const promptId = ref(props.defaultPromptId ?? 0);
const ctxPrevOriginal = ref<number | null>(props.defaultCtxPrevOriginal ?? 0);
const ctxPrevTransformed = ref<number | null>(props.defaultCtxPrevTransformed ?? 0);
const ctxNextOriginal = ref<number | null>(props.defaultCtxNextOriginal ?? 0);
const submitting = ref(false);
const error = ref<string | null>(null);

const canSubmit = computed(() =>
  tnId.value !== 0 && modelConfigId.value !== 0 && promptId.value !== 0 &&
  ctxPrevOriginal.value !== null && ctxPrevTransformed.value !== null && ctxNextOriginal.value !== null &&
  !submitting.value,
);

watch(open, async (v) => {
  if (!v) return;
  error.value = null;
  submitting.value = false;
  tnId.value = 0;
  modelConfigId.value = props.defaultModelConfigId ?? 0;
  promptId.value = props.defaultPromptId ?? 0;
  ctxPrevOriginal.value = props.defaultCtxPrevOriginal ?? 0;
  ctxPrevTransformed.value = props.defaultCtxPrevTransformed ?? 0;
  ctxNextOriginal.value = props.defaultCtxNextOriginal ?? 0;
  try {
    const [tnsRes, modelsRes] = await Promise.all([
      ipcListTns(props.dataAssetId),
      ipcListModels(),
    ]);
    tns.value = tnsRes;
    models.value = modelsRes;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}, { immediate: true });

async function onSubmit() {
  if (!canSubmit.value) return;
  submitting.value = true;
  error.value = null;
  try {
    const ids = await ipcEnqueueTns({
      transformation_novel_id: tnId.value,
      chapter_ids: [props.chapterId],
      prompt_id: promptId.value,
      model_config_id: modelConfigId.value,
      ctx_prev_original: ctxPrevOriginal.value ?? 0,
      ctx_prev_transformed: ctxPrevTransformed.value ?? 0,
      ctx_next_original: ctxNextOriginal.value ?? 0,
    });
    emit('submitted', ids);
    open.value = false;
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.row { display: flex; align-items: center; margin-bottom: 12px; gap: 12px; }
.row > label { width: 90px; font-size: 14px; color: var(--text-secondary); flex-shrink: 0; }
.row select { flex: 1; height: 32px; }
.readonly { font-size: 13px; color: var(--text-secondary); font-variant-numeric: tabular-nums; }
.row.ctx { gap: 16px; }
.row.ctx > div { flex: 1; display: flex; flex-direction: column; gap: 4px; }
.row.ctx label { width: auto; font-size: 12px; color: var(--text-muted); }
.error { color: var(--danger); font-size: 12px; margin-top: 8px; }
.hint { color: var(--text-muted); font-size: 12px; margin-top: 8px; }
.ctx-hint { color: var(--text-muted); font-size: 11px; margin-top: -4px; margin-bottom: 12px; line-height: 1.5; }
.mode-info { color: var(--text-muted); font-size: 12px; margin-top: -4px; margin-bottom: 12px; }
</style>