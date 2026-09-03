<template>
  <span
    ref="wrapEl"
    class="tip-trigger"
    @mouseenter="onEnter"
    @mouseleave="onLeave"
  >
    <slot />
  </span>
</template>

<script setup lang="ts">
import { onMounted, onUpdated, ref } from 'vue';
import { useTooltip } from '../../composables/useTooltip';

const props = defineProps<{ text: string }>();

const wrapEl = ref<HTMLElement | null>(null);
const overflowed = ref(false);
const tip = useTooltip();

function detectOverflow() {
  const wrap = wrapEl.value;
  if (!wrap) return;
  const child = wrap.firstElementChild as HTMLElement | null;
  if (!child) return;
  overflowed.value =
    child.scrollWidth > child.clientWidth + 1 ||
    child.scrollHeight > child.clientHeight + 1;
}

onMounted(detectOverflow);
onUpdated(detectOverflow);

function onEnter() {
  detectOverflow();
  if (!overflowed.value) return;
  const wrap = wrapEl.value;
  if (!wrap) return;
  tip.show(props.text, wrap);
}

function onLeave() {
  tip.hide();
}
</script>

<style scoped>
.tip-trigger {
  display: inline-block;
  max-width: 100%;
}
</style>
