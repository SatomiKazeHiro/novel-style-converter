<template>
  <!--
    全局确认对话框宿主 —— 全应用唯一的确认框渲染点。
    挂在 App.vue 根部（与 TooltipHost 同级），状态来自 composables/useConfirm.ts。
    业务代码一律 `await confirmDialog({...})`，不要自己再写 ConfirmDialog 状态。
  -->
  <Dialog
    :open="current !== null"
    :title="current?.title ?? ''"
    :width="440"
    @update:open="(v: boolean) => { if (!v) settle(false); }"
  >
    <p class="message">{{ current?.message }}</p>
    <template #footer>
      <Button v-if="!current?.alertOnly" @click="settle(false)">
        {{ current?.cancelText ?? '取消' }}
      </Button>
      <Button :kind="current?.kind === 'danger' ? 'danger' : 'primary'" @click="settle(true)">
        {{ current?.confirmText ?? '确认' }}
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import Dialog from './Dialog.vue';
import Button from './Button.vue';
import { useConfirmHost } from '../../composables/useConfirm';

const { current, settle } = useConfirmHost();
</script>

<style scoped>
/* 措辞里可能带换行（"\n"），必须 pre-wrap 才按预期断行。 */
.message {
  margin: 0;
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
