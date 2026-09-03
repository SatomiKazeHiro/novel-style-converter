<template>
  <header class="page-header">
    <div class="back">
      <slot name="back" />
    </div>
    <div class="title-block">
      <div class="title-row">
        <h2 class="title" :class="size">{{ title }}</h2>
        <slot name="meta" />
      </div>
      <p v-if="subtitle" class="subtitle">{{ subtitle }}</p>
    </div>
    <div class="actions">
      <slot name="actions" />
    </div>
  </header>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    title: string;
    subtitle?: string;
    /// default: 22px(列表页标题)。small: 18px(详情页 / 长文件名场景,
    /// 字号小一点能让单行多装十几个字、减少 ellipsis 截断)。
    size?: 'default' | 'small';
  }>(),
  { subtitle: '', size: 'default' },
);
</script>

<style scoped>
.page-header {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border-color);
}
.title-block {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.title-row {
  display: flex;
  align-items: baseline;
  gap: 12px;
  min-width: 0;
}
.title {
  margin: 0;
  font-family: var(--font-serif);
  font-size: var(--text-h2);
  font-weight: var(--font-weight-medium);
  line-height: var(--leading-tight);
  letter-spacing: -0.005em;
  color: var(--text-primary);
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.title.small {
  font-size: 18px;
}
.subtitle {
  margin: 4px 0 0;
  font-size: 13px;
  line-height: 1.4;
  color: var(--text-secondary);
}
.back {
  display: flex;
  align-items: center;
}
.back:empty { display: none; }
.actions {
  display: flex;
  gap: 12px;
  align-items: center;
  justify-self: end;
}
.actions:empty { display: none; }
</style>