<template>
  <Dialog v-model:open="open" :title="editing ? '编辑模型' : '新增模型'" :width="560">
    <div v-if="!editing" class="catalog-picker">
      <button class="picker-toggle" type="button" @click="pickerOpen = !pickerOpen">
        <span>{{ pickerOpen ? '▾' : '▸' }} 从模型清单选择</span>
        <span class="picker-state" :class="{ ok: catalogReady, busy: catalogLoading, err: catalogError }">
          {{ catalogReady ? '已加载' : catalogLoading ? '加载中…' : '点击加载' }}
        </span>
      </button>
      <div v-if="pickerOpen" class="picker-body">
        <div class="picker-row">
          <label>服务商</label>
          <select v-model="pickerProvider" class="picker-select">
            <option value="">— 选择 —</option>
            <option v-for="p in catalog.providerList.value" :key="p.id" :value="p.id">
              {{ p.name }}{{ p.hasApi ? '  · ' + p.api : '' }}  ({{ p.modelCount }})
            </option>
          </select>
        </div>
        <div class="picker-row">
          <label>模型</label>
          <select v-model="pickerModel" class="picker-select" :disabled="!pickerProvider">
            <option value="">— 选择 —</option>
            <option v-for="m in pickerModelList" :key="m.id" :value="m.id">
              {{ m.name }}
            </option>
          </select>
        </div>
        <div class="picker-actions">
          <Button size="small" :disabled="!pickerModel" @click="applyCatalogSelection">
            应用到下方表单
          </Button>
          <span v-if="pickerError" class="picker-err">{{ pickerError }}</span>
        </div>
      </div>
    </div>

    <div class="row">
      <label>名称 *</label>
      <Input v-model="form.name" />
    </div>
    <div class="row">
      <label>Base URL *</label>
      <Input v-model="form.base_url" placeholder="https://api.openai.com/v1" />
    </div>
    <div class="row">
      <label>API Key *</label>
      <Input v-model="form.api_key" type="password" />
    </div>
    <div class="row">
      <label>模型名 *</label>
      <Input v-model="form.model" placeholder="gpt-4" />
    </div>
    <div class="row">
      <label>max_tokens</label>
      <NumberInput v-model="maxTokensRef" :min="0" :step="256" />
    </div>
    <div class="row">
      <label>max_context</label>
      <NumberInput v-model="maxContextRef" :min="0" :step="1024" placeholder="留空 = 不强制" />
    </div>
    <div class="row">
      <label>temperature</label>
      <NumberInput v-model="temperatureRef" :min="0" :max="2" :step="0.1" />
    </div>
    <div class="row">
      <label>并发数</label>
      <NumberInput v-model="form.concurrency" :min="1" :max="16" />
    </div>
    <div class="row hint-row">
      <span class="concurrency-hint">同时向此模型发请求的上限。例如 =3 表示最多 3 个章节同时转换,其余排队等待。worker 全局默认 2 个任务槽。</span>
    </div>
    <div class="row actions">
      <Button :loading="store.testing" @click="onTest">测试连接</Button>
      <Button size="small" @click="onClearReport" v-if="testReport">清空结果</Button>
    </div>
    <div v-if="testReport" class="report" :class="{ ok: !testReport.error, fail: !!testReport.error }">
      <div class="report-head">
        <Tag :kind="testReport.error ? 'danger' : 'success'">
          {{ testReport.error ? '失败' : '成功' }}
        </Tag>
        <span class="metric"><strong>{{ testReport.latency_ms }}</strong> ms</span>
        <span class="metric" v-if="testReport.tokens_in != null">
          tokens in <strong>{{ testReport.tokens_in }}</strong>
        </span>
        <span class="metric" v-if="testReport.tokens_out != null">
          tokens out <strong>{{ testReport.tokens_out }}</strong>
        </span>
      </div>
      <pre v-if="testReport.content_preview" class="preview">{{ testReport.content_preview }}</pre>
      <pre v-if="testReport.error" class="error-text">{{ testReport.error }}</pre>
    </div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" :disabled="!canSubmit" @click="onSubmit">
        保存
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';
import NumberInput from './ui/NumberInput.vue';
import Tag from './ui/Tag.vue';
import { useModelsStore } from '../stores/models';
import { useCatalog } from '../composables/useCatalog';
import type { ModelConfigInput, TestModelReport } from '../ipc/types';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [ModelConfigInput] }>();
const props = defineProps<{ initial: ModelConfigInput | null }>();

const store = useModelsStore();
const catalog = useCatalog();
const editing = computed(() => (props.initial?.id ?? 0) > 0);
const form = reactive<ModelConfigInput>(blank());
const maxTokensRef = ref<number | null>(null);
const maxContextRef = ref<number | null>(null);
const temperatureRef = ref<number | null>(null);
const testReport = ref<TestModelReport | null>(null);

function blank(): ModelConfigInput {
  return {
    id: 0,
    name: '',
    base_url: 'https://api.openai.com/v1',
    api_key: '',
    model: '',
    max_tokens: null,
    max_context: null,
    temperature: null,
    disable_thinking: false,
    concurrency: 1,
  };
}

function applyInitial(value: ModelConfigInput | null) {
  if (!value) {
    Object.assign(form, blank());
    maxTokensRef.value = null;
    maxContextRef.value = null;
    temperatureRef.value = null;
    testReport.value = null;
    return;
  }
  Object.assign(form, value);
  maxTokensRef.value = value.max_tokens ?? null;
  maxContextRef.value = value.max_context ?? null;
  temperatureRef.value = value.temperature ?? null;
  testReport.value = null;
}

