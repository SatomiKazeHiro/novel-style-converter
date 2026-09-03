<template>
  <input
    class="number"
    type="number"
    :value="modelValue"
    :min="min"
    :max="max"
    :step="step"
    @input="onInput"
  />
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue: number | null;
    min?: number;
    max?: number;
    step?: number;
  }>(),
  { min: undefined, max: undefined, step: 1 },
);
const emit = defineEmits<{ 'update:modelValue': [number | null] }>();
function onInput(e: Event) {
  const raw = (e.target as HTMLInputElement).value;
  const v = raw === '' ? null : Number(raw);
  emit('update:modelValue', v);
}
</script>

<style scoped>
.number {
  width: 100%;
  height: 34px;
  padding: 6px 12px;
  border: none;
  border-bottom: 1px solid var(--border-color);
  background: transparent;
  font-family: var(--font-mono);
  font-size: 14px;
  color: var(--text-primary);
  box-sizing: border-box;
  outline: none;
  border-radius: 0;
  font-variant-numeric: tabular-nums;
  transition: border-color 0.1s;
}
.number:hover {
  border-bottom-color: var(--border-strong);
}
.number:focus {
  border-bottom-color: var(--color-cinnabar);
}
</style>
