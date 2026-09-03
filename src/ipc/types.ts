// 瀛楁鍛藉悕绾﹀畾:
// - 鍝嶅簲绫诲瀷(鍚庣 `#[derive(Serialize)]` 鐨勫疄浣?濡?`ModelConfig` / `Upload`)
//   淇濇寔 snake_case,涓?`crates/nsc-core/src/models/*.rs` 涓€涓€瀵瑰簲銆?
// - IPC 鍏ュ弬鐨?*鍛戒护鍙傛暟鍚?(`payload` / `id` 绛?鐢?Tauri 鑷姩 camelCase 鍖?
//   鎵€浠ュ灞?`invoke('cmd', { ... })` 鐨?key 鐢?camelCase銆?
// - IPC 鍏ュ弬鐨勫唴灞?DTO(鍚庣鏄惧紡 `#[serde(rename_all="snake_case")]` 鐨?
//   蹇呴』鎸?snake_case 鍘熸牱鍙?鍓嶇 wrapper 涓嶈鍋氫换浣?inline 鏀瑰悕銆?
// 瀛楁鍙樻洿蹇呴』鍚屾淇敼鍚庣 DTO + 鏈枃浠?+ commands.ts 涓搴?wrapper銆?

/**
 * 鍚庣 `model_configs` 琛岀殑鍓嶇闀滃儚銆?
 * - `api_key` 鏄庢枃瀛?SQLite,鍓嶇鎷垮埌涔熻鍘熸牱鍥炰紶(涓嶈鑴辨晱 鈥斺€?鎻愪氦鏃朵粛闇€瑕佺湡瀹炲€?
 * - `concurrency` 鏄?per-model 骞跺彂涓婇檺:worker 绔寜 `model_config_id` 鍏变韩淇″彿閲忋€?
 *   鏈夋晥鑼冨洿 [1,16];瓒呰繃鐗╃悊 worker 鏁?榛樿 2)涓嶄細瑙﹀彂棰濆闃诲,浣嗕笅闄?1 闃叉 0 姝婚攣銆?
 * - `archived = 1` 琛ㄧず杞垹(API key 宸茶娓呯┖);浠嶄繚鐣欏湪 list 鍝嶅簲閲屼緵 UI 灞曠ず鍘嗗彶 model銆?
 * - 瀛楁鍏ㄩ儴 snake_case 鏉ヨ嚜鍚庣 serde(鍚庣**涓?*鍋?rename)
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
 * `upsert_model` / `test_model` 鍏ュ弬:`id === 0` 琛ㄧず鏂板缓,鍚﹀垯鎸?id 鏇存柊銆?
 * 杩欐槸鍚庣 snake_case DTO(鍐呭眰瀛楁鍘熸牱鍙?涓嶈 inline 鏀瑰悕)銆?
 */
export type ModelConfigInput = Omit<ModelConfig, 'id' | 'archived'> & { id: number };

