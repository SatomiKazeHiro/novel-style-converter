use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use nsc_core::ai::{AiProvider, OpenAiProvider};
use nsc_core::db::Db;
use nsc_core::recorder::{spawn_writer, AiCallRecorder, ChannelRecorder};
use nsc_core::catalog::{parse_close_thinking_models, BUNDLED_CATALOG_JSON, CatalogStore};
use tauri::Manager;
use nsc_core::transformer::{BatchScheduler, JobQueue, Notifier};

mod commands;

/// 启动时构造 CatalogStore 并注入 Tauri state。
fn build_catalog_store(app: &tauri::AppHandle) -> Result<Arc<CatalogStore>, String> {
    commands::catalog::build_store(app)
}

fn data_db_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("novel-style-converter").join("data.db")
}

pub fn run() {
    let _ = dotenvy::dotenv();

    let path = data_db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = Db::open(&path).expect("failed to open db");
    nsc_core::startup_recovery::run(&db.lock())
        .expect("startup safe-recovery failed");
    nsc_core::startup_cleanup::run(&db.lock())
        .expect("startup cleanup failed");
    // builtin prompt 同步 —— 启动期把 builtin.rs 源码的 template 同步到 DB,
    // 让用户在 UI 里看到的 builtin 模板跟源码一致(否则改了 builtin.rs 不会生效,
    // 历史上 0014/0028 的 migration 都是为了同样目的 —— 改 seed 之后就不再需要为每次模板改动写 migration)。
    // 见 db/repo/prompt.rs::seed_builtin_if_empty:count=0 INSERT,count>0 UPDATE。
    db.seed_builtin_prompts()
        .expect("seed builtin prompts failed");

    // ── AI 调用 recorder ────────────────────────────────────────────────────
    // Channel 容量 4096;满时 drop new(不阻塞 hot path)。Writer 任务按 db_path 重开
    // DB 落库,worker 线程不持 DB 句柄,避免跨线程 Send/Sync 摩擦。
    // spawn_writer 自己 std::thread::spawn + 内建 tokio current_thread runtime,
    // 不依赖调用方线程是否在 tokio reactor(在 run() 是 builder 同步阶段,.run() 之前
    // 没有 reactor,直接 tokio::spawn 会 panic "there is no reactor running")。
    // handle 留着 —— app 退出时 sender 被 drop → channel 关闭 → recv 返回 None
    // → loop break,最后几行日志能落完。不主动 abort,避免截断。
    let (recorder, rx) = ChannelRecorder::new(4096);
    let _writer_handle = spawn_writer(db.clone(), recorder.clone(), rx);
    let recorder: Arc<dyn AiCallRecorder> = Arc::new(recorder);
    // 启动期解析 bundled catalog 得到 已知可关思考 的 model_id 集合。
    // 转换业务调 AI 时(transformer / test_model)如果 model_config.model 在该集合内 ——
    // 尽力塞 reasoning_effort:"none";否则不塞。catalog 拖入 / 远端更新需要重启应用生效。
    let close_thinking: Arc<HashSet<String>> = Arc::new(parse_close_thinking_models(BUNDLED_CATALOG_JSON));

    fn make_provider(cfg: &nsc_core::models::ModelConfig) -> Box<dyn AiProvider> {
        Box::new(
            OpenAiProvider::new(cfg.base_url.clone(), cfg.api_key.clone())
                .unwrap_or_else(|_| {
                    OpenAiProvider::new(cfg.base_url.clone(), String::new())
                        .expect("fallback openai")
                }),
        )
    }

    let db_for_workers = db.clone();
    let job_queue = Arc::new(JobQueue::new(
        2,
        move || Ok(db_for_workers.clone()),
        make_provider,
        recorder.clone(),
        close_thinking.clone(),
    ));
    let scheduler = Arc::new(BatchScheduler::new(
        db.clone(),
        job_queue.clone(),
        Arc::new(make_provider),
        recorder.clone(),
        close_thinking.clone(),
    ));
    {
        let sched = scheduler.clone();
        let notify: Notifier = Arc::new(move |tid, success, error, content| {
            if !success && error.is_none() { return; }
            let res = if success {
                sched.on_chapter_done(tid, content)
            } else {
                sched.on_chapter_failed(tid, error.unwrap_or_default())
            };
            if let Err(e) = res {
                eprintln!("[BatchScheduler] notify 澶勭悊澶辫触: {e}");
            }
        });
        job_queue.set_notifier(notify);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(db)
        .manage(job_queue)
        .manage(scheduler)
        .manage(recorder)
        .manage(close_thinking)
        .setup(|app| {
        let store = build_catalog_store(app.handle())
            .map_err(|e| format!("catalog store 构建失败: {e}"))?;
        app.manage(store);
        Ok(())
    })
        .invoke_handler(tauri::generate_handler![
            commands::models::list_models,
            commands::models::list_models_including_archived,
            commands::models::upsert_model,
            commands::models::delete_model,
            commands::models::restore_model,
            commands::models::test_model,
            commands::uploads::list_uploads,
            commands::uploads::upload_file,
            commands::uploads::preview_upload_deletion,
            commands::uploads::delete_upload,
            commands::uploads::get_upload,
            commands::uploads::get_upload_text,
            commands::uploads::get_upload_text_chunk,
            commands::uploads::update_upload_text,
            commands::chapters::list_chapter_segments,
            commands::chapters::list_committed_segments,
            commands::chapters::list_chapters,
            commands::chapters::get_chapter_contents,
            commands::chapters::get_chapter,
            commands::chapters::update_chapter_body,
            commands::cleaning::preview_cleaning,
            commands::data_assets::list_data_asset_chapters,
            commands::data_assets::commit_data_asset,
            commands::data_assets::list_data_assets,
            commands::data_assets::find_data_asset_by_upload,
            commands::data_assets::delete_data_asset,
            commands::data_assets::promote_workflow,
            commands::data_assets::count_promoted_data_assets_by_workflow,
            commands::data_assets::list_promoted_data_assets_for_workflow,
            commands::data_assets::list_data_assets_by_upload,
            commands::transformation_novels::list_transformation_novels,
            commands::transformation_novels::get_transformation_novel,
            commands::transformation_novels::create_transformation_novel,
            commands::transformation_novels::update_transformation_novel,
            commands::transformation_novels::delete_transformation_novel,
            commands::transformations::list_transformation_chapters,
            commands::transformations::list_transformation_chapters_for_chapter,
            commands::transformations::enqueue_transformation_chapters,
            commands::transformations::enqueue_all_chapters,
            commands::transformations::append_chapters_to_batch,
            commands::prompts::list_prompts,
            commands::prompts::get_prompt,
            commands::prompts::upsert_prompt,
            commands::prompts::delete_prompt,
            commands::prompts::restore_prompt,
            commands::prompts::list_prompts_including_archived,
            commands::prompts::count_prompt_usage,
            commands::workflows::list_transformation_source_chapters,
            commands::workflows::create_workflow,
            commands::workflows::list_workflows,
            commands::workflows::get_workflow,
            commands::workflows::list_workflow_chapters,
            commands::workflows::stop_workflow,
            commands::workflows::retry_workflow_chapters,
            commands::catalog::catalog_status,
            commands::catalog::catalog_refresh,
            commands::catalog::catalog_import_drop,
            commands::catalog::catalog_read_active,
            commands::ai_call_logs::list_ai_call_logs,
            commands::ai_call_logs::get_ai_call_log,
            commands::ai_call_logs::clear_ai_call_logs,
            commands::util::open_external_url,
            commands::overview::get_overview_graph,
            commands::workflows::list_chapter_workflow_results,
            commands::workflows::regenerate_chapter_preview,
            commands::workflows::commit_chapter_preview,
            commands::workflows::preview_first_chapter,
            commands::workflows::list_chapter_previews,
            commands::workflows::discard_chapter_preview,            commands::workflows::delete_workflow,

        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}