import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { ChapterSegment, ChapterInput } from '../ipc/types';
import {
  commitDataAsset as ipcCommitDataAsset,
  findDataAssetByUpload as ipcFindDataAssetByUpload,
  getUploadText as ipcGetUploadText,
  getUpload as ipcGetUpload,
  listChapterSegments as ipcListChapterSegments,
  listCommittedSegments as ipcListCommittedSegments,
} from '../ipc/commands';
import { countChapterChars, stripInvisibles, stripTrailingInvisibles } from '../utils/splitChapters';

type SourceKind = 'committed' | 'fresh';

export const useChaptersStore = defineStore('chapters', () => {
  const uploadId = ref<number | null>(null);
  const rawText = ref<string>('');
  const filename = ref<string>('');

  const rawLines = computed<string[]>(() => rawText.value.split('\n'));

  const chapterSplits = ref<Set<string>>(new Set());
  const initialChapterSplits = ref<Set<string>>(new Set());
  const titles = ref<Map<string, string>>(new Map());
  const initialTitles = ref<Map<string, string>>(new Map());

  const workingChapters = ref<ChapterSegment[]>([]);

  const loading = ref(false);
  const error = ref<string | null>(null);
  const sourceKind = ref<SourceKind | null>(null);

  let requestToken = 0;

  const committed = computed(() => sourceKind.value === 'committed');

  function setEqual(a: Set<string>, b: Set<string>): boolean {
    if (a.size !== b.size) return false;
    for (const k of a) if (!b.has(k)) return false;
    return true;
  }
  function mapEqual(a: Map<string, string>, b: Map<string, string>): boolean {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) if (b.get(k) !== v) return false;
    return true;
  }

  const dirty = computed(() =>
    !setEqual(chapterSplits.value, initialChapterSplits.value) ||
    !mapEqual(titles.value, initialTitles.value),
  );

  function applyWorking(): ChapterSegment[] {
    const sortedKeys = [...chapterSplits.value].map(Number).sort((a, b) => a - b);
    const out: ChapterSegment[] = [];
    for (let i = 0; i < sortedKeys.length; i++) {
      const key = sortedKeys[i];
      const next = i + 1 < sortedKeys.length ? sortedKeys[i + 1] : rawLines.value.length;
      const title = titles.value.get(String(key));
      if (title === undefined) {
        throw new Error(`titles 缺 key=${key}：chapterSplits 与 titles 不一致`);
      }
      const content = rawLines.value.slice(key + 1, next).join('\n');
      out.push({ title, content: stripTrailingInvisibles(content), word_count: countChapterChars(content), title_line: key });
    }
    return out;
  }

  function recompute() { workingChapters.value = applyWorking(); }

  function recomputeInitialFromSegs(segs: ChapterSegment[]) {
    const splits = new Set<string>();
    const t = new Map<string, string>();
    for (const s of segs) {
      if (s.title_line < 0 || s.title_line >= rawLines.value.length) {
        throw new Error(`title_line 越界：upload_id=${uploadId.value} title_line=${s.title_line} rawLines=${rawLines.value.length}`);
      }
      splits.add(String(s.title_line));
      t.set(String(s.title_line), s.title);
    }
    initialChapterSplits.value = splits;
    initialTitles.value = t;
    chapterSplits.value = new Set(splits);
    titles.value = new Map(t);
  }

  function toggleChapterSplit(key: string) {
    if (chapterSplits.value.has(key)) {
      chapterSplits.value.delete(key);
      // 不 delete titles:用户编辑过的标题在 toggle-on 时必须能恢复。
      // chapterSplits != initialChapterSplits 已被 dirty 检测。
    } else {
      const line = Number(key);
      if (!Number.isFinite(line) || line < 0 || line >= rawLines.value.length) {
        throw new Error(`toggleChapterSplit 越界：key=${key} rawLines=${rawLines.value.length}`);
      }
      chapterSplits.value.add(key);
      if (!titles.value.has(key)) titles.value.set(key, stripInvisibles(rawLines.value[line]));
    }
    recompute();
  }

  function updateTitle(key: string, title: string) {
    if (!titles.value.has(key)) throw new Error(`updateTitle 未知 key=${key}`);
    titles.value.set(key, title);
    recompute();
  }

  function reset() {
    chapterSplits.value = new Set(initialChapterSplits.value);
    titles.value = new Map(initialTitles.value);
    recompute();
  }

  async function load(id: number) {
    uploadId.value = id;
    workingChapters.value = [];
    rawText.value = '';
    filename.value = '';
    chapterSplits.value = new Set();
    initialChapterSplits.value = new Set();
    titles.value = new Map();
    initialTitles.value = new Map();
    sourceKind.value = null;
    loading.value = true;
    error.value = null;
    ++requestToken;
    const token = requestToken;
    try {
      const [text, meta, dataAssetIds] = await Promise.all([
        ipcGetUploadText(id),
        ipcGetUpload(id).catch(() => null),
        ipcFindDataAssetByUpload(id).catch(() => [] as number[]),
      ]);
      if (token !== requestToken) return;
      rawText.value = text;
      filename.value = meta?.filename ?? '';
      const ownedDataAssetId = dataAssetIds[0] ?? null;
      const segs = ownedDataAssetId !== null
        ? await ipcListCommittedSegments(ownedDataAssetId)
        : await ipcListChapterSegments(id);
      if (token !== requestToken) return;
      sourceKind.value = ownedDataAssetId !== null ? 'committed' : 'fresh';
      recomputeInitialFromSegs(segs);
      recompute();
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  function unload() {
    ++requestToken;
    uploadId.value = null;
    rawText.value = '';
    filename.value = '';
    chapterSplits.value = new Set();
    initialChapterSplits.value = new Set();
    titles.value = new Map();
    initialTitles.value = new Map();
    workingChapters.value = [];
    sourceKind.value = null;
    error.value = null;
    loading.value = false;
  }

  async function reSplit() {
    if (uploadId.value === null || sourceKind.value !== 'committed') return;
    const id = uploadId.value;
    const token = ++requestToken;
    try {
      const fresh = await ipcListChapterSegments(id);
      if (token !== requestToken) return;
      sourceKind.value = 'fresh';
      recomputeInitialFromSegs(fresh);
      recompute();
    } catch (e: unknown) {
      if (token === requestToken) error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function commit(title: string): Promise<number> {
    if (uploadId.value === null) throw new Error('no upload loaded');
    if (loading.value) throw new Error('loading in progress, cannot commit');
    const segs: ChapterInput[] = workingChapters.value.map((s) => ({
      title: s.title,
      content: s.content,
      title_line: s.title_line,
    }));
    try {
      const newDataAssetId = await ipcCommitDataAsset(uploadId.value, { title, chapters: segs });
      sourceKind.value = 'committed';
      initialChapterSplits.value = new Set(chapterSplits.value);
      initialTitles.value = new Map(titles.value);
      return newDataAssetId;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    }
  }

  return {
    uploadId, rawText, filename, rawLines,
    workingChapters, chapterSplits, initialChapterSplits, titles, initialTitles,
    sourceKind, loading, error, committed, dirty,
    load, toggleChapterSplit, updateTitle, reset, reSplit, commit, unload,
    recomputeInitialFromSegs,
  };
});