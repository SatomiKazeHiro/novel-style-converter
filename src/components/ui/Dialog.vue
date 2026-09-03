<template>
  <Teleport to="body">
    <div v-if="open" class="overlay" :style="{ zIndex: zIndexValue }" @click.self="close">
      <div class="dialog" :class="{ 'dialog-full': size === 'full' }" :style="size === 'full' ? undefined : { width: widthCss }">
        <header class="header">
          <span class="title">{{ title }}</span>
          <button class="close" type="button" @click="close">×</button>
        </header>
        <div class="body"><slot /></div>
        <footer class="footer"><slot name="footer" /></footer>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { nextStack } from './dialog-stack';

const props = withDefaults(
  defineProps<{ title?: string; width?: number | string; size?: 'default' | 'full' }>(),
  { title: '', width: 540, size: 'default' },
);

const open = defineModel<boolean>('open', { required: true });

const widthCss = computed(() =>
  typeof props.width === 'number' ? `${props.width}px` : props.width,
);

/// 弹窗层级管理:
/// - nextStack() 返回全局递增的 z-index,确保后打开的弹窗总是叠在先打开的之上。
/// - stack 计数器在 dialog-stack.ts 里 —— 必须是真正的模块级(在 <script setup>
///   之外),否则每个 Dialog 实例独立计数,撞 z-index 后退化为 DOM 顺序,导致
///   嵌套弹窗被父弹窗遮挡。
/// - 上限 9999 后回卷到 1001,防止长期使用 z-index 无限增长。
const zIndexValue = ref(1000);
watch(open, (isOpen) => {
  if (isOpen) {
    zIndexValue.value = nextStack();
  }
}, { immediate: true });

function close() {
  open.value = false;
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(26, 23, 20, 0.45);
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog {
  background: var(--color-sheet);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-card);
  display: flex;
  flex-direction: column;
  max-height: 90vh;
  overflow: hidden;
  box-shadow: 0 1px 0 0 var(--border-rouge) inset, 0 12px 36px rgba(26, 23, 20, 0.18);
}
.header {
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-soft);
  display: flex;
  justify-content: space-between;
  align-items: center;
  position: relative;
}
.header::before {
  /* dialog 顶部一道窄红印条 — 像中国传统册页 */
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  height: 2px;
  background: var(--color-cinnabar);
}
.title {
  font-family: var(--font-serif);
  font-size: 17px;
  font-weight: var(--font-weight-regular);
  color: var(--text-primary);
}
.close {
  background: none;
  border: none;
  font-size: 22px;
  cursor: pointer;
  color: var(--text-muted);
  padding: 0;
  line-height: 1;
  font-family: var(--font-serif);
}
.close:hover {
  color: var(--color-cinnabar);
}
.body {
  padding: 20px;
  overflow: auto;
  flex: 1;
}
.footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border-soft);
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  background: var(--color-paper);
}

.dialog-full {
  width: 100vw;
  height: 100vh;
  max-height: 100vh;
  border-radius: 0;
  border: none;
}
.dialog-full .header {
  padding: 12px 24px;
}
.dialog-full .body {
  padding: 16px 24px;
}
.dialog-full .footer {
  padding: 12px 24px;
}
</style>
