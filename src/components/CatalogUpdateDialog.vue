<template>
  <Dialog v-model:open="open" :title="titleText" :width="500">
    <div v-if="phase === 'loading'" class="phase">
      <div class="spinner" aria-label="加载中" />
      <p class="phase-msg">正在拉取 <a href="#" @click.prevent="openUrl(REMOTE_URL)">{{ REMOTE_URL }}</a> …</p>
      <p class="phase-hint">需要 VPN / 直连；不通时会自动切换到拖拽模式。</p>
    </div>

    <div v-else-if="phase === 'failed'" class="phase">
      <p class="phase-msg danger">拉取失败</p>
      <pre v-if="error" class="error-text">{{ error }}</pre>
      <p class="phase-hint">
        请在浏览器访问
        <a href="#" @click.prevent="openUrl(REMOTE_URL)">models.dev/api.json</a>
        下载后，将 <code>api.json</code> 拖到下面，或点击选择文件：
      </p>
      <div
        class="dropzone"
        :class="{ over: dragOver, busy: importing }"
        @dragover.prevent="onDragOver"
        @dragleave.prevent="onDragLeave"
        @drop.prevent="onDrop"
        @click="onPickClick"
      >
        <input
          ref="fileRef"
          type="file"
          accept="application/json,.json"
          style="display: none"
          @change="onPickChange"
        />
        <div v-if="!importing" class="dropzone-empty">
          <div class="dropzone-icon">⤓</div>
          <div>拖入 <code>api.json</code> 或点击选择文件</div>
        </div>
        <div v-else class="dropzone-empty">
          <div class="spinner small" />
          <div>正在导入…</div>
        </div>
      </div>
    </div>

    <div v-else-if="phase === 'success'" class="phase">
      <p class="phase-msg success">已更新模型清单</p>
      <p v-if="successMeta" class="phase-hint mono">
        {{ formatSize(successMeta.size_bytes) }} · {{ formatTimeShort(successMeta.fetched_at) }}
      </p>
      <p class="phase-hint">下次启动仍生效（cache 存于 APPDATA）。</p>
    </div>

    <template #footer>
      <Button @click="open = false">{{ phase === 'success' ? '完成' : '关闭' }}</Button>
      <Button
        v-if="phase === 'failed'"
        kind="primary"
        :loading="importing"
        @click="retryHttp"
      >
        重试拉取
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import {
  catalogImportDrop,
  catalogRefresh,
  type CatalogMeta,
} from '../ipc/commands';
import { REMOTE_URL } from '../ipc/catalog';
import { openExternalUrl } from '../ipc/commands';
import { formatTimeShort } from '../utils/format';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  updated: [];
}>();

type Phase = 'loading' | 'failed' | 'success';

const phase = ref<Phase>('loading');
const error = ref<string | null>(null);
const successMeta = ref<CatalogMeta | null>(null);
const importing = ref(false);
const dragOver = ref(false);
const fileRef = ref<HTMLInputElement | null>(null);

const titleText = computed(() => {
  switch (phase.value) {
    case 'loading':
      return '更新模型清单';
    case 'failed':
      return '更新模型清单 · 拉取失败';
    case 'success':
      return '更新模型清单 · 完成';
  }
});

watch(
  open,
  async (v) => {
    if (v) {
      phase.value = 'loading';
      error.value = null;
      successMeta.value = null;
      await runHttpRefresh();
    }
  },
  { immediate: false },
);

async function runHttpRefresh(): Promise<void> {
  phase.value = 'loading';
  error.value = null;
  try {
    const res = await catalogRefresh();
    if (res.ok && res.meta) {
      phase.value = 'success';
      successMeta.value = res.meta;
      emit('updated');
    } else {
      phase.value = 'failed';
      error.value = res.error ?? '未知错误';
    }
  } catch (e: unknown) {
    phase.value = 'failed';
    error.value = e instanceof Error ? e.message : String(e);
  }
}

async function openUrl(url: string): Promise<void> {
  try {
    await openExternalUrl(url);
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

async function retryHttp(): Promise<void> {
  await runHttpRefresh();
}

function onDragOver(e: DragEvent): void {
  if (importing.value) return;
  dragOver.value = true;
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
}

function onDragLeave(): void {
  dragOver.value = false;
}

async function onDrop(e: DragEvent): Promise<void> {
  dragOver.value = false;
  if (importing.value) return;
  const file = e.dataTransfer?.files?.[0];
  if (!file) return;
  await importFile(file);
}

function onPickClick(): void {
  if (importing.value) return;
  fileRef.value?.click();
}

async function onPickChange(e: Event): Promise<void> {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (file) await importFile(file);
}

async function importFile(file: File): Promise<void> {
  if (!file.name.toLowerCase().endsWith('.json')) {
    error.value = '不是 JSON 文件: ' + file.name;
    return;
  }
  importing.value = true;
  error.value = null;
  try {
    const text = await file.text();
    const res = await catalogImportDrop(text);
    if (res.ok && res.meta) {
      phase.value = 'success';
      successMeta.value = res.meta;
      emit('updated');
    } else {
      error.value = res.error ?? '导入失败';
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    importing.value = false;
  }
}

function formatSize(n: number): string {
  if (n < 1024) return n + ' B';
  if (n < 1024 * 1024) return (n / 1024).toFixed(1) + ' KB';
  return (n / 1024 / 1024).toFixed(2) + ' MB';
}
</script>

<style scoped>
.phase {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 12px;
  min-height: 80px;
}
.phase-msg {
  font-family: var(--font-serif);
  font-size: 15px;
  color: var(--text-primary);
  margin: 0;
}
.phase-msg.danger { color: var(--danger); }
.phase-msg.success { color: var(--success, var(--color-cinnabar)); }
.phase-hint {
  font-size: 13px;
  color: var(--text-secondary);
  margin: 0;
  line-height: 1.6;
}
.phase-hint.mono {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-muted);
}
.phase-hint a { color: var(--primary); text-decoration: underline; }
.phase-hint code {
  font-family: var(--font-mono);
  background: var(--color-bg);
  padding: 1px 4px;
  border-radius: 3px;
}
.error-text {
  margin: 0;
  padding: 8px 12px;
  background: var(--danger-bg);
  border: 1px solid var(--danger-border);
  border-radius: var(--radius-pin);
  color: var(--danger);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 120px;
  overflow: auto;
}
.dropzone {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 110px;
  padding: 16px;
  border: 2px dashed var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--color-bg);
  cursor: pointer;
  transition: border-color 120ms, background 120ms;
  user-select: none;
}
.dropzone.over {
  border-color: var(--primary);
  background: var(--primary-bg, var(--color-bg));
}
.dropzone.busy {
  cursor: progress;
  opacity: 0.7;
}
.dropzone-empty {
  text-align: center;
  color: var(--text-muted);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}
.dropzone-icon {
  font-size: 22px;
  color: var(--text-secondary);
}
.dropzone-empty code {
  font-family: var(--font-mono);
  background: var(--color-sheet);
  padding: 1px 4px;
  border-radius: 3px;
}
.spinner {
  width: 28px;
  height: 28px;
  border: 3px solid var(--border-soft);
  border-top-color: var(--primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  align-self: center;
}
.spinner.small {
  width: 18px;
  height: 18px;
  border-width: 2px;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
