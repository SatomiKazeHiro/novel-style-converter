<template>
  <Dialog
    v-model:open="open"
    :title="title"
    :width="420"
  >
    <p class="message">{{ message }}</p>
    <template #footer>
      <Button kind="primary" @click="onOk">{{ okText }}</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import Dialog from './Dialog.vue';
import Button from './Button.vue';

withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    message: string;
    okText?: string;
  }>(),
  { okText: '知道了' },
);

const emit = defineEmits<{
  'update:open': [boolean];
  ok: [];
}>();

const open = defineModel<boolean>('open', { required: true });

function onOk() {
  open.value = false;
  emit('ok');
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