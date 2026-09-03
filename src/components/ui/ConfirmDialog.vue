<template>
  <Dialog
    v-model:open="open"
    :title="title"
    :width="420"
  >
    <p class="message">{{ message }}</p>
    <template #footer>
      <Button @click="onCancel">{{ cancelText }}</Button>
      <Button :kind="kind === 'danger' ? 'danger' : 'primary'" @click="onConfirm">
        {{ confirmText }}
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import Dialog from './Dialog.vue';
import Button from './Button.vue';

const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    message: string;
    confirmText?: string;
    cancelText?: string;
    kind?: 'default' | 'danger';
  }>(),
  { confirmText: '确认', cancelText: '取消', kind: 'default' },
);

const emit = defineEmits<{
  'update:open': [boolean];
  confirm: [];
  cancel: [];
}>();

const open = defineModel<boolean>('open', { required: true });

function onConfirm() {
  open.value = false;
  emit('confirm');
}

function onCancel() {
  open.value = false;
  emit('cancel');
}
</script>

<style scoped>
.message {
  margin: 0;
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>