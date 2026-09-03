import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import {
  getChapter as ipcGetChapter,
  listChapters as ipcListChapters,
  listTransformationChaptersForChapter as ipcListTnForChapter,
} from '../ipc/commands';
import type { Chapter, ChapterMeta, TransformationChapterRow } from '../ipc/types';

export const useTransformViewStore = defineStore('transformView', () => {
  const chapterId = ref<number | null>(null);
  const dataAssetId = ref<number | null>(null);
  const chapter = ref<Chapter | null>(null);
  const allChapters = ref<ChapterMeta[]>([]);
  const transformations = ref<TransformationChapterRow[]>([]);
  const selectedTransformationId = ref<number | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  let requestToken = 0;

  async function load(id: number) {
    chapterId.value = id;
    chapter.value = null;
    allChapters.value = [];
    transformations.value = [];
    selectedTransformationId.value = null;
    loading.value = true;
    error.value = null;
    ++requestToken;
    const token = requestToken;
    try {
      const ch = await ipcGetChapter(id);
      dataAssetId.value = ch.data_asset_id;
      const [chs, tns] = await Promise.all([
        ipcListChapters(ch.data_asset_id),
        ipcListTnForChapter(id),
      ]);
      if (token !== requestToken) return;
      chapter.value = ch;
      allChapters.value = chs;
      transformations.value = tns;
      selectedTransformationId.value = tns[0]?.id ?? null;
    } catch (e: unknown) {
      if (token === requestToken) {
        chapter.value = null;
        allChapters.value = [];
        transformations.value = [];
        selectedTransformationId.value = null;
        error.value = e instanceof Error ? e.message : String(e);
      }
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  async function refresh() {
    if (chapterId.value == null) return;
    await load(chapterId.value);
  }

  function selectTransformation(id: number) {
    if (!transformations.value.some((t) => t.id === id)) return;
    selectedTransformationId.value = id;
  }

  const currentIndex = computed(() => {
    const ch = chapter.value;
    if (ch === null) return -1;
    return allChapters.value.findIndex((c) => c.id === ch.id);
  });

  async function gotoChapter(direction: 'prev' | 'next') {
    const currentIdx = currentIndex.value;
    if (currentIdx < 0) return;
    const target = direction === 'prev' ? allChapters.value[currentIdx - 1] : allChapters.value[currentIdx + 1];
    if (!target) return;
    await load(target.id);
  }

  const selectedTransformation = computed(() =>
    transformations.value.find((t) => t.id === selectedTransformationId.value) ?? null,
  );

  const originalContent = computed(() => chapter.value?.body ?? '');

  const canGoPrev = computed(() => currentIndex.value > 0);
  const canGoNext = computed(() => {
    const i = currentIndex.value;
    return i >= 0 && i < allChapters.value.length - 1;
  });

  return {
    chapterId, dataAssetId, chapter, allChapters,
    transformations, selectedTransformationId, selectedTransformation,
    loading, error, originalContent, canGoPrev, canGoNext,
    load, refresh, selectTransformation, gotoChapter,
  };
});
