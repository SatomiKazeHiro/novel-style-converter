<template>
  <input
    class="input"
    :type="type"
    :value="modelValue"
    :placeholder="placeholder"
    @input="onInput"
  />
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue: string;
    type?: string;
    placeholder?: string;
  }>(),
  { type: 'text', placeholder: '' },
);
const emit = defineEmits<{ 'update:modelValue': [string] }>();
function onInput(e: Event) {
  emit('update:modelValue', (e.target as HTMLInputElement).value);
}
</script>

<style scoped>
.input {
  width: 100%;
  height: 34px;
  padding: 6px 12px;
  border: none;
  border-bottom: 1px solid var(--border-color);
  background: transparent;
  font-family: var(--font-sans);
  font-size: 14px;
  color: var(--text-primary);
  box-sizing: border-box;
  outline: none;
  border-radius: 0;
  transition: border-color 0.1s;
}
.input::placeholder {
  color: var(--text-muted);
  font-family: var(--font-serif);
  font-style: italic;
}
.input:hover {
  border-bottom-color: var(--border-strong);
}
.input:focus {
  border-bottom-color: var(--color-cinnabar);
}
</style>
