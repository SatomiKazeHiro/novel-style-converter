// 前端调用规则:
// - 命令参数名(payload / id / chapterId / uploadId 等)由 Tauri 自动 camelCase 化,
//   invoke 外层传参对象用 camelCase 字段名。
// - 内层 DTO 由后端显式 `#[serde(rename_all="snake_case")]` 接收,前端必须按
//   snake_case 原样发(payload.base_url / payload.api_key / ...),不要 inline 改名。
// - 响应类型(后端实体)保持 snake_case,见 types.ts。
//
// Tauri v2: invoke 入口从 `@tauri-apps/api/tauri` 改 `@tauri-apps/api/core`。
import { invoke } from '@tauri-apps/api/core';
import type {
  ModelConfig, ModelConfigInput,
  UploadSummary, CleaningPreview,
  DataAssetChapter, DataAssetRow, DataAsset, CommitDataAssetInput,
  ChapterSegment, ChapterMeta, ChapterContentRow, Chapter,
  TransformationNovelSummary, TransformationChapterRow,
  CreateTransformationNovelInput, UpdateTransformationNovelInput,
  EnqueuePayload, EnqueueAllPayload, QueueSnapshot,
  Prompt, PromptInput, TestModelReport,
  CreateWorkflowInput, PromoteWorkflowInput, WorkflowSummary, WorkflowChapterRow, DeleteWorkflowResult,
  SourceChapterRow, ChapterWorkflowResultRow,
  UploadDeletePreview,
  AiCallLog, AiCallLogFilter, AiCallLogPage,
  ChapterPreviewRow, CommitPreviewInput, RegeneratePreviewInput,
  PreviewFirstChapterInput, PreviewFirstChapterOutput,
  AppendChaptersToBatchPayload, AppendChaptersResult,
} from './types';

// ─── Models ────────────────────────────────────────────────────────────────
export function listModels(): Promise<ModelConfig[]> {
  return invoke<ModelConfig[]>('list_models');
}

export function listModelsIncludingArchived(): Promise<ModelConfig[]> {
  return invoke<ModelConfig[]>('list_models_including_archived');
}

export function upsertModel(payload: ModelConfigInput): Promise<number> {
  return invoke<number>('upsert_model', { payload });
}

export function deleteModel(id: number): Promise<void> {
  return invoke<void>('delete_model', { id });
}

export function restoreModel(id: number): Promise<void> {
  return invoke<void>('restore_model', { id });
}

export function testModel(payload: ModelConfigInput): Promise<TestModelReport> {
  return invoke<TestModelReport>('test_model', { payload });
}

// ─── Uploads ───────────────────────────────────────────────────────────────
export function listUploads(): Promise<UploadSummary[]> {
  return invoke<UploadSummary[]>('list_uploads');
}

export function uploadFile(payload: { file_path: string; filename: string }): Promise<UploadSummary> {
  return invoke<UploadSummary>('upload_file', { payload });
}

export function deleteUpload(id: number): Promise<void> {
  return invoke<void>('delete_upload', { id });
}

export async function getUploadText(id: number): Promise<string> {
  const payload = await invoke<ArrayBuffer | string>('get_upload_text', { id });
  if (typeof payload === 'string') return payload;
  return new TextDecoder().decode(new Uint8Array(payload));
}

/// 按字节区间懒加载 upload 原文。后端会对齐到 UTF-8 字符边界,实际返回长度可能略小于 length。
/// 大文件详情页用此接口分块拉取,避免一次性渲染 N MB textarea 卡顿。
export async function getUploadTextChunk(
  id: number,
  byteOffset: number,
  byteLength: number,
): Promise<string> {
  const payload = await invoke<ArrayBuffer | string>('get_upload_text_chunk', {
    id,
    byteOffset,
    byteLength,
  });
  if (typeof payload === 'string') return payload;
  return new TextDecoder().decode(new Uint8Array(payload));
}

export function getUpload(id: number): Promise<UploadSummary> {
  return invoke<UploadSummary>('get_upload', { id });
}

export function updateUploadText(id: number, text: string): Promise<void> {
  return invoke<void>('update_upload_text', { id, text });
}

export function previewCleaning(
  text: string,
  ruleIds: string[],
): Promise<CleaningPreview> {
  return invoke<CleaningPreview>('preview_cleaning', { text, ruleIds });
}

