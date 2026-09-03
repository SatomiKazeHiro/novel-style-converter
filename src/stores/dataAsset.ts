import { defineStore } from 'pinia';
import { computed, ref, shallowRef } from 'vue';
import { listDataAssetChapters, listDataAssets as ipcListDataAssets, updateChapterBody } from '../ipc/commands';
import { countWords } from '../utils/format';
import type { DataAssetChapter } from '../ipc/types';
import type { DataAssetRow } from '../ipc/types';

/// dataAsset store 自己的 view 类型:不绑定 parse-page 的 ChapterSegment。
/// 这里只展示 title/content/word_count,以及 title_line 0/实际坐标供将来跳转;
/// promoted 章节无原文坐标 → title_line=0,UI 跳过即可。
type DataAssetChapterView = {
  title: string;
  content: string;
  word_count: number;
  title_line: number;
};

export const useDataAssetStore = defineStore('dataAsset', () => {
  const dataAssetId = ref<number | null>(null);
  const title = ref<string>('');
  const filename = ref<string>('');
  const parsedAt = ref<string | null>(null);
  const tnCount = ref<number>(0);
  const chapters = ref<DataAssetChapterView[]>([]);
  /// 跟 chapters 对齐:章节 db id,update_chapter_body 要用
  const chapterIds = ref<(number | null)[]>([]);
  const selectedIdx = ref<number | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  /// 编辑态:editing=false 时右侧只读;editing=true 时右侧 textarea,draftContent 是草稿。
  /// draftContent 用 shallowRef:可能十几 MB,跳过 deep proxy,赋值/读取走原生 string。
  const editing = ref(false);
  const draftContent = shallowRef<string>('');
  /// 进入编辑时锁定的原文,用于 dirty 判断和取消时还原。
  const editingOriginal = shallowRef<string>('');
  const editingDirty = ref(false);
  const saving = ref(false);
  /// 跟 chapters 数组对齐:每条对应章节的 source_kind(给 UI 标签用)。
  const sourceKinds = ref<('transformed' | 'original')[]>([]);
  /// 跟 chapters 对齐:每章的 edited_at(null = 未编辑)。跟 sourceK  维度正交。
  const editedAts = ref<(string | null)[]>([]);
  // 工作流转正相关元数据
  const kind = ref<'source' | 'promoted'>('source');
  const sourceWorkflowId = ref<number | null>(null);
  /// 上传文件 id:数据资产所属 upload,source 永远有值;promoted 来源工作流被删后 source_workflow_id=null,但 uploadId 仍在。
  const uploadId = ref<number | null>(null);
  const note = ref<string>('');

  let requestToken = 0;

  async function load(id: number) {
    dataAssetId.value = id;
    loading.value = true;
    error.value = null;
    chapters.value = [];
    chapterIds.value = [];
    sourceKinds.value = [];
    editedAts.value = [];
    cancelEdit();
    title.value = '';
    filename.value = '';
    parsedAt.value = null;
    tnCount.value = 0;
    kind.value = 'source';
    sourceWorkflowId.value = null;
    uploadId.value = null;
    note.value = '';
    ++requestToken;
    const token = requestToken;
    try {
      const [chs, assets] = await Promise.all([
        listDataAssetChapters(id),
        ipcListDataAssets(),
      ]);
      if (token !== requestToken) return;
      chapters.value = chs.map((c: DataAssetChapter) => ({
        title: c.title,
        content: c.body,
        word_count: c.word_count,
        /// promoted 章节 title_line 为 null,view 视图无跳转需求,记 0 表示"无坐标",
        /// 不影响后续 selectedContent / saveEdit(它们只读 content / word_count)。
        title_line: c.title_line ?? 0,
      }));
      chapterIds.value = chs.map((c: DataAssetChapter) => c.id);
      sourceKinds.value = chs.map((c: DataAssetChapter) => c.source_kind);
      editedAts.value = chs.map((c: DataAssetChapter) => c.edited_at);
      const row: DataAssetRow | undefined = assets.find((a: DataAssetRow) => a.id === id);
      if (row) {
        title.value = row.title;
        filename.value = row.filename;
        parsedAt.value = row.parsed_at;
        tnCount.value = row.tn_count;
        kind.value = row.kind;
        sourceWorkflowId.value = row.source_workflow_id;
        uploadId.value = row.upload_id;
        note.value = row.note ?? '';
      } else {
        kind.value = 'source';
        sourceWorkflowId.value = null;
        uploadId.value = null;
        note.value = '';
      }
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (token === requestToken) loading.value = false;
    }
  }

  function selectChapter(idx: number) {
    selectedIdx.value = idx;
  }
  function selectFirstIfNone() {
    if (selectedIdx.value == null && chapters.value.length > 0) selectedIdx.value = 0;
  }

  const selectedContent = computed(() => {
    const i = selectedIdx.value;
    if (i == null) return '';
    return chapters.value[i]?.content ?? '';
  });

  /// 派生 da(promoted) 禁编辑:正文要么来自 workflow 转换结果,要么是失败回退的原文,
  /// 让用户改会破坏"派生 = workflow 整体可交付"语义。
  /// 数据资产是独立数据实体——任意 kind 都可编辑,语义上等同"已选章节"。
const editable = computed(() => selectedIdx.value !== null);

  function enterEdit() {
    if (!editable.value) return;
    const c = chapters.value[selectedIdx.value ?? -1];
    if (!c) return;
    editingOriginal.value = c.content;
    draftContent.value = c.content;
    editingDirty.value = false;
    editing.value = true;
  }

  function cancelEdit() {
    editing.value = false;
    editingDirty.value = false;
    draftContent.value = '';
    editingOriginal.value = '';
  }

  /// textarea 双向绑定入口:Vue v-model 直接 assign shallowRef 即可,
  /// 这里顺手更新 dirty(避免模板里再写 computed)。
  function onDraftInput(next: string) {
    draftContent.value = next;
    editingDirty.value = next !== editingOriginal.value;
  }

  async function saveEdit(): Promise<void> {
    if (!editing.value || !editable.value) return;
    const idx = selectedIdx.value;
    if (idx == null) return;
    const cid = chapterIds.value[idx];
    if (cid == null) return;
    saving.value = true;
    error.value = null;
    try {
      await updateChapterBody(cid, draftContent.value);
      // 本地同步:按统一口径重算字数,跟后端落库值保持一致
      chapters.value[idx] = {
        ...chapters.value[idx],
        content: draftContent.value,
        word_count: countWords(draftContent.value),
      };
      // 后端 update_body 同时写了 edited_at;本地同步保持一致
      editedAts.value[idx] = new Date().toISOString();
      cancelEdit();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      saving.value = false;
    }
  }

  return {
    dataAssetId, title, filename, parsedAt, tnCount,
    chapters, chapterIds, sourceKinds, editedAts, selectedIdx, selectedContent,
    loading, error,
    kind, sourceWorkflowId, uploadId, note,
    editable, editing, draftContent, editingDirty, saving,
    load, selectChapter, selectFirstIfNone,
    enterEdit, cancelEdit, onDraftInput, saveEdit,
  };
});

/// 前端兜底用的统一口径字数:跟后端 word::count 一致,saveEdit 落库前本地同步。
/// word_count() 走 nsc-core 的 word 模块,前端没法直接 import Rust 函数,
/// 覆盖 Rust is_whitespace 的所有空白字符(space/tab/LF/CR + Unicode 空白)。
///
/// 改前曾在此文件本地定义,经 dead-code 审计发现与 `src/utils/format.ts:45`
/// 导出函数完全等价(16 个测试 case 实证:ASCII/中文/CJK 标点/NBSP/全角空格/
/// ZWSP/BOM/CRLF/VT/FF 全部产出一致),改用 import 形式。
