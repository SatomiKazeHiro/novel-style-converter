// 字段命名约定:
// - 响应类型(后端 `#[derive(Serialize)]` 的实体,如 `ModelConfig` / `Upload`)
//   保持 snake_case,与 `crates/nsc-core/src/models/*.rs` 一一对应。
// - IPC 入参的**命令参数名**(`payload` / `id` 等)由 Tauri 自动 camelCase 化,
//   所以外层 `invoke('cmd', { ... })` 的 key 用 camelCase。
// - IPC 入参的内层 DTO(后端显式 `#[serde(rename_all="snake_case")]` 的)
//   必须按 snake_case 原样发,前端 wrapper 不要做任何 inline 改名。
// 字段变更必须同步修改后端 DTO + 本文件 + commands.ts 中对应 wrapper。

/**
 * 后端 `model_configs` 行的前端镜像。
 * - `api_key` 明文存 SQLite,前端拿到也要原样回传(不要脱敏 —— 提交时仍需要真实值)。
 * - `concurrency` 是 per-model 并发上限:worker 端按 `model_config_id` 共享信号量。
 *   有效范围 [1,16];超过物理 worker 数(默认 2)不会触发额外阻塞,但下限 1 防止 0 死锁。
 * - `archived = 1` 表示软删(API key 已被清空);仍保留在 list 响应里供 UI 展示历史 model。
 * - 字段全部 snake_case 来自后端 serde(后端**不**做 rename)
 */
export interface ModelConfig {
  id: number;
  name: string;
  base_url: string;
  api_key: string;
  model: string;
  max_tokens: number | null;
  /** 模型最大上下文窗口（输入 tokens 上限）。null = 不强制校验。 */
  max_context: number | null;
  temperature: number | null;
  /** 用户主动关闭思考的开关 —— true = 发 reasoning_effort:"none",false = 模型自决。
   *  仅对官方支持该能力的模型生效(由 UI 控制何时暴露)。 */
  disable_thinking: boolean;
  concurrency: number;
  archived: number;
}

/**
 * `upsert_model` / `test_model` 入参:`id === 0` 表示新建,否则按 id 更新。
 * 这是后端 snake_case DTO(内层字段原样发,不要 inline 改名)。
 */
export type ModelConfigInput = Omit<ModelConfig, 'id' | 'archived'> & { id: number };

/**
 * `test_model` 结构化返回:
 * - 成功：`content_preview` 填响应前 200 字符，`tokens_in/out` 来自 provider usage。
 * - 失败：`error` 填完整字符串（provider 创建失败 / 非 2xx / 空 choices / 缺 usage 都会写），
 *   `content_preview` 与 tokens 全为 null。
 * - 任意路径都会填 `latency_ms`（创建 provider 失败也计超时）。
 */
export interface TestModelReport {
  model: string;
  base_url: string;
  latency_ms: number;
  tokens_in: number | null;
  tokens_out: number | null;
  content_preview: string | null;
  error: string | null;
}

/// 删除工作流结果 —— 后端 `DeleteWorkflowResult`,snake_case。
/// - deleted_batch_id:被删的 batch id。
/// - promoted_data_asset_count:删除时已派生自此工作流的 promoted da 数(UI 提示用)。
export interface DeleteWorkflowResult {
  deleted_batch_id: number;
  promoted_data_asset_count: number;
}

/// State 1: 原始上传文件元数据。不含章节结构(章节在 data_assets)。
export interface UploadSummary {
  id: number;
  sha256: string;
  filename: string;
  byte_size: number;
  uploaded_at: string;
  file_path: string;
  /// zh-aware 字数(汉字 + 字母 + 数字),upload_file 时后端一次算好。
  word_count: number;
}

/// 数据资产类型 —— source 是原始解析产物;promoted 是从工作流结果派生的新资产。
export type DataAssetKind = 'source' | 'promoted';

/// 单条 data_asset 元数据(供 promote_workflow / list_data_assets_by_upload 等返回)。
export interface DataAsset {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  source_filename: string;
  kind: DataAssetKind;
  source_workflow_id: number | null;
  source_data_asset_id: number | null;
  note: string;
}