watch(() => props.initial, applyInitial, { immediate: true });
watch(open, (v) => {
  if (v) applyInitial(props.initial);
});

const pickerOpen = ref(false);
const pickerProvider = ref('');
const pickerModel = ref('');
const pickerError = ref<string | null>(null);

const catalogReady = computed(() => catalog.data.value !== null);
const catalogLoading = catalog.loading;
const catalogError = catalog.error;

const pickerModelList = computed(() =>
  pickerProvider.value ? catalog.modelList(pickerProvider.value) : [],
);

watch(pickerOpen, async (v) => {
  if (!v) return;
  if (!catalogReady.value && !catalogLoading.value) {
    try {
      await catalog.load();
    } catch {
      // catalog.error 已经有信息了,无需再设
    }
  }
});

watch(pickerProvider, () => {
  pickerModel.value = '';
});

function applyCatalogSelection(): void {
  pickerError.value = null;
  if (!pickerProvider.value || !pickerModel.value) {
    pickerError.value = '请先选择服务商和模型';
    return;
  }
  const p = catalog.getProvider(pickerProvider.value);
  const m = catalog.getModel(pickerProvider.value, pickerModel.value);
  if (!p || !m) {
    pickerError.value = '清单里找不到该条目,请刷新模型清单';
    return;
  }
  if (p.api) form.base_url = p.api;
  form.model = m.id;
  if (!form.name.trim()) {
    form.name = p.name + ' / ' + m.name;
  }
  if (m.limit?.output != null) {
    maxTokensRef.value = m.limit.output;
  }
  if (m.limit?.context != null) {
    maxContextRef.value = m.limit.context;
  }
  if (m.temperature === true) {
    temperatureRef.value = 0.7;
  } else if (m.temperature === false) {
    temperatureRef.value = 0;
  }
}



const canSubmit = computed(
  () =>
    form.name.trim() !== '' &&
    form.base_url.trim() !== '' &&
    form.api_key.trim() !== '' &&
    form.model.trim() !== '',
);

async function onTest() {
  testReport.value = null;
  try {
    testReport.value = await store.test({
      ...form,
      max_tokens: maxTokensRef.value,
      max_context: maxContextRef.value,
      temperature: temperatureRef.value,
    });
  } catch (e) {
    testReport.value = {
      model: form.model,
      base_url: form.base_url,
      latency_ms: 0,
      tokens_in: null,
      tokens_out: null,
      content_preview: null,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

function onClearReport() {
  testReport.value = null;
}

function onSubmit() {
  if (!canSubmit.value) return;
  emit('submit', {
    ...form,
    max_tokens: maxTokensRef.value,
    max_context: maxContextRef.value,
    temperature: temperatureRef.value,
  });
  open.value = false;
}
</script>

<style scoped>
.catalog-picker {
  margin-bottom: 14px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--color-bg);
  overflow: hidden;
}
.picker-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 10px 14px;
  background: transparent;
  border: none;
  cursor: pointer;
  font-family: var(--font-serif);
  font-size: 14px;
  color: var(--text-primary);
  text-align: left;
}
.picker-toggle:hover {
  background: var(--color-sheet);
}
.picker-state {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-muted);
  padding: 1px 8px;
  border-radius: 3px;
  background: var(--color-sheet);
}
.picker-state.ok { color: var(--success, var(--color-cinnabar)); }
.picker-state.busy { color: var(--text-secondary); }
.picker-state.err { color: var(--danger); }
.picker-body {
  padding: 12px 14px 14px;
  border-top: 1px solid var(--border-soft);
  background: var(--color-sheet);
}
.picker-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 10px;
}
.picker-row label {
  width: 70px;
  font-size: 13px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.picker-select {
  flex: 1;
  height: 32px;
  padding: 4px 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-bg);
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--text-primary);
  outline: none;
}
.picker-select:focus { border-color: var(--color-cinnabar); }
.picker-select:disabled { opacity: 0.5; cursor: not-allowed; }
.picker-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 4px;
}
.picker-err {
  font-size: 12px;
  color: var(--danger);
}
.row {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}
.row label {
  width: 100px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.row.actions {
  margin-top: 16px;
}
.thinking-row {
  margin-top: -4px;
  margin-bottom: 12px;
}
.thinking-toggle {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
}
.thinking-toggle code {
  font-family: var(--font-mono);
  font-size: 12px;
  background: var(--color-bg);
  padding: 1px 5px;
  border-radius: 3px;
}
.hint-row {
  margin-top: -8px;
  margin-bottom: 12px;
}
.concurrency-hint {
  margin-left: 112px;
  font-size: 11px;
  color: var(--text-muted);
  font-style: italic;
}
.report {
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: var(--radius-pin);
  border: 1px solid var(--border-soft);
  background: var(--color-sheet);
  font-family: var(--font-mono);
  font-size: 12px;
}
.report.ok {
  border-color: var(--success-border, var(--border-soft));
  background: var(--success-bg, var(--color-sheet));
}
.report.fail {
  border-color: var(--danger-border);
  background: var(--danger-bg);
}
.report-head {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.metric {
  color: var(--text-secondary);
}
.metric strong {
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-weight: 600;
  margin: 0 2px;
}
.preview {
  margin: 8px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--text-primary);
  max-height: 160px;
  overflow: auto;
}
.error-text {
  margin: 8px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--danger);
  max-height: 200px;
  overflow: auto;
}
</style>
