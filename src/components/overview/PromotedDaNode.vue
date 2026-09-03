<template>
  <div class="ov-card promoted-da">
    <div class="ov-kind"><IconSparkles :size="14" :stroke-width="2.2" /> 派生数据</div>
    <div class="ov-title">{{ data.title }}</div>
    <div class="ov-meta">{{ data.meta || ' ' }}</div>
  </div>
</template>

<script setup lang="ts">
import IconSparkles from '~icons/lucide/sparkles';
defineProps<{ data: { title: string; meta?: string; subtitle?: string | null } }>();
</script>

<style scoped>
.ov-card {
  width: 240px;
  min-height: 92px;
  border-radius: 12px;
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-family: -apple-system, system-ui, sans-serif;
  box-shadow: 0 6px 14px -8px rgba(15, 23, 42, 0.35);
  border: 1.5px solid #047857;
  border-top: 4px solid #065F46;
  transition: transform 120ms ease, box-shadow 120ms ease;
  cursor: pointer;
  position: relative;
}
.ov-card:hover { transform: translateY(-1px); box-shadow: 0 10px 22px -10px rgba(15, 23, 42, 0.45); }
.ov-kind {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  opacity: 0.85;
}
.ov-title {
  font-size: 15px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ov-meta {
  font-size: 12px;
  opacity: 0.8;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.promoted-da {
  background: linear-gradient(180deg, #34D399 0%, #10B981 100%);
  color: #FFFFFF;
}
.promoted-da .ov-kind { color: #D1FAE5; }
.promoted-da .ov-meta { color: #D1FAE5; }
</style>