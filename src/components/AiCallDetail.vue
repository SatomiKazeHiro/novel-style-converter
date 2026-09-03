<template>
  <Dialog :open="open" title="AI 调用详情" :width="780" @update:open="onClose">
    <div v-if="loading" class="loading">加载中...</div>
    <div v-else-if="error" class="alert">{{ error }}</div>
    <div v-else-if="log" class="detail">
      <section class="block">
        <h3>基本信息</h3>
        <div class="kv"><span class="k">id</span><span class="v">{{ log.id }}</span></div>
        <div class="kv"><span class="k">时间</span><span class="v mono">{{ formatTime(log.created_at) }}</span></div>
        <div class="kv"><span class="k">业务</span><span class="v">{{ businessLabel(log.business) }}</span></div>
        <div class="kv"><span class="k">状态</span><span class="v">
          <Tag :kind="log.status === 'success' ? 'success' : 'danger'">
            {{ log.status === 'success' ? '成功' : '失败' }}
          </Tag>
        </span></div>
        <div class="kv"><span class="k">延迟</span><span class="v">{{ formatLatency(log.latency_ms) }}</span></div>
        <div class="kv"><span class="k">context</span><span class="v mono">
          {{ log.context_type ?? "—" }}{{ log.context_id ? `#${log.context_id}` : "" }}
        </span></div>
      </section>

      <section class="block">
        <h3>模型配置</h3>
        <div class="kv"><span class="k">model_config_id</span><span class="v">{{ log.model_config_id ?? "—" }}</span></div>
        <div class="kv"><span class="k">model</span><span class="v mono">{{ log.model_name }}</span></div>
        <div class="kv"><span class="k">base_url</span><span class="v mono">{{ log.base_url }}</span></div>
        <div class="kv"><span class="k">temperature</span><span class="v">{{ log.temperature ?? "—" }}</span></div>
        <div class="kv"><span class="k">max_tokens</span><span class="v">{{ log.max_tokens ?? "—" }}</span></div>
      </section>

      <section class="block">
        <h3>Tokens</h3>
        <div class="kv"><span class="k">in 粗估</span><span class="v">{{ log.estimated_tokens_in ?? "—" }}</span></div>
        <div class="kv"><span class="k">in 实际</span><span class="v">{{ log.actual_tokens_in ?? "—" }}</span></div>
        <div class="kv"><span class="k">out 实际</span><span class="v">{{ log.actual_tokens_out ?? "—" }}</span></div>
      </section>

      <section class="block">
        <h3>system 预览 ({{ log.system_size.toLocaleString() }} 字符)</h3>
        <pre v-if="log.system_preview" class="preview">{{ log.system_preview }}</pre>
        <p v-else class="muted">(空)</p>
      </section>

      <section class="block">
        <h3>user 预览 ({{ log.user_size.toLocaleString() }} 字符)</h3>
        <pre v-if="log.user_preview" class="preview">{{ log.user_preview }}</pre>
        <p v-else class="muted">(空)</p>
      </section>

      <section class="block">
        <h3>response 预览 ({{ log.response_size.toLocaleString() }} 字符)</h3>
        <pre v-if="log.response_preview" class="preview">{{ log.response_preview }}</pre>
        <p v-else class="muted">(空)</p>
      </section>

      <section v-if="log.error" class="block">
        <h3>错误</h3>
        <pre class="preview error-preview">{{ log.error }}</pre>
      </section>
    </div>
    <template #footer>
      <Button @click="onClose">关闭</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { formatTime } from '../utils/format';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Tag from './ui/Tag.vue';
import { getAiCallLog } from '../ipc/commands';
import type { AiCallBusiness, AiCallLog } from '../ipc/types';

const props = defineProps<{ id: number }>();
const emit = defineEmits<{ close: [] }>();

/// Dialog 的 open 由本组件本地维护 —— 父组件用 v-if="detailId !== null" 控制挂载,
/// 关闭时 emit close 通知父组件把 detailId 置空。
const open = ref(true);
const log = ref<AiCallLog | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);

async function fetchLog(id: number) {
  loading.value = true;
  error.value = null;
  try {
    log.value = await getAiCallLog(id);
    if (!log.value) {
      error.value = `未找到 id=${id} 的记录`;
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.id,
  (id) => {
    if (id > 0) void fetchLog(id);
  },
  { immediate: true },
);

function onClose() {
  open.value = false;
  emit('close');
}


function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

function businessLabel(b: AiCallBusiness): string {
  return b === 'transform_chapter'
    ? 'transform_chapter (章节转换)'
    : b === 'regenerate_preview'
    ? 'regenerate_preview (试运行预览)'
    : 'test_model (模型测试)';
}
</script>

<style scoped>
.loading { padding: 32px; text-align: center; color: var(--text-muted); }
.alert {
  padding: 12px 16px;
  background: var(--danger-bg);
  color: var(--danger);
  border-radius: var(--radius-pin);
  border: 1px solid var(--danger-border);
}
.detail { display: flex; flex-direction: column; gap: 18px; max-height: 70vh; overflow-y: auto; padding-right: 4px; }
.block h3 {
  margin: 0 0 10px;
  font-size: 13px;
  font-weight: var(--font-weight-medium);
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.kv {
  display: grid;
  grid-template-columns: 140px 1fr;
  gap: 12px;
  font-size: 13px;
  padding: 4px 0;
  align-items: center;
}
.kv .k { color: var(--text-muted); }
.kv .v { color: var(--text-primary); word-break: break-all; }
.kv .mono { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
.preview {
  margin: 0;
  padding: 10px 12px;
  background: var(--bg-section);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 320px;
  overflow-y: auto;
}
.error-preview {
  color: var(--danger);
  border-color: var(--danger-border);
  background: var(--danger-bg);
}
.muted { color: var(--text-muted); font-size: 13px; }
</style>