// ─── Data assets ───────────────────────────────────────────────────────────
export function listDataAssetChapters(dataAssetId: number): Promise<DataAssetChapter[]> {
  return invoke<DataAssetChapter[]>('list_data_asset_chapters', { dataAssetId });
}


export function commitDataAsset(
  uploadId: number,
  payload: CommitDataAssetInput,
): Promise<number> {
  return invoke<number>('commit_data_asset', { uploadId, ...payload });
}

/** Upload id → 该 upload 派生的全部 data_asset id(按 id DESC)。可能为空。 */
export function findDataAssetByUpload(uploadId: number): Promise<number[]> {
  return invoke<number[]>('find_data_asset_by_upload', { uploadId });
}

/** 删 data_asset。locked 时拒绝;unlocked 时通过 FK CASCADE 自动清 chapters 等。 */
export function deleteDataAsset(dataAssetId: number): Promise<void> {
  return invoke<void>('delete_data_asset', { dataAssetId });
}

/** Library.vue "数据资产" tab:列出所有 data_asset + 来源 upload 文件名。 */
export function listDataAssets(): Promise<DataAssetRow[]> {
  return invoke<DataAssetRow[]>('list_data_assets');
}

// ─── Chapters ──────────────────────────────────────────────────────────────
export function listChapterSegments(uploadId: number): Promise<ChapterSegment[]> {
  return invoke<ChapterSegment[]>('list_chapter_segments', { uploadId });
}

/** 从 chapters 表读已提交章节段(byte_start/byte_end)。老数据(NULL 范围)被过滤。 */
export function listCommittedSegments(dataAssetId: number): Promise<ChapterSegment[]> {
  return invoke<ChapterSegment[]>('list_committed_segments', { dataAssetId });
}

export function listChapters(dataAssetId: number): Promise<ChapterMeta[]> {
  return invoke<ChapterMeta[]>('list_chapters', { dataAssetId });
}

export function getChapterContents(dataAssetId: number): Promise<ChapterContentRow[]> {
  return invoke<ChapterContentRow[]>('get_chapter_contents', { dataAssetId });
}

export function getChapter(chapterId: number): Promise<Chapter> {
  return invoke<Chapter>('get_chapter', { chapterId });
}

/** 编辑单章正文：后端同时按统一口径重算 word_count 落库。 */
export function updateChapterBody(chapterId: number, body: string): Promise<void> {
  return invoke<void>('update_chapter_body', { chapterId, body });
}

// ─── Transformation novels ─────────────────────────────────────────────────
export function listTransformationNovels(dataAssetId?: number): Promise<TransformationNovelSummary[]> {
  return invoke<TransformationNovelSummary[]>('list_transformation_novels', { dataAssetId });
}

export function getTransformationNovel(id: number): Promise<TransformationNovelSummary> {
  return invoke<TransformationNovelSummary>('get_transformation_novel', { id });
}
export function createTransformationNovel(payload: CreateTransformationNovelInput): Promise<number> {
  return invoke<number>('create_transformation_novel', { payload });
}

export function updateTransformationNovel(payload: UpdateTransformationNovelInput): Promise<void> {
  return invoke<void>('update_transformation_novel', { payload });
}

export function deleteTransformationNovel(id: number): Promise<void> {
  return invoke<void>('delete_transformation_novel', { id });
}

// ─── Transformation chapters ───────────────────────────────────────────────
export function listTransformationChapters(transformationNovelId: number): Promise<TransformationChapterRow[]> {
  return invoke<TransformationChapterRow[]>('list_transformation_chapters', { transformationNovelId });
}

export function listTransformationChaptersForChapter(chapterId: number): Promise<TransformationChapterRow[]> {
  return invoke<TransformationChapterRow[]>('list_transformation_chapters_for_chapter', { chapterId });
}

export function enqueueTransformationChapters(payload: EnqueuePayload): Promise<number[]> {
  return invoke<number[]>('enqueue_transformation_chapters', { payload });
}

export function enqueueAllChapters(payload: EnqueueAllPayload): Promise<number[]> {
  return invoke<number[]>('enqueue_all_chapters', { payload });
}

// ─── Workflows ─────────────────────────────────────────────────────────────
export const listTransformationSourceChapters = (tnId: number): Promise<SourceChapterRow[]> =>
  invoke<SourceChapterRow[]>('list_transformation_source_chapters', { tnId });

