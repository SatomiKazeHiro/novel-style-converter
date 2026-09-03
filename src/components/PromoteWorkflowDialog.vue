<template>
  <Dialog v-model:open="open" title="转为数据资产" :width="520">
    <div class="promote-form">
      <div class="row">
        <label>数据资产标题 <span class="required">*</span></label>
        <input
          v-model="title"
          type="text"
          placeholder="输入标题"
          :disabled="submitting"
          class="title-input"
        />
      </div>
      <div class="summary">
        <div class="summary-row">
          <span class="dot dot-success"></span>
          <span><strong>{{ successCount }}</strong> 章将使用转换结果</span>
        </div>
        <div class="summary-row">
          <span class="dot dot-original"></span>
          <span><strong>{{ failCount }}</strong> 章失败,将使用原文</span>
        </div>
        <div class="summary-row">
          <span class="dot dot-original"></span>
          <span><strong>{{ skipCount }}</strong> 章被跳过,将使用原文</span>
        </div>
      </div>
      <div v-if="externalError" class="error-banner">{{ externalError }}</div>
      <div v-else-if="error" class="error">{{ error }}</div>
      <div class="hint">
        转正后会生成一份新的 <code>promoted</code> 数据资产，与源数据资产互相独立。
        多次转正可保留不同版本。
      </div>
    </div>
    <template #footer>
      <Button size="small" :disabled="submitting" @click="open = false">取消</Button>
      <Button
        size="small"
        kind="primary"
        :disabled="!title.trim()"
        :loading="submitting"
        @click="onConfirm"
      >确认转正</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';

const props = defineProps<{
  workflowLabel: string;
  sourceDataAssetTitle: string;
  successCount: number;
  failCount: number;
  skipCount: number;
  /// 父组件传入的外部错误(后端报错)。区别于 dialog 内部 error ref —— 内部
  /// error 是 onConfirm 同步校验失败用,持续时间短;外部 error 是异步 IPC 失败,
  /// 需要稳定显示直到父组件清除(bug B: confirmPromote 把错误塞进 promoteError
  /// 但从没传给 dialog,被静默吞掉)。
  externalError?: string | null;
}>();

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  confirm: [string];
}>();

const title = ref('');
const submitting = ref(false);
const error = ref<string | null>(null);

watch(open, (o) => {
  if (o) {
    const label = props.workflowLabel || '工作流';
    title.value = `${props.sourceDataAssetTitle} - ${label}`;
    error.value = null;
    submitting.value = false;
  }
});

async function onConfirm() {
  const t = title.value.trim();
  if (!t) return;
  submitting.value = true;
  error.value = null;
  try {
    emit('confirm', t);
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.promote-form {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.row label {
  font-size: 13px;
  font-weight: 500;
  color: var(--c-text, #222);
}
.required {
  color: var(--c-danger, #d32f2f);
}
.title-input {
  padding: 6px 10px;
  border: 1px solid var(--c-border, #ccc);
  border-radius: 4px;
  font-size: 14px;
  background: var(--c-bg, #fff);
  color: var(--c-text, #222);
}
.title-input:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.summary {
  background: var(--c-bg-soft, #f5f5f5);
  border: 1px solid var(--c-border, #e0e0e0);
  border-radius: 4px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.summary-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.dot-success { background: #4caf50; }
.dot-original { background: #9e9e9e; }
.error {
  color: var(--c-danger, #d32f2f);
  font-size: 13px;
}
.error-banner {
  color: var(--c-danger, #d32f2f);
  font-size: 13px;
  padding: 8px 10px;
  border: 1px solid var(--c-danger, #d32f2f);
  border-radius: 4px;
  background: rgba(211, 47, 47, 0.05);
}
.hint {
  font-size: 12px;
  color: var(--c-text-muted, #666);
  line-height: 1.5;
}
.hint code {
  background: var(--c-bg-soft, #f5f5f5);
  padding: 1px 4px;
  border-radius: 3px;
  font-size: 11px;
}
</style>