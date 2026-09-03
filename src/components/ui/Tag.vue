<template>
  <span class="tag" :class="kind">
    <slot />
  </span>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ kind?: 'success' | 'warn' | 'danger' | 'info' }>(), {
  kind: 'info',
});
</script>

<style scoped>
.tag {
  display: inline-flex;
  align-items: center;
  padding: 2px 10px;
  font-size: 11px;
  font-weight: var(--font-weight-medium);
  border-radius: var(--radius-pin);
  border: 1px solid transparent;
  box-sizing: border-box;
  white-space: nowrap;
  letter-spacing: 0.02em;
}
.tag.success {
  background: var(--success-bg);
  color: var(--success);
  border-color: var(--success-border);
}
.tag.warn {
  background: var(--warn-bg);
  color: var(--warn);
  border-color: var(--warn-border);
}
.tag.danger {
  background: var(--danger-bg);
  color: var(--danger);
  border-color: var(--danger-border);
}
.tag.info {
  background: var(--bg-section);
  color: var(--text-secondary);
  border-color: var(--border-soft);
}
</style>
