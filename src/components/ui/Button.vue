<template>
  <button
    class="btn"
    :class="[kind, size, { loading }]"
    :disabled="disabled || loading"
    @click="emit('click', $event)"
  >
    <span v-if="loading" class="btn-spinner" />
    <slot />
  </button>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    kind?: 'primary' | 'danger' | 'default';
    size?: 'small' | 'default';
    disabled?: boolean;
    loading?: boolean;
  }>(),
  { kind: 'default', size: 'default', disabled: false, loading: false },
);
const emit = defineEmits<{ click: [MouseEvent] }>();
</script>

<style scoped>
/*
  kind = primary   → 朱砂红填充(印章/committed action)
  kind = default   → 浅米 outline,墨字
  kind = danger    → 深朱填充(destructive),与 primary 区分通过纯度而非 hue
*/
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 34px;
  padding: 6px 14px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  background: var(--color-sheet);
  color: var(--text-primary);
  font-size: 13px;
  font-weight: var(--font-weight-medium);
  font-family: inherit;
  cursor: pointer;
  box-sizing: border-box;
  letter-spacing: 0.02em;
  transition: background 0.1s, border-color 0.1s, color 0.1s;
}
.btn:hover:not(:disabled) {
  background: var(--bg-hover);
  border-color: var(--border-strong);
}
.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.btn.primary {
  background: var(--color-cinnabar);
  border-color: var(--color-cinnabar);
  color: #faf6ee;
}
.btn.primary:hover:not(:disabled) {
  background: var(--color-cinnabar-deep);
  border-color: var(--color-cinnabar-deep);
}
.btn.danger {
  background: transparent;
  border-color: var(--color-cinnabar-deep);
  color: var(--color-cinnabar-deep);
}
.btn.loading { cursor: progress; }
.btn-spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  margin-right: 6px;
  border: 2px solid currentColor;
  border-top-color: transparent;
  border-radius: 50%;
  animation: btn-spin 0.7s linear infinite;
  vertical-align: middle;
}
@keyframes btn-spin { to { transform: rotate(360deg); } }
.btn.danger:hover:not(:disabled) {
  background: var(--color-cinnabar-deep);
  color: #faf6ee;
}
.btn.small {
  height: 28px;
  padding: 4px 10px;
  font-size: 12px;
}
</style>
