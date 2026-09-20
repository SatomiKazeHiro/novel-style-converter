<template>
  <Dialog v-model:open="open" title="查看 prompt" :width="560">
    <div class="row">
      <label>名称</label>
      <span class="value">{{ initial.name }}</span>
    </div>
    <div class="row">
      <label>kind</label>
      <span class="value">{{ formatPromptKind(initial.kind) }}</span>
    </div>
    <div class="row column">
      <label>template</label>
      <textarea
        class="template-area"
        rows="14"
        readonly
        spellcheck="false"
        :value="initial.template"
      />
      <p class="hint">内置 prompt 只读。点进文本框后按 Ctrl+A 全选、Ctrl+C 复制。</p>
    </div>
    <div class="vars">
      <div class="vars-head">可用变量</div>
      <ul class="vars-list">
        <li v-for="v in PROMPT_VARIABLES" :key="v.token">
          <code class="var">{{ v.token }}</code>
          <span class="var-desc">{{ v.desc }}</span>
        </li>
      </ul>
    </div>
    <template #footer>
      <Button @click="open = false">关闭</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';
import { formatPromptKind } from '../utils/prompt-locale';
import { PROMPT_VARIABLES } from '../utils/prompt-variables';
import type { Prompt } from '../ipc/types';

defineProps<{ initial: Prompt }>();

const open = defineModel<boolean>('open', { required: true });
</script>

<style scoped>
.row {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}
.row.column {
  flex-direction: column;
  align-items: stretch;
}
.row label {
  width: 100px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.value {
  font-size: 14px;
  color: var(--text-primary);
}
.template-area {
  width: 100%;
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-paper);
  color: var(--text-primary);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  box-sizing: border-box;
}
.template-area:focus { border-color: var(--border-strong); }
.hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--text-secondary);
}
.vars {
  margin-top: 16px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-pin);
  background: var(--color-paper);
  padding: 8px 12px;
}
.vars-head {
  font-size: 12px;
  color: var(--text-secondary);
}
.vars-list {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
}
.vars-list li {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 2px 0;
}
.var {
  flex-shrink: 0;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 12px;
  color: var(--text-primary);
}
.var-desc {
  font-size: 12px;
  color: var(--text-secondary);
}
</style>