/// State 2: 一次解析结果 = 一份 data_asset + 一组分章节切片。
export interface DataAssetSummary {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  source_filename: string;
  tn_count: number;
}

/// Library.vue "数据资产" tab 行:data_asset 元数据 + 来源 upload 文件名 + 章节总字数。
export interface DataAssetRow {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  filename: string;
  byte_size: number;
  /// SUM(chapters.word_count) WHERE data_asset_id = da.id銆?
  word_count: number;
  /// COUNT(transformation_novels.id) WHERE data_asset_id = da.id。
  tn_count: number;
  /// 资产类型:source = 原始解析;promoted = 从工作流结果派生。
  kind: DataAssetKind;
  /// 当 kind=promoted 时,记录源 workflow(batch.id);source 时为 null。
  source_workflow_id: number | null;
  /// 当 kind=promoted 时,记录源 data_asset.id;source 时为 null。
  source_data_asset_id: number | null;
  /// 用户备注。
  note: string;
  /// 派生出多少 promoted da(仅 source 类型有值,promoted 类型始终 0)。
  promoted_count: number;
}

/// State 2 章节元数据(供 list_data_asset_chapters 返回)。正文由前端按 byte 切片 original_text。
export interface DataAssetChapter {
  id: number;
  idx: number;
  title: string;
  body: string;
  word_count: number;
  /// 章节来源:transformed = 工作流转换结果;original = 原文(派生 da 失败章节回退)。
  source_kind: 'transformed' | 'original';
  source_chapter_id: number | null;
  edited_at: string | null;
  /// 标题文本在 upload.original_text 里的 0-based 行号。null = 无原文坐标(promoted 章节)。
  title_line: number | null;
}

/// commit_data_asset 入参:title + 章节列表(每个含 title + content + title_line)。
export interface CommitDataAssetInput {
  title: string;
  chapters: Array<{
    title: string;
    content: string;
    title_line: number;
  }>;
}

export interface ChapterSegment {
  title: string;
  content: string;
  word_count: number;
  /// 标题行 0-based 行号
  title_line: number;
  edited_at?: string | null;
}

export interface ChapterMeta {
  id: number;
  idx: number;
  title: string;
  word_count: number;
  edited_at?: string | null;
}

/**
 * `get_chapter_contents` 返回:章节正文预览(预览页用)。内容是后端从
 * `uploads.original_text` 按 byte range 切片后、剥首行标题再 trim。
 */
export interface ChapterContentRow {
  idx: number;
  title: string;
  content: string;
}

/// 章节切片实体。byte_start/byte_end 永远在 upload.original_text 坐标系。
export interface Chapter {
  id: number;
  data_asset_id: number;
  idx: number;
  title: string;
  body: string;
  word_count: number;
  /// 章节来源:transformed = 工作流转换结果;original = 原文(派生 da 的失败章节回退)。
  source_kind: 'transformed' | 'original';
  /// 派生时指向源 chapter.id(只在派生 da 里有值)。
  source_chapter_id: number | null;
  edited_at: string | null;
}

/**
 * `commit_data_asset` 入参的章节元素。
 * 仅标题 + byte 范围。后端按 byte range 切片原文计算 `word_count` / `idx`。
 */
export type ChapterInput = {
  title: string;
  content: string;
  title_line: number;
};

/**
 * `list_transformation_novels` 返回:转换小说元数据。
 * `chapters_count` 是该 `data_asset_id` 下所有 chapters 的总数,
 * 不代表这本 tn 实际有多少 transformation_chapter 行。
 */
export interface TransformationNovelSummary {
  id: number;
  data_asset_id: number;
  title: string;
  created_at: string;
  chapters_count: number;
  note: string;
  workflow_count: number;
  running_workflow_count: number;
}

/**
 * `create_transformation_novel` 入参:后端 snake_case DTO,
 * 三个默认字段为可空。内层字段原样发,不要 inline 改名。
 * 命名带 Input 后缀,与后端 `*Payload` 区分,避免跨语言同名歧义。
 */
