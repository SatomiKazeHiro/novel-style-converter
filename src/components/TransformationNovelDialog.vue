<template>
  <Dialog v-model:open="open" title="创建转换小说" :width="420">
    <div class="row">
      <label>源 upload</label>
      <span class="hint">id {{ dataAssetId }} · 已解析</span>
    </div>
    <div class="row">
      <label>标题 *</label>
      <Input v-model="title" placeholder="如:斗破_热血版" class="title-input" />
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button @click="open = false">取消</Button>
      <Button kind="primary" class="submit" :disabled="title.trim() === '' || submitting" @click="onSubmit">创建</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import Input from './ui/Input.vue';

const props = defineProps<{ dataAssetId: number }>();
const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{
  submit: [{
    data_asset_id: number;
    title: string;
  }];
}>();

const title = ref('');
const submitting = ref(false);
const error = ref<string | null>(null);

watch(open, (v) => {
  if (!v) return;
  title.value = '';
  error.value = null;
  submitting.value = false;
});

async function onSubmit() {
  error.value = null;
  submitting.value = true;
  try {
    emit('submit', {
      data_asset_id: props.dataAssetId,
      title: title.value.trim(),
    });
    open.value = false;
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
.row label {
  width: 90px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.row select {
  flex: 1;
  height: 34px;
  padding: 6px 12px;
  border: none;
  border-bottom: 1px solid var(--border-color);
  background: transparent;
  font-family: var(--font-sans);
  font-size: 14px;
  color: var(--text-primary);
  outline: none;
  box-sizing: border-box;
}
.hint { font-size: 13px; color: var(--text-muted); }
.error { color: var(--danger); font-size: 12px; }
</style>