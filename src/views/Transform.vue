<template>
  <section class="transform">
    <TransformChapterNav
      v-if="store.chapter"
      :chapter="store.chapter"
      :current-idx="currentIdx"
      :total-chapters="store.allChapters.length"
      :can-go-prev="store.canGoPrev"
      :can-go-next="store.canGoNext"
      @prev="store.gotoChapter('prev')"
      @next="store.gotoChapter('next')"
    />

    <div v-if="store.error" class="alert">
      <span>{{ store.error }}</span>
      <Button size="small" @click="store.refresh()">↻ 重试</Button>
      <Button size="small" @click="router.push('/data-assets')">← 返回数据资产</Button>
    </div>

    <TransformVersionTabs
      v-if="store.transformations.length > 0"
      :transformations="store.transformations"
      :selected-id="store.selectedTransformationId"
      @select="store.selectTransformation"
    />

    <div v-if="store.loading" class="loading">加载中...</div>

    <template v-else-if="store.transformations.length === 0">
      <div class="empty">
        <p>该章节还没有转换结果</p>
        <Button kind="primary" @click="dialogOpen = true">⚙ 首次转换</Button>
      </div>
    </template>

    <template v-else>
      <TransformCompareView
        :original="store.originalContent"
        :transformed="store.selectedTransformation?.result_content ?? ''"
        :status="store.selectedTransformation?.status"
        :error="store.selectedTransformation?.error"
        :selected-version-label="versionLabel"
      />
      <TransformResultFooter
        :status="store.selectedTransformation?.status"
        :error="store.selectedTransformation?.error"
        :tokens-in="store.selectedTransformation?.tokens_in"
        :tokens-out="store.selectedTransformation?.tokens_out"
        :prompt-id="store.selectedTransformation?.prompt_id ?? 0"
        :model-config-id="store.selectedTransformation?.model_config_id ?? 0"
        :completed-at="store.selectedTransformation?.completed_at"
        @retransform="dialogOpen = true"
      />
    </template>

    <TransformDialog
      v-if="store.dataAssetId != null && store.chapterId != null"
      v-model:open="dialogOpen"
      :data-asset-id="store.dataAssetId"
      :chapter-id="store.chapterId"
      :default-prompt-id="store.selectedTransformation?.prompt_id"
      :default-model-config-id="store.selectedTransformation?.model_config_id"
      @submitted="onSubmitted"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import Button from '../components/ui/Button.vue';
import TransformChapterNav from '../components/TransformChapterNav.vue';
import TransformVersionTabs from '../components/TransformVersionTabs.vue';
import TransformCompareView from '../components/TransformCompareView.vue';
import TransformResultFooter from '../components/TransformResultFooter.vue';
import TransformDialog from '../components/TransformDialog.vue';
import { useTransformViewStore } from '../stores/transformView';

const route = useRoute();
const router = useRouter();
const store = useTransformViewStore();
const dialogOpen = ref(false);

const currentIdx = computed(() =>
  store.chapter ? store.allChapters.findIndex((c) => c.id === store.chapter!.id) : -1,
);

const versionLabel = computed(() => {
  const t = store.selectedTransformation;
  if (!t) return '';
  const idx = store.transformations.findIndex((x) => x.id === t.id);
  return `v${store.transformations.length - idx}`;
});

function parseChapterId(): number | null {
  const raw = route.params.chapterId;
  const n = Number(raw);
  if (!Number.isFinite(n) || n <= 0) return null;
  return n;
}

async function loadFromRoute() {
  const id = parseChapterId();
  if (id === null) {
    store.chapterId = null;
    store.chapter = null;
    store.allChapters = [];
    store.transformations = [];
    store.selectedTransformationId = null;
    store.error = `无效的 chapter ID: ${String(route.params.chapterId)}`;
    store.loading = false;
    return;
  }
  await store.load(id);
}

onMounted(loadFromRoute);
watch(() => route.params.chapterId, loadFromRoute);

async function onSubmitted() {
  await store.refresh();
}
</script>

<style scoped>
.transform { display: flex; flex-direction: column; height: 100%; gap: 12px; }
.alert {
  padding: 8px 12px; background: var(--bg-hover); color: var(--color-cinnabar-deep);
  border-radius: var(--radius-pin); font-size: 13px;
  display: flex; align-items: center; gap: 12px;
}
.loading {
  flex: 1; display: flex; align-items: center; justify-content: center;
  color: var(--text-secondary); font-size: 13px;
}
.empty {
  flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 16px; color: var(--text-secondary); font-size: 14px;
}
.empty p { margin: 0; }
</style>