/**
 * `test_model` 缁撴瀯鍖栬繑鍥烇細
 * - 鎴愬姛锛歚content_preview` 濉搷搴斿墠 200 瀛楃锛宍tokens_in/out` 鏉ヨ嚜 provider usage銆?
 * - 澶辫触锛歚error` 濉畬鏁村瓧绗︿覆锛坧rovider 鍒涘缓澶辫触 / 闈?2xx / 绌?choices / 缂?usage 閮戒細鍐欙級锛?
 *   `content_preview` 涓?tokens 鍏ㄤ负 null銆?
 * - 浠绘剰璺緞閮戒細濉?`latency_ms`锛堝垱寤?provider 澶辫触涔熻瓒呮椂锛夈€?
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

/// State 1: 鍘熷涓婁紶鏂囦欢鍏冩暟鎹€備笉鍚珷鑺傜粨鏋?绔犺妭鍦?data_assets)銆?
export interface UploadSummary {
  id: number;
  sha256: string;
  filename: string;
  byte_size: number;
  uploaded_at: string;
  file_path: string;
  /// zh-aware 瀛楁暟(姹夊瓧 + 瀛楁瘝 + 鏁板瓧),upload_file 鏃跺悗绔竴娆＄畻濂姐€?
  word_count: number;
}

/// 鏁版嵁璧勪骇绫诲瀷 鈥?source 鏄師濮嬭В鏋愪骇鐗?promoted 鏄粠宸ヤ綔娴佺粨鏋滄淳鐢熺殑鏂拌祫浜с€?
export type DataAssetKind = 'source' | 'promoted';

/// 鍗曟潯 data_asset 鍏冩暟鎹?渚?promote_workflow / list_data_assets_by_upload 绛夎繑鍥?銆?
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

/// State 2: 涓€娆¤В鏋愮粨鏋?= 涓€浠?data_asset + 涓€缁勫垎绔犺妭鍒囩墖銆?
export interface DataAssetSummary {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  source_filename: string;
  tn_count: number;
}

/// Library.vue "鏁版嵁璧勪骇" tab 琛?data_asset 鍏冩暟鎹?+ 鏉ユ簮 upload 鏂囦欢鍚?+ 绔犺妭鎬诲瓧鏁般€?
export interface DataAssetRow {
  id: number;
  upload_id: number;
  title: string;
  parsed_at: string;
  filename: string;
  byte_size: number;
  /// SUM(chapters.word_count) WHERE data_asset_id = da.id銆?
  word_count: number;
  /// COUNT(transformation_novels.id) WHERE data_asset_id = da.id銆?
  tn_count: number;
  /// 璧勪骇绫诲瀷:source = 鍘熷瑙ｆ瀽;promoted = 浠庡伐浣滄祦缁撴灉娲剧敓銆?
  kind: DataAssetKind;
  /// 褰?kind=promoted 鏃?璁板綍婧?workflow(batch.id);source 鏃朵负 null銆?
  source_workflow_id: number | null;
  /// 褰?kind=promoted 鏃?璁板綍婧?data_asset.id;source 鏃朵负 null銆?
  source_data_asset_id: number | null;
  /// 鐢ㄦ埛澶囨敞銆?
  note: string;
  /// 娲剧敓鍑哄灏?promoted da(浠?source 绫诲瀷鏈夊€?promoted 绫诲瀷濮嬬粓 0)銆?
  promoted_count: number;
}

/// State 2 绔犺妭鍏冩暟鎹?浠?list_data_asset_chapters 杩斿洖)銆傛鏂囩敱鍓嶇鎸?byte 鍒囩墖 original_text銆?
export interface DataAssetChapter {
  id: number;
  idx: number;
  title: string;
  body: string;
  word_count: number;
  /// 绔犺妭鏉ユ簮:transformed = 宸ヤ綔娴佽浆鎹㈢粨鏋?original = 鍘熸枃(娲剧敓 da 澶辫触绔犺妭鍥為€€)銆?
  source_kind: 'transformed' | 'original';
  source_chapter_id: number | null;
  edited_at: string | null;
  /// 标题文本在 upload.original_text 里的 0-based 行号。null = 无原文坐标(promoted 章节)。
  title_line: number | null;
}

/// commit_data_asset 鍏ュ弬:title + 绔犺妭鍒楄〃(姣忎釜鍚?title + content + title_line)銆?
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
 * `get_chapter_contents` 杩斿洖:绔犺妭姝ｆ枃棰勮(棰勮椤电敤)銆傚唴瀹规槸鍚庣浠?
 * `uploads.original_text` 鎸?byte range 鍒囩墖鍚?鍓ラ琛屾爣棰樺啀 trim銆?
 */
export interface ChapterContentRow {
  idx: number;
  title: string;
  content: string;
}

/// 绔犺妭鍒囩墖瀹炰綋銆俠yte_start/byte_end 姘歌繙鍦?upload.original_text 鍧愭爣绯汇€?
export interface Chapter {
  id: number;
  data_asset_id: number;
  idx: number;
  title: string;
  body: string;
  word_count: number;
  /// 绔犺妭鏉ユ簮:transformed = 宸ヤ綔娴佽浆鎹㈢粨鏋?original = 鍘熸枃(娲剧敓 da 鐨勫け璐ョ珷鑺傚洖閫€)銆?
  source_kind: 'transformed' | 'original';
  /// 娲剧敓鏃舵寚鍚戞簮 chapter.id(鍙湪娲剧敓 da 閲屾湁鍊?銆?
  source_chapter_id: number | null;
  edited_at: string | null;
}

/**
 * `commit_data_asset` 鍏ュ弬鐨勭珷鑺傚厓绱?
 * 浠呮爣棰?+ byte 鑼冨洿銆傚悗绔寜 byte range 鍒囩墖鍘熸枃璁＄畻 `word_count` / `idx`銆?
 */
