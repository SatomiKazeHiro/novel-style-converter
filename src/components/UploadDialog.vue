<template>
  <Dialog v-model:open="open" title="上传 .txt 文件" :width="500">
    <div
      class="file-slot"
      :class="{ filled: !!fileInfo }"
      role="button"
      tabindex="0"
      @click="onPick"
      @keydown.enter.prevent="onPick"
      @keydown.space.prevent="onPick"
    >
      <template v-if="!fileInfo">
        <IconFileText :size="36" :stroke-width="1.5" class="slot-icon" />
        <div class="slot-title">点击选择 .txt 文件</div>
        <div class="slot-sub">导入后可解析为章节、生成数据资产</div>
      </template>
      <template v-else>
        <IconFileCheck2 :size="28" :stroke-width="1.6" class="slot-icon filled" />
        <div class="slot-info">
          <div class="slot-name">{{ filename }}</div>
          <div class="slot-path" :title="filePath">{{ filePath }}</div>
        </div>
        <button type="button" class="slot-replace" @click.stop="onPick">更换</button>
      </template>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button :disabled="submitting" @click="open = false">取消</Button>
      <Button kind="primary" :loading="submitting" :disabled="!canSubmit || submitting" @click="onSubmit">上传</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import IconFileText from '~icons/lucide/file-text';
import IconFileCheck2 from '~icons/lucide/file-check-2';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [{ filePath: string; filename: string }] }>();

const filePath = ref('');
const filename = ref('');
const error = ref<string | null>(null);
const submitting = ref(false);
const picking = ref(false);

const fileInfo = computed(() => (filePath.value ? { name: filename.value, path: filePath.value } : null));
const canSubmit = computed(() => filePath.value !== '');

watch(open, (v) => {
  if (v) {
    filePath.value = '';
    filename.value = '';
    error.value = null;
    submitting.value = false;
    picking.value = false;
  }
});

async function onPick() {
  if (picking.value) return;
  error.value = null;
  picking.value = true;
  try {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: 'Text', extensions: ['txt'] }],
    });
    if (typeof selected === 'string') {
      filePath.value = selected;
      const segs = selected.split(/[\\/]/);
      filename.value = segs[segs.length - 1] || 'uploaded.txt';
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    picking.value = false;
  }
}

function onSubmit() {
  if (!canSubmit.value) return;
  error.value = null;
  submitting.value = true;
  try {
    emit('submit', { filePath: filePath.value, filename: filename.value });
    open.value = false;
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.file-slot {
  border: 1.5px solid var(--border-soft);
  border-radius: var(--radius-card);
  background: var(--bg-hover);
  padding: 32px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  cursor: pointer;
  transition: border-color 120ms ease, background 120ms ease;
  user-select: none;
  outline: none;
}
.file-slot:hover,
.file-slot:focus-visible {
  border-color: var(--border-color);
}
.file-slot.filled {
  flex-direction: row;
  align-items: center;
  justify-content: flex-start;
  gap: 14px;
  padding: 14px 16px;
  background: var(--accent-bg);
  border-color: var(--accent);
  cursor: default;
}
.slot-icon {
  color: var(--text-secondary);
  flex-shrink: 0;
}
.slot-icon.filled {
  color: var(--accent);
}
.slot-title {
  font-size: 15px;
  font-weight: 500;
  color: var(--text-primary);
}
.slot-sub {
  font-size: 12px;
  color: var(--text-muted);
}
.slot-info {
  flex: 1 1 auto;
  min-width: 0;
}
.slot-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.slot-path {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.slot-replace {
  background: none;
  border: 1px solid var(--accent);
  border-radius: var(--radius-pin);
  font-size: 12px;
  color: var(--accent);
  cursor: pointer;
  padding: 4px 10px;
  font-family: inherit;
  flex-shrink: 0;
  transition: background 120ms ease, color 120ms ease;
}
.slot-replace:hover {
  background: var(--accent);
  color: #fff;
}
.error {
  margin-top: 10px;
  color: var(--danger);
  font-size: 12px;
}
</style>