export const createWorkflow = (payload: CreateWorkflowInput): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('create_workflow', { payload });

// ── Workflow → DataAsset 转正 ──────────────────────────────────────
/// 把 Stopped workflow 的结果物化为新的 promoted data_asset。
export const promoteWorkflow = (input: PromoteWorkflowInput): Promise<DataAsset> =>
  invoke<DataAsset>('promote_workflow', { batchId: input.batchId, title: input.title });

/// 统计指定 workflow 已派生出多少 promoted da(给 UI badge 用)。
export const countPromotedDataAssetsByWorkflow = (batchId: number): Promise<number> =>
  invoke<number>('count_promoted_data_assets_by_workflow', { batchId });

/// 列出指定 workflow 派生的所有 promoted da(详情下钻用)。
export const listPromotedDataAssetsForWorkflow = (batchId: number): Promise<DataAsset[]> =>
  invoke<DataAsset[]>('list_promoted_data_assets_for_workflow', { batchId });

/// 列出指定 upload 派生的所有 data_asset(包含 source + promoted)。
export const listDataAssetsByUpload = (uploadId: number): Promise<DataAsset[]> =>
  invoke<DataAsset[]>('list_data_assets_by_upload', { uploadId });

export const listWorkflows = (tnId: number): Promise<WorkflowSummary[]> =>
  invoke<WorkflowSummary[]>('list_workflows', { tnId });

export const getWorkflow = (batchId: number): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('get_workflow', { batchId });


export const listWorkflowChapters = (batchId: number): Promise<WorkflowChapterRow[]> =>
  invoke<WorkflowChapterRow[]>('list_workflow_chapters', { batchId });

export const stopWorkflow = (batchId: number): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('stop_workflow', { batchId });

export const retryWorkflowChapters = (batchId: number, chapterIds: number[]): Promise<WorkflowSummary> =>
  invoke<WorkflowSummary>('retry_workflow_chapters', { batchId, chapterIds });
/// 删除工作流。仅 stopped/completed/terminated/cancelled 状态可删;后端 cascade
/// 处理 workflow_results / workflow_result_chapters / transformation_chapters / chapter_previews,
/// 并把 data_assets.source_workflow_id SET NULL(promoted da 物理保留)。
export const deleteWorkflow = (batchId: number): Promise<DeleteWorkflowResult> =>
  invoke<DeleteWorkflowResult>('delete_workflow', { batchId });



export const listChapterWorkflowResults = (tnId: number, chapterId: number): Promise<ChapterWorkflowResultRow[]> =>
  invoke<ChapterWorkflowResultRow[]>('list_chapter_workflow_results', { tnId, chapterId });

// ─── Queue ─────────────────────────────────────────────────────────────────
export function getQueueSnapshot(): Promise<QueueSnapshot> {
  return invoke<QueueSnapshot>('get_queue_snapshot');
}

// ─── Prompts ───────────────────────────────────────────────────────────────
export function listPrompts(): Promise<Prompt[]> {
  return invoke<Prompt[]>('list_prompts');
}

export function listPromptsIncludingArchived(): Promise<Prompt[]> {
  return invoke<Prompt[]>('list_prompts_including_archived');
}

export function getPrompt(id: number): Promise<Prompt> {
  return invoke<Prompt>('get_prompt', { id });
}

export function upsertPrompt(payload: PromptInput): Promise<number> {
  return invoke<number>('upsert_prompt', { payload });
}

export function deletePrompt(id: number): Promise<void> {
  return invoke<void>('delete_prompt', { id });
}

export function restorePrompt(id: number): Promise<void> {
  return invoke<void>('restore_prompt', { id });
}

export function countPromptUsage(promptId: number): Promise<number> {
  return invoke<number>('count_prompt_usage', { promptId });
}


// ─── AI calls ───────────────────────────────────────────────────────────────
export function listAiCallLogs(filter: AiCallLogFilter): Promise<AiCallLogPage> {
  return invoke<AiCallLogPage>('list_ai_call_logs', { filter });
}

export function getAiCallLog(id: number): Promise<AiCallLog | null> {
  return invoke<AiCallLog | null>('get_ai_call_log', { id });
}

export function clearAiCallLogs(): Promise<number> {
  return invoke<number>('clear_ai_call_logs');
}