export interface CreateTransformationNovelInput {
  data_asset_id: number;
  title: string;
}

/**
 * `update_transformation_novel` 入参:后端 snake_case DTO,三个默认字段可空。
 * null 表示清空存量默认值。后端 update 行为:用 payload 覆盖 cur.default_*)。
 */
export interface UpdateTransformationNovelInput {
  id: number;
  title: string;
}

// === Workflow 工作流 ===
/**
 * 后端 `BatchStatus` 全部 7 值。前端必须穷举映射中文,否则 UI 会甩原始字符串。
 * - pending/running: 工作中。
 * - stopped: spec §3.3 收尾态,只能 retry 空槽,不再回 running。
 * - paused: 失败策略 = pause_and_review,批停在等用户决策(继续/终止/跳过)。
 * - completed/terminated/cancelled: batch 的最终终态,含义不可逆。
 */
export type WorkflowStatus = 'pending' | 'running' | 'stopped' | 'paused' | 'completed' | 'terminated' | 'cancelled';

/** `promote_workflow` 入参:把某个 batch 的结果集转正成新的 data_asset。 */
export interface PromoteWorkflowInput {
  batchId: number;
  title: string;
}

/**
 * `list_workflows` / `get_workflow` 返回:工作流汇总 + 章节计数。
 * counts 直接嵌在行内 —— 不用单独的 count 接口。
 */
export interface WorkflowSummary {
  id: number;
  tn_id: number;
  label: string | null;
  status: WorkflowStatus;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  done_count: number;
  failed_count: number;
  skipped_count: number;
  total_count: number;
  promoted_count: number;
  // 新增(append_chapters spec §3.2 / Task 8)—— 同质配置字段 join prompts/model_configs。
  // - prompt_name / model_display_name:archived 或物理删除时,后端 fallback 为 ''。
  //   UI 的 AppendChaptersDialog 仅用于展示上下文,空串视为"该来源已被清理"。
  // - mode 跟 CreateWorkflowInput 一致('compress' / 'style')。
  // - ctx_* 是 stopped batch append 时的"沿用旧配置"读端字段。
  prompt_name: string;
  model_display_name: string;
  mode: 'compress' | 'style';
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
}

/** `list_workflow_chapters` 返回:tc 行 + 章节标题/idx + 关联结果槽预览。 */
export interface WorkflowChapterRow {
  tc_id: number;
  chapter_id: number;
  chapter_idx: number;
  chapter_title: string;
  status: TransformStatus;
  error: string | null;
  content_preview: string | null;
  is_empty_slot: boolean;
}

/** `list_transformation_source_chapters` 返回:tn 下全部源章节 + 非空结果数。 */
export interface SourceChapterRow {
  chapter_id: number;
  idx: number;
  title: string;
  word_count: number;
  non_empty_result_count: number;
}

/** `list_chapter_workflow_results` 返回:某源章节在所有工作流里的结果(按 batch_id DESC)。 */
export interface ChapterWorkflowResultRow {
  batch_id: number;
  batch_label: string | null;
  batch_status: WorkflowStatus;
  batch_ended_at: string | null;
  content: string | null;
  status: TransformStatus;
}

/**
 * `create_workflow` 入参:后端 snake_case DTO,所有字段必填(spec §5.1)。
 *
 * `on_failure_policy` 是章节失败时的处理策略:
 * - `pause_and_review`: 失败时 batch 转 Paused,等用户在 modal 里手动决策(重试/跳过/终止)
 * - `skip_failed`:      失败时该章标 Skipped,继续派下一章(batch 留 Running)
 */
export interface CreateWorkflowInput {
  tn_id: number;
  label: string | null;
  chapter_ids: number[];
  prompt_id: number;
  model_config_id: number;
  mode: 'compress' | 'style';
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
  on_failure_policy: 'pause_and_review' | 'skip_failed';
  /// 试运行首章结果(spec §3.1 / §4.2)。用户满意后由 dialog 状态传入 create_workflow,后端事务内把 idx 最小那个 chapter 的 tc 标 done;为 null 时与原行为一致(所有 tc pending)。
  preview_first_chapter: FirstChapterSeed | null;
}