export type ChapterInput = {
  title: string;
  content: string;
  title_line: number;
};

/**
 * `list_transformation_novels` 杩斿洖:杞崲灏忚鍏冩暟鎹€?
 * `chapters_count` 鏄 `data_asset_id` 涓嬫墍鏈?chapters 鐨勬€绘暟,
 * 涓嶄唬琛ㄨ繖鏈?tn 瀹為檯鏈夊灏?transformation_chapter 琛屻€?
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
 * `create_transformation_novel` 鍏ュ弬:鍚庣 snake_case DTO,
 * 涓変釜榛樿瀛楁涓哄彲绌?鍐呭眰瀛楁鍘熸牱鍙?涓嶈 inline 鏀瑰悕銆?
 * 鍛藉悕鍔?Input 鍚庣紑,涓庡悗绔?`*Payload` 鍖哄垎,閬垮厤璺ㄨ瑷€鍚屽悕姝т箟銆?
 */
export interface CreateTransformationNovelInput {
  data_asset_id: number;
  title: string;
}

/**
 * `update_transformation_novel` 鍏ュ弬:鍚庣 snake_case DTO,涓変釜榛樿瀛楁鍙┖銆?
 * null 琛ㄧず娓呯┖瀛橀噺榛樿鍊?鍚庣 update 琛屼负:鐢?payload 瑕嗙洊 cur.default_*)銆?
 */
export interface UpdateTransformationNovelInput {
  id: number;
  title: string;
}

// === Workflow 宸ヤ綔娴?===
/**
 * 后端 `BatchStatus` 全部 7 值。前端必须穷举映射中文,否则 UI 会甩原始字符串。
 * - pending/running: 工作中。
 * - stopped: spec §3.3 收尾态,只能 retry 空槽,不再回 running。
 * - paused: 失败策略 = pause_and_review,批停在等用户决策(继续/终止/跳过)。
 * - completed/terminated/cancelled: batch 的最终终态,含义不可逆。
 */
export type WorkflowStatus = 'pending' | 'running' | 'stopped' | 'paused' | 'completed' | 'terminated' | 'cancelled';

/**
 * `list_workflows` / `get_workflow` 杩斿洖:宸ヤ綔娴佹眹鎬?+ 绔犺妭璁℃暟銆?
 * counts 鐩存帴宓屽湪琛屽唴 鈥斺€?涓嶇敤鍗曠嫭璋?count 鎺ュ彛銆?
 */
export interface PromoteWorkflowInput {
  batchId: number;
  title: string;
}

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

/** `list_workflow_chapters` 杩斿洖:tc 琛?+ 绔犺妭鏍囬/idx + 鍏宠仈缁撴灉妲介瑙堛€?*/
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

/** `list_transformation_source_chapters` 杩斿洖:tn 涓嬪叏閮ㄦ簮绔犺妭 + 闈炵┖缁撴灉鏁般€?*/
export interface SourceChapterRow {
  chapter_id: number;
  idx: number;
  title: string;
  word_count: number;
  non_empty_result_count: number;
}

/** `list_chapter_workflow_results` 杩斿洖:鏌愭簮绔犺妭鍦ㄦ墍鏈夊伐浣滄祦閲岀殑缁撴灉(鎸?batch_id DESC)銆?*/
export interface ChapterWorkflowResultRow {
  batch_id: number;
  batch_label: string | null;
  batch_status: WorkflowStatus;
  batch_ended_at: string | null;
  content: string | null;
  status: TransformStatus;
}

/** `create_workflow` 鍏ュ弬:鍚庣 snake_case DTO,鎵€鏈夊瓧娈靛繀濉?spec 搂5.1)銆?*/
/**
 * `create_workflow` 鍏ュ弬:鍚庣 snake_case DTO,鎵€鏈夊瓧娈靛繀濉?spec 搂5.1)銆?
 *
 * `on_failure_policy` 鏄珷鑺傚け璐ユ椂鐨勫鐞嗙瓥鐣?
 * - `pause_and_review`: 澶辫触鏃?batch 杞?Paused,绛夌敤鎴峰湪 modal 閲屾墜鍔ㄥ喅绛?閲嶈瘯/璺宠繃/缁堟)
 * - `skip_failed`:      澶辫触鏃惰绔犳爣 Skipped,缁х画娲句笅涓€绔?batch 鐣?Running)
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
 * `transformation_chapters.status` 鐘舵€佹満:
 * `pending` 鈫?`running` 鈫?(`done` | `failed` | `cancelled`)
 * 澶辫触涓嶈嚜鍔ㄩ噸璇?鈥?鐢ㄦ埛鎵嬪姩璋?`enqueue_transformation_chapters` 閲嶆帓闃熴€?
 */
