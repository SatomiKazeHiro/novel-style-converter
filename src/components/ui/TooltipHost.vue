<template>
  <Teleport to="body">
    <div
      id="app-tooltip-host"
      class="tip-host"
      :class="{ visible }"
      :style="{ left: `${x}px`, top: `${y}px` }"
      role="tooltip"
    >
      {{ text }}
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { useTooltip } from '../../composables/useTooltip';
const { text, visible, x, y } = useTooltip();
</script>

<style scoped>
.tip-host {
  position: fixed;
  top: 0;
  left: 0;
  /* x/y 是 floating-ui 返回的 floating 左上角坐标· translate 仅用来水平居中 */
  transform: translateX(-50%);
  background: var(--color-sheet);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pin);
  padding: 4px 10px;
  font-size: 12px;
  font-family: var(--font-sans);
  color: var(--text-primary);
  white-space: nowrap;
  max-width: 60vw;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
  box-shadow: 0 2px 6px -2px rgba(26, 23, 20, 0.18);
  opacity: 0;
  transition: opacity 120ms ease;
  z-index: 9999;
}
.tip-host.visible {
  opacity: 1;
}
</style>