/**
 * `transformation_chapters.status` 状态机:
 * `pending` → `running` → (`done` | `failed` | `cancelled`)
 * 失败不自动重试 —— 用户手动调 `enqueue_transformation_chapters` 重排队。
 */
export type TransformStatus = 'pending' | 'running' | 'done' | 'failed' | 'skipped' | 'cancelled';

/**
 * `list_transformation_chapters` / `list_transformation_chapters_for_chapter` 返回:
 * 一次转换任务的完整状态。`chapter_idx` / `chapter_title` 是 join `chapters` 表拼上的,
 * 方便 Transform 页直接展示,无需二次请求。
 */
export interface TransformationChapterRow {
  id: number;
  transformation_novel_id: number;
  chapter_id: number;
  chapter_idx: number;
  chapter_title: string;
  mode: 'compress' | 'style';
  prompt_id: number;
  model_config_id: number;
  status: TransformStatus;
  result_content: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
  error: string | null;
  started_at: string | null;
  completed_at: string | null;
  batch_id: number | null;
  style_ref_chapter_id: number | null;
}

/**
 * `enqueue_transformation_chapters` 入参。三个上下文数:
 * - `ctx_prev_original` —— 模板 `{{prev_original}}` 占位的前文原文章数
 * - `ctx_prev_transformed` —— 模板 `{{prev_transformed}}` 占位的前文已转换章数
 * (画风参考,不污染原文上下文;若前面没有已转换结果则渲染为 `(暂无已转换参考)`)
 * - `ctx_next_original` —— 模板 `{{next_original}}` 占位的后文原文章数
 * 后端按 (chapter_id, prompt_id, model_config_id) 同时匹配才视为画风参考。
 */
export type EnqueuePayload = {
  transformation_novel_id: number;
  chapter_ids: number[];
  prompt_id: number;
  model_config_id: number;
  ctx_prev_original: number;
  ctx_prev_transformed: number;
  ctx_next_original: number;
};

/**
 * `enqueue_all_chapters` 入参:对 `transformation_novel` 下全部 chapter 入队
 * (后端从 `chapters` 表按 `data_asset_id` 拉全量 chapter_id)。
 */
export type EnqueueAllPayload = Omit<EnqueuePayload, 'chapter_ids'>;

/// 清洗预览结果。cleaned_text 给前端展示;lines_delta 为输出与输入的行数差
/// (规则折叠/合并短段 → 负数;加缩进不改行数 → 0;现有实现下几乎不会正)。
/// chars_delta 为字符数差(加缩进时为正,合并/折叠时可能为负)。
export interface CleaningPreview {
  cleaned_text: string;
  lines_delta: number;
  chars_delta: number;
}

/**
 * 后端 `prompts` 表行的前端镜像(取自 `nsc_core::models::Prompt`)。
 * `kind` 来自后端 `PromptKind` 枚举(`#[serde(rename_all = "snake_case")]`)
 * —— 前端拿到 / 发回 `"compress"` / `"style"`。
 * - `is_builtin` 为 true 的行在 UI 上不可编辑 / 不可删除,可"复制"成用户版。
 * - `archived = 1` 表示软删 —— 行仍保留供 `transformation_chapters.prompt_id` 反查历史 prompt 名称 / 模板。
 *   默认 list 不返回,需走 `list_prompts_including_archived`。
 */
export interface Prompt {
  id: number;
  name: string;
  kind: 'compress' | 'style';
  template: string;
  is_builtin: boolean;
  /** 0 = 正常,1 = 已归档(软删)。后端 INTEGER 到前端用 number 接收。 */
  archived: number;
}

/**
 * `upsert_prompt` 入参。`id === 0` 表示新建(走 insert);>0 表示更新(走 update)。
 * 字段保持 snake_case-by-default —— `kind` / `name` / `template` 都是单词,
 * 没有 `#[serde(rename_all)]` 在这层 DTO 上,所以前端按字段名原样发。
 * - 排除 `is_builtin` —— 后端不通过此 DTO 改 builtin 标记。
 * - 排除 `archived` —— 软删走 `delete_prompt` / `restore_prompt` 专用命令。
 */