export type TransformStatus = 'pending' | 'running' | 'done' | 'failed' | 'skipped' | 'cancelled';

/**
 * `list_transformation_chapters` / `list_transformation_chapters_for_chapter` 杩斿洖:
 * 涓€娆¤浆鎹换鍔＄殑瀹屾暣鐘舵€併€俙chapter_idx` / `chapter_title` 鏄?join `chapters` 琛ㄦ嫾涓婄殑,
 * 鏂逛究 Transform 椤电洿鎺ュ睍绀?鏃犻渶浜屾璇锋眰銆?
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
 * `enqueue_transformation_chapters` 鍏ュ弬銆備笁涓笂涓嬫枃鏁?
 * - `ctx_prev_original` 鈥斺€?妯℃澘 `{{prev_original}}` 鍗犱綅鐨勫墠鏂囧師鏂囩珷鏁?
 * - `ctx_prev_transformed` 鈥斺€?妯℃澘 `{{prev_transformed}}` 鍗犱綅鐨勫墠鏂囧凡杞崲绔犳暟
 * (鐢婚鍙傝€?涓嶆薄鏌撳師鏂囦笂涓嬫枃;鑻ュ墠闈㈡病鏈夊凡杞崲缁撴灉鍒欐覆鏌撲负 `(鏆傛棤宸茶浆鎹㈠弬鑰?`)
 * - `ctx_next_original` 鈥斺€?妯℃澘 `{{next_original}}` 鍗犱綅鐨勫悗鏂囧師鏂囩珷鏁?
 * 鍚庣鎸?(chapter_id, prompt_id, model_config_id) 鍚屾椂鍖归厤鎵嶈涓虹敾椋庡弬鑰冦€?
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
 * `enqueue_all_chapters` 鍏ュ弬:瀵?`transformation_novel` 涓嬪叏閮?chapter 鍏ラ槦
 * (鍚庣浠?`chapters` 琛ㄦ寜 `data_asset_id` 鎷夊叏閲?chapter_id)銆?
 */
export type EnqueueAllPayload = Omit<EnqueuePayload, 'chapter_ids'>;

/** `JobQueue` 鍐呴儴鐨?job 鐘舵€?涓?`TransformStatus` 鍚屽瓧闈㈤噺,浣嗚涔夊眰闈㈡湁鍒?
 * `TransformStatus` 鏄?DB 琛岀殑鎸佷箙鐘舵€?`JobStatus` 鏄?worker pool 鐨勫唴瀛樺揩鐓? */
export type JobStatus = 'pending' | 'running' | 'done' | 'failed' | 'cancelled';

/** 鍗曚釜 job 鐨勫疄鏃跺揩鐓с€傞攣浜夌敤鏃惰 job 鍙兘涓嶅嚭鐜板湪 snapshot 涓€?*/
export interface JobInfo {
  transformation_id: number;
  chapter_title: string;
  chapter_idx: number;
  status: JobStatus;
  error: string | null;
  tokens_in: number | null;
  tokens_out: number | null;
}

/** `JobQueue.snapshot()` 涓€娆℃媺鍥炵殑鍏ㄩ噺闃熷垪蹇収,鎸夌姸鎬佸垎鍥涚粍銆?
 *  閿佷簤鐢ㄦ椂杩斿洖绌?瀛楁閮戒负绌烘暟缁?,鍓嶇 1s 杞鍙笉澶勭悊銆?*/
export interface QueueSnapshot {
  pending: JobInfo[];
  running: JobInfo[];
  done: JobInfo[];
  failed: JobInfo[];
}

/// 娓呮礂棰勮缁撴灉銆俢leaned_text 缁欏墠绔睍绀?lines_delta 涓鸿緭鍑轰笌杈撳叆鐨勮鏁板樊
/// (瑙勫垯鎶樺彔/鍚堝苟鐭 鈫?璐熸暟;鍔犵缉杩涗笉鏀硅鏁?鈫?0;鐜版湁瀹炵幇涓嬪嚑涔庝笉浼氭)銆?
/// chars_delta 涓哄瓧绗︽暟宸?鍔犵缉杩涙椂涓烘,鍚堝苟/鎶樺彔鏃跺彲鑳戒负璐?銆?
export interface CleaningPreview {
  cleaned_text: string;
  lines_delta: number;
  chars_delta: number;
}