// upload deletion preview: list derived data_assets without cascading.
export function previewUploadDeletion(uploadId: number): Promise<UploadDeletePreview> {
  return invoke<UploadDeletePreview>("preview_upload_deletion", { uploadId });
}

// === Overview ===
export function getOverviewGraph(): Promise<import("./types").OverviewGraph> {
  return invoke<import("./types").OverviewGraph>("get_overview_graph");
}
// ─── Chapter previews (RegeneratePreview dialog) ──────────────────────────
/// 发起预览生成。返回 preview.id;实际 AI 调用在后台进行。
/// 前端通过轮询 listChapterPreviews 直到 status != 'generating' 反映进度。
export function regenerateChapterPreview(
  payload: RegeneratePreviewInput,
): Promise<number> {
  return invoke<number>('regenerate_chapter_preview', {
    batchId: payload.batch_id,
    chapterId: payload.chapter_id,
    customInput: payload.custom_input,
  });
}

/// 提交预览(spec §4.2) —— 用草稿区内容覆盖 wrc.content,清空所有 preview 行。
/// 返回更新后的 WorkflowSummary(状态/计数可能变化)。
export function commitChapterPreview(payload: CommitPreviewInput): Promise<WorkflowSummary> {
  return invoke<WorkflowSummary>('commit_chapter_preview', { input: payload });
}

/// 列出某章节的 preview 行,按 id DESC。
export function listChapterPreviews(batchId: number, chapterId: number): Promise<ChapterPreviewRow[]> {
  return invoke<ChapterPreviewRow[]>('list_chapter_previews', { batchId, chapterId });
}

/// 放弃某个 preview 行。
export function discardChapterPreview(previewId: number): Promise<void> {
  return invoke<void>('discard_chapter_preview', { previewId });
}
// ─── Catalog (models.dev) ─────────────────────────────────────────────────
export type CatalogSource = 'bundled' | 'cache';

export interface CatalogMeta {
  source: CatalogSource;
  origin?: 'http' | 'drop';
  fetched_at?: string;
  sha256: string;
  size_bytes: number;
}

export interface CatalogStatus {
  source: CatalogSource;
  meta: CatalogMeta | null;
  bundled_size_bytes: number;
  cache_size_bytes: number | null;
}

export interface CatalogRefreshResult {
  ok: boolean;
  source: string;
  meta: CatalogMeta | null;
  error: string | null;
}

export interface CatalogImportResult {
  ok: boolean;
  meta: CatalogMeta | null;
  error: string | null;
}

export function catalogStatus(): Promise<CatalogStatus> {
  return invoke<CatalogStatus>('catalog_status');
}

export async function catalogRefresh(): Promise<CatalogRefreshResult> {
  return invoke<CatalogRefreshResult>('catalog_refresh');
}

export async function catalogImportDrop(jsonContent: string): Promise<CatalogImportResult> {
  return invoke<CatalogImportResult>('catalog_import_drop', { jsonContent });
}

export async function catalogReadActive(): Promise<string> {
  return invoke<string>('catalog_read_active');
}

/// 调系统默认浏览器打开外部链接。Tauri webview 的 <a target="_blank"> 会开 webview
/// 新标签,不会引导到外部浏览器 —— 走 Rust 命令调 open crate 等价于用户在桌面点链接。
/// 后端限制只接受 http/https 协议,其他(比如 javascript:)直接拒绝。
export function openExternalUrl(url: string): Promise<void> {
  return invoke<void>('open_external_url', { url });
}

/// 「新建工作流」试运行区 IPC wrapper(spec §3.4 / §5.1)。
/// 调一次 AI 跑 idx 最小的章节,返回 preview 结果;不写 batch / tc / wrc 行。
/// 用户满意后再通过 `createWorkflow({ ..., preview_first_chapter })` 把结果作为 seed 传入。
export function previewFirstChapter(
  input: PreviewFirstChapterInput,
): Promise<PreviewFirstChapterOutput> {
  return invoke<PreviewFirstChapterOutput>('preview_first_chapter', { input });
}

export function appendChaptersToBatch(payload: AppendChaptersToBatchPayload): Promise<AppendChaptersResult> {
  return invoke<AppendChaptersResult>('append_chapters_to_batch', {
    batchId: payload.batchId,
    chapterIds: payload.chapterIds,
  });
}