export type PromptInput = Omit<Prompt, 'id' | 'is_builtin' | 'archived'> & { id: number };

/// ai_call_logs 表前端镜像,详见 migrations/0018_ai_call_logs.sql。
/// - business = transform_chapter | test_model(看两条 AI 调用路径)
/// - preview 字段是前 10KB,完整内容看 transformation_chapters.result_content / 调用方上下文
/// - estimated_tokens_in 用 chars/2 启发式(zh-aware 粗估),UI 标注粗估
export type AiCallBusiness = "transform_chapter" | "test_model" | "regenerate_preview";
export type AiCallStatus = "success" | "failed";

export interface AiCallLog {
  id: number;
  created_at: string;
  business: AiCallBusiness;
  context_type: string | null;
  context_id: number | null;
  model_config_id: number | null;
  model_name: string;
  base_url: string;
  temperature: number | null;
  max_tokens: number | null;
  system_preview: string | null;
  user_preview: string | null;
  system_size: number;
  user_size: number;
  estimated_tokens_in: number | null;
  actual_tokens_in: number | null;
  actual_tokens_out: number | null;
  status: AiCallStatus;
  response_preview: string | null;
  response_size: number;
  latency_ms: number;
  error: string | null;
  /// 产出比例越出护栏时的说明;null = 正常(或本次调用无可比输入,如 test_model)。
  /// 纯量测:不据此判失败,由用户决定是否重跑(见 nsc-core transformer::ratio_guard)。
  ratio_note: string | null;
}

/** list_ai_call_logs 入参 —— 后端 snake_case DTO,字段保持 Rust 原名。 */
export type AiCallLogFilter = {
  business?: AiCallBusiness | null;
  model_config_id?: number | null;
  status?: AiCallStatus | null;
  limit?: number | null;
  /// 跳过行数(>=0)。传统 OFFSET 翻页,UI "第 N 页"导航。
  offset?: number | null;
};

/// list_ai_call_logs 返回包装。后端 snake_case。total 是同 filter 下的总行数,
/// 供 UI 计算 "共 N 条 / 共 X 页"。
export interface AiCallLogPage {
  logs: AiCallLog[];
  total: number;
}

/// 单章节预览草稿状态(spec §4 / §5.3)。后端 serde snake_case。
export type PreviewStatus = 'generating' | 'done' | 'failed';

/// 单章节预览行(spec §5.3)—— 后端 nsc_core::models::ChapterPreviewRow,IPC 直接复用。
/// `created_at` / `updated_at` 是 RFC3339 字符串(DateTime<Utc> serde 自动转)。
export interface ChapterPreviewRow {
  id: number;
  batch_id: number;
  chapter_id: number;
  custom_input: string | null;
  preview_content: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
  error: string | null;
  status: PreviewStatus;
  created_at: string;
  updated_at: string;
}

/// 提交预览入参(spec §4.2 / §5.2)—— 后端 `CommitPreviewInput`,snake_case DTO。
export interface CommitPreviewInput {
  batch_id: number;
  chapter_id: number;
  draft_content: string;
  source_preview_id: number | null;
}

/// 发起预览生成入参(spec §5.2) —— 注意是 IPC 参数,后端命令签名是直接展开的
/// (regenerate_chapter_preview(batch_id, chapter_id, custom_input)),
/// 所以 wrapper 里要用展开式 invoke 而非内嵌 payload。
export interface RegeneratePreviewInput {
  batch_id: number;
  chapter_id: number;
  custom_input: string | null;
}

/** 上传删除前的确认信息。删 upload 不联动删 data_asset，仅提示以供用户另行去处理。 */
export interface UploadDeletePreviewItem {
  id: number;
  title: string;
  chapters_count: number;
  tn_count: number;
}
export interface UploadDeletePreview {
  upload_id: number;
  filename: string;
  source_filename: string;
  derived_data_assets: UploadDeletePreviewItem[];
}