/**
 * 鍚庣 `prompts` 琛ㄨ鐨勫墠绔暅鍍?鍙栬嚜 `nsc_core::models::Prompt`)銆?
 * `kind` 鏉ヨ嚜鍚庣 `PromptKind` 鏋氫妇(`#[serde(rename_all = "snake_case")]`)
 * 鈥斺€?鍓嶇鎷垮埌 / 鍙戝洖 `"compress"` / `"style"`銆?
 * - `is_builtin` 涓?true 鐨勮鍦?UI 涓婁笉鍙紪杈?/ 涓嶅彲鍒犻櫎,鍙?澶嶅埗"鎴愮敤鎴风増銆?
 * - `archived = 1` 琛ㄧず杞垹 鈥斺€?琛屼粛淇濈暀渚?`transformation_chapters.prompt_id` 鍙嶆煡鍘嗗彶 prompt 鍚嶇О / 妯℃澘銆?
 *   榛樿 list 涓嶈繑鍥?闇€璧?`list_prompts_including_archived`銆?
 */
export interface Prompt {
  id: number;
  name: string;
  kind: 'compress' | 'style';
  template: string;
  is_builtin: boolean;
  /** 0 = 姝ｅ父,1 = 宸插綊妗?杞垹)銆傚悗绔?INTEGER 鍒?鍓嶇鐢?number 鏀躲€?*/
  archived: number;
}

/**
 * `upsert_prompt` 鍏ュ弬銆俙id === 0` 琛ㄧず鏂板缓(璧?insert);>0 琛ㄧず鏇存柊(璧?update)銆?
 * 瀛楁淇濇寔 snake_case-by-default 鈥斺€?`kind` / `name` / `template` 閮芥槸鍗曡瘝,
 * 娌℃湁 `#[serde(rename_all)]` 鍦ㄨ繖灞?DTO 涓?鎵€浠ュ墠绔寜瀛楁鍚嶅師鏍峰彂銆?
 * - 鎺掗櫎 `is_builtin` 鈥斺€?鍚庣涓嶉€氳繃姝?DTO 鏀?builtin 鏍囪銆?
 * - 鎺掗櫎 `archived` 鈥斺€?杞垹璧?`delete_prompt` / `restore_prompt` 涓撶敤鍛戒护銆?
 */
export type PromptInput = Omit<Prompt, 'id' | 'is_builtin' | 'archived'> & { id: number };

/// ai_call_logs 琛ㄥ墠绔暅鍍?璇﹁ migrations/0018_ai_call_logs.sql銆?
/// - business = transform_chapter | test_model(鐪嬩袱鏉?AI璋冪敤 璺緞)
/// - preview 瀛楁鏄墠 10KB,瀹屾暣鍐呭鐪?transformation_chapters.result_content / 璋冪敤鏂逛笂涓嬫枃
/// - estimated_tokens_in 鐢?chars/2 鍚彂寮?zh-aware 绮椾及),UI 鏍囨敞绮椾及
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
}

/** list_ai_call_logs 鍏ュ弬 鈥斺€?鍚庣 snake_case DTO,瀛楁淇濇寔 Rust 鍘熷悕銆?*/
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

/** 涓婁紶鍒犻櫎鍓嶇殑纭淇℃伅銆傚垹 upload 涓嶈仈鍔ㄥ垹 data_asset锛屼粎鎻愮ず浠ヤ緵鐢ㄦ埛鍙﹁鍘诲鐞嗐€?*/
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
export interface PreviewFirstChapterOutput {
  content: string;
  tokens_in: number;
  tokens_out: number;
}

/// 「新建工作流」试运行区可选项（spec 2026-09-01）。
/// 后端 nsc_core::models::FirstChapterSeed + SeedSource。
/// IPC 字段名 preview_first_chapter 保留，类型从必填改 nullable。
export interface FirstChapterSeed {
  content: string;
  source: FirstChapterSeedSource;
}

/// 区分 LLM 出 vs 手写。手写时 tokens_in/out 都是 0,语义"无 LLM 调用"。
export type FirstChapterSeedSource =
  | { kind: 'llm'; tokens_in: number; tokens_out: number }
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
