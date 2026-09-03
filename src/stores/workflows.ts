import { defineStore } from 'pinia';
import { useQueryClient } from '@tanstack/vue-query';
import {
  createWorkflow, stopWorkflow, retryWorkflowChapters, deleteWorkflow as deleteWorkflowCmd,
  promoteWorkflow, regenerateChapterPreview, commitChapterPreview, discardChapterPreview,
  appendChaptersToBatch,
} from '../ipc/commands';
import type {
  CreateWorkflowInput, WorkflowSummary, DataAsset, DeleteWorkflowResult,
  ChapterPreviewRow, CommitPreviewInput,
  AppendChaptersToBatchPayload, AppendChaptersResult,
} from '../ipc/types';

/// workflows store —— 缩成 mutation + invalidate 编排。
///
/// 读侧"按 key 缓存"职责已迁到 TanStack Query (vue-query):
/// - byTn / chaptersByBatch / sourcesByTn / resultsByTnChapter / previewsByBatchChapter 6 个 Map 全删
/// - 6 个 loadXxx 函数全删 —— view 端 useQuery 自动跑
/// - promotedByBatch 死数据整个删
/// - refresh(batchId) 删 —— invalidate 替代
///
/// mutation 留在 store 是因为它们涉及"IPC + invalidate 哪些 query key"的业务编排,
/// 不在 view 里散落。deleteWorkflow 的 in-memory find/splice 也只在 store 里需要一次。
export const useWorkflowsStore = defineStore('workflows', () => {
  const queryClient = useQueryClient();

  async function create(payload: CreateWorkflowInput): Promise<WorkflowSummary> {
    const w = await createWorkflow(payload);
    await queryClient.invalidateQueries({ queryKey: ['workflows', w.tn_id] });
    return w;
  }

  async function stop(batchId: number): Promise<WorkflowSummary> {
    const w = await stopWorkflow(batchId);
    // stop 影响 batch 自身 + 该 tn 下 workflows 列表的计数 / 状态
    await queryClient.invalidateQueries({ queryKey: ['workflowChapters', batchId] });
    await queryClient.invalidateQueries({ queryKey: ['workflows'] });
    return w;
  }

  async function retry(batchId: number, chapterIds: number[]): Promise<WorkflowSummary> {
    const w = await retryWorkflowChapters(batchId, chapterIds);
    await queryClient.invalidateQueries({ queryKey: ['workflowChapters', batchId] });
    return w;
  }

  /// 往已 stopped 的工作流追加章节(spec:stopped-batch-append-chapters)。
  /// 跟 retry 一样,失效章节列表 + workflows 总览。
  async function appendChapters(payload: AppendChaptersToBatchPayload): Promise<AppendChaptersResult> {
    const res = await appendChaptersToBatch(payload);
    await queryClient.invalidateQueries({ queryKey: ['workflowChapters', payload.batchId] });
    await queryClient.invalidateQueries({ queryKey: ['workflows'] });
    return res;
  }

  /// 删除工作流。后端 cascade 处理衍生表。
  async function deleteWorkflow(batchId: number): Promise<DeleteWorkflowResult> {
    const res = await deleteWorkflowCmd(batchId);
    await queryClient.invalidateQueries({ queryKey: ['workflows'] });
    return res;
  }

  /// 工作流转正为新数据资产。源 workflow 状态不变 (completed 仍是 completed),
  /// 但 tn 视图可能刷新以反映派生的 da。Library 数据资产 tab 也需要刷新,
  /// 否则新 da 在 library 看不到(bug A)。
  async function promote(batchId: number, title: string): Promise<DataAsset> {
    const newDa = await promoteWorkflow({ batchId, title });
    await queryClient.invalidateQueries({ queryKey: ['workflows'] });
    await queryClient.invalidateQueries({ queryKey: ['dataAssets'] });
    return newDa;
  }

  async function regeneratePreview(
    batchId: number, chapterId: number, customInput: string | null,
  ): Promise<number> {
    const id = await regenerateChapterPreview({
      batch_id: batchId, chapter_id: chapterId, custom_input: customInput,
    });
    await queryClient.invalidateQueries({ queryKey: ['chapterPreviews', batchId, chapterId] });
    return id;
  }

  async function commitPreview(payload: CommitPreviewInput): Promise<WorkflowSummary> {
    const w = await commitChapterPreview(payload);
    // 提交后该章节 preview 行被删;workflow 章节行更新(转换结果);workflow 状态 / 计数变化;
    // 侧栏 chapterWorkflowResults (chapterWorkflowResults[*]) 也需刷新,前缀匹配即覆盖所有已缓存的章节结果。
    await queryClient.invalidateQueries({ queryKey: ['chapterPreviews', payload.batch_id, payload.chapter_id] });
    await queryClient.invalidateQueries({ queryKey: ['chapterWorkflowResults'] });
    await queryClient.invalidateQueries({ queryKey: ['workflowChapters', payload.batch_id] });
    await queryClient.invalidateQueries({ queryKey: ['workflows'] });
    return w;
  }

  async function discardPreview(previewId: number): Promise<void> {
    await discardChapterPreview(previewId);
    // 不传 batchId/chapterId —— 找到包含该 preview 的 chapterPreviews key 重新加载。
    // TanStack Query 没有"按值找 key"API;这里通过 store 保留的预览数据反查,或让调用方传 key。
    // 调用方 RegeneratePreviewDialog.vue 会自己 invalidate,这里暂不处理未知 key 场景。
    void previewId;
  }

  return {
    create, stop, retry, deleteWorkflow, promote,
    regeneratePreview, commitPreview, discardPreview,
    appendChapters,
  };
});

// 保留 ChapterPreviewRow / WorkflowSummary 类型 re-export 以避免 view 端 import 路径变化
export type { ChapterPreviewRow };