/**
 * 总览页(Overview.vue)单次拉取的整张关系图。
 * 严格只画 4 类正向边(OverviewEdgeKind);`source_data_asset_id` 这种回溯字段不进图,
 * 只在节点 `subtitle` 里展示。
 */
export type OverviewNodeKind =
  | 'upload'
  | 'source_data_asset'
  | 'promoted_data_asset'
  | 'transformation_novel'
  | 'batch';

export interface OverviewNode {
  id: number;
  /** 前端 vue-flow `id` 字段:形如 `upload:1` / `da:7` / `tn:3` / `batch:42`。 */
  key: string;
  kind: OverviewNodeKind;
  title: string;
  word_count: number | null;
  chapter_count: number | null;
  child_count: number | null;
  /** 仅 `batch` 有:pending/running/paused/stopped/completed/terminated/cancelled。 */
  status: string | null;
  /** 仅 `upload` 有:文件字节数(原始 i64),前端 formatSize 渲染成 B/KB/MB。 */
  byte_size: number | null;
  /** DA:回溯来源("由 batch 42 生成")。 */
  subtitle: string | null;
  /** 仅 `batch` 有:所属 transformation_novel.id,前端点击跳转 `/library/transformation/:tnId`。 */
  tn_id: number | null;
}

export type OverviewEdgeKind =
  | 'upload_to_source_da'
  | 'upload_to_promoted_da'
  | 'da_to_tn'
  | 'tn_to_batch'
  | 'batch_to_promoted_da';

export interface OverviewEdge {
  source: string;
  target: string;
  kind: OverviewEdgeKind;
}

export interface OverviewStats {
  upload_count: number;
  data_asset_count: number;
  transformation_novel_count: number;
  /** running + paused 计数。 */
  running_batch_count: number;
  /** 最近 24h 失败的 batch 数。 */
  failed_recent_count: number;
}

export interface OverviewGraph {
  nodes: OverviewNode[];
  edges: OverviewEdge[];
  stats: OverviewStats;
  /** 当前节点总数(未截断)。 */
  total_nodes_raw: number;
  truncated: boolean;
}

/// 「新建工作流」试运行区 IPC 入参(spec §5.1)。
export interface PreviewFirstChapterInput {
  tn_id: number;
  chapter_id: number;
  prompt_id: number;
  model_config_id: number;
  /// true 时取最近 1 章原文作前文参考;false 时不取(对应 ctx_prev_original=0, ctx_prev_transformed=0)。
  include_prev: boolean;
  /// true 时取最近 1 章原文作后文参考;false 时不取(对应 ctx_next_original=0)。
  include_next: boolean;
  /// 「附加指令」(本期 UI 不暴露,留 TODO)。非空字符串时拼到 system prompt 文末。
  custom_input: string | null;
}

/// `preview_first_chapter` 出参(spec §5.1)。
/// `tokens_*` 为 null = 该 provider 未返回 usage(UI 显示 "—"),不是失败。
export interface PreviewFirstChapterOutput {
  content: string;
  tokens_in: number | null;
  tokens_out: number | null;
}

/// 「新建工作流」试运行区可选项（spec 2026-09-01）。
/// 后端 nsc_core::models::FirstChapterSeed + SeedSource。
/// IPC 字段名 preview_first_chapter 保留，类型从必填改 nullable。
export interface FirstChapterSeed {
  content: string;
  source: FirstChapterSeedSource;
}

/// 区分 LLM 出 vs 手写。手写时 tokens_in/out 都是 0,语义"无 LLM 调用"。
/// llm 分支的 null 表示 provider 未返回 usage —— 与 manual 的 0 语义不同,不要合并。
export type FirstChapterSeedSource =
  | { kind: 'llm'; tokens_in: number | null; tokens_out: number | null }
  | { kind: 'manual' };

/// `append_chapters_to_batch` 入参。
export type AppendChaptersToBatchPayload = {
  batchId: number;
  chapterIds: number[];
};

/// `append_chapters_to_batch` 返回。
export interface AppendChaptersResult {
  batch_id: number;
  added_tc_ids: number[];
}
