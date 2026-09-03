//! `read_context` 中 `prev_tx` 切片顺序的回归测试 —— queue.rs。
//!
//! 旧实现:`prev_n(da, idx, 32)` 拉全部旧章(ASC),再用 `.take(ctx_prev_transformed)`
//! 取前 N —— 等于「最旧 N 章」。要求是「最近 N 章」,所以所有 idx >= 2 的章
//! 都把 idx=1 当作前文。
//!
//! 修正:把 `ctx_prev_transformed` 作为 SQL LIMIT,丢掉 `.take()`。
//! `prev_n` 内部已经 `ORDER BY idx DESC LIMIT n` + `v.reverse()`,
//! 所以 LIMIT=n 就直接返回「最近 N 章(升序)」,跟 `prev_orig` 路径一致。
use std::sync::Arc;

use nsc_core::db::Db;
use nsc_core::models::batch::{BatchStatus, NewBatch, OnFailurePolicy};
use nsc_core::models::prompt::PromptKind;
use nsc_core::models::{Chapter, ModelConfig, NewModelConfig, NewTransformationChapter, Prompt};
use nsc_core::transformer::queue::read_context;
use nsc_core::transformer::JobSpec;

/// 建 upload + data_asset + chapters 1..=n,返回 (Db, da_id, cids[i]=chapter_i.id)。
fn seed_chapters(n: i32) -> (Arc<Db>, i64, Vec<i64>) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let upload_id = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: format!("sha-{n}"),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();
    let mut cids = Vec::with_capacity(n as usize);
    for i in 1..=n {
        cids.push(db.chapters().insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: i,
            title: format!("chapter {i}"),
            body: format!("orig body {i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap());
    }
    (db, da_id, cids)
}

/// 落 batch + 真 prompt/model 行,再为指定章节插入 transformation_chapters 行。
/// 返回 (batch_id, prompt_id, model_id)。
fn setup_batch_with_tcs(
    db: &Db,
    da_id: i64,
    prompt_id: i64,
    model_id: i64,
    cids: &[i64],
) -> i64 {
    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();
    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("ctx-prev-test".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
        prompt_id,
        model_config_id: model_id,
        mode: "compress".into(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
    }).unwrap();
    db.batches().set_status(batch_id, BatchStatus::Running).unwrap();
    let b = db.batches().get(batch_id).unwrap().unwrap();
    for &cid in cids {
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: b.transformation_novel_id,
            chapter_id: cid,
            mode: PromptKind::Compress,
            prompt_id,
            model_config_id: model_id,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            batch_id: Some(batch_id),
            style_ref_chapter_id: None,
        }).unwrap();
    }
    batch_id
}

fn make_prompt(id: i64) -> Prompt {
    Prompt {
        id,
        name: "test".into(),
        kind: PromptKind::Compress,
        template: "{{chapter}}".into(),
        is_builtin: false,
        archived: 0,
    }
}

fn make_model_config(id: i64) -> ModelConfig {
    ModelConfig {
        id,
        name: "Test".into(),
        base_url: "http://localhost".into(),
        api_key: "x".into(),
        model: "test-model".into(),
        max_tokens: None,
        max_context: Some(8000),
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
        archived: 0,
    }
}

/// 主回归测试:ctx_prev_transformed=1 时,read_context 必须把 idx=2(最近一邻章)
/// 的已写入 transformed content 作为 prev_tx 的第一项,而不是 idx=1(最旧)。
#[test]
fn prev_tx_picks_most_recent_neighbor_not_oldest() {
    let (db, da_id, cids) = seed_chapters(5);

    // chapters 1..=5: idx=1..5。当前章 = ch3 (idx=3),prev 候选 = ch1, ch2。
    let prompt_id = db.prompts().insert(&make_prompt(0)).unwrap();
    let model_id = db.model_configs().insert(&NewModelConfig {
        name: "Test".into(),
        base_url: "http://localhost".into(),
        api_key: "x".into(),
        model: "test-model".into(),
        max_tokens: None,
        max_context: Some(8000),
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
    }).unwrap();

    // 为 ch1、ch2、ch3 建 tc 行,共享同一个 batch。
    let batch_id = setup_batch_with_tcs(&db, da_id, prompt_id, model_id, &cids[0..=2]);

    // 只 ch1、ch2 标 done 并落结果槽;ch3 不标(它就是当前章)。
    for &cid in &cids[0..=1] {
        let tcs = db.transformation_chapters().list_by_chapter(cid).unwrap();
        assert_eq!(tcs.len(), 1);
        db.transformation_chapters().mark_done(tcs[0].id, String::new(), 0, 0).unwrap();
    }

    // 给 batch 建结果集 + ch1/ch2 槽(空),然后写入正文。
    db.workflow_results().create_for_batch_with_slots(batch_id, &cids[0..=2]).unwrap();
    db.workflow_results().write_content_by_chapter(batch_id, cids[0], "TX-1-from-chapter-1").unwrap();
    db.workflow_results().write_content_by_chapter(batch_id, cids[1], "TX-2-from-chapter-2").unwrap();
    // 故意不给 ch3 (cids[2]) 写内容 —— 它是当前章,本来就不会被 prev_n 命中。

    let tn_id = db.transformation_novels().list_by_data_asset(da_id).unwrap()[0].id;

    // 拿 ch3 实体当 JobSpec.chapter。
    let ch3: Chapter = db.chapters().get(cids[2]).unwrap().unwrap();
    let job = JobSpec {
        tc_id: db.transformation_chapters().list_by_chapter(cids[2]).unwrap()[0].id,
        tn_id,
        mode: PromptKind::Compress,
        chapter: ch3,
        prompt: make_prompt(prompt_id),
        model_config: make_model_config(model_id),
        ctx_prev_original: 0,
        ctx_prev_transformed: 1, // 关键:只取最近 1 章
        ctx_next_original: 0,
    };

    let prep = read_context(&db, &job).expect("read_context ok");

    // 旧 bug 路径:会先拿到 ch1 的 transformed("TX-1-...")作为唯一候选。
    // 修正后:只取最近 1 章 = ch2 → "TX-2-..."。
    assert_eq!(prep.prev_tx.len(), 1,
        "ctx_prev_transformed=1 应只产 1 条 prev_tx,得到 {} 条", prep.prev_tx.len());
    let (title, content) = &prep.prev_tx[0];
    assert_eq!(content, "TX-2-from-chapter-2",
        "prev_tx[0] 必须是最近邻章 ch2 的 transformed 正文");
    assert_eq!(title, "chapter 2");
}

/// ctx_prev_transformed=2 时,prev_tx 应是 [ch2, ch3] 的顺序(最近优先)。
/// 当前章 = ch4(idx=4),prev 候选 = ch1, ch2, ch3 共 3 章。
/// 旧 bug 路径会拿 [ch1, ch2](最旧优先,因为 LIMIT=32 后 .take(2) 砍尾)。
/// 修正后:prev_n(..., 2) 直接取最近 2 章,ASC 后是 [ch2, ch3]。
#[test]
fn prev_tx_orders_recent_first_ascending() {
    let (db, da_id, cids) = seed_chapters(6);

    let prompt_id = db.prompts().insert(&make_prompt(0)).unwrap();
    let model_id = db.model_configs().insert(&NewModelConfig {
        name: "Test".into(),
        base_url: "http://localhost".into(),
        api_key: "x".into(),
        model: "test-model".into(),
        max_tokens: None,
        max_context: Some(8000),
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
    }).unwrap();
    // tc 行 ch1..=3 + 当前章 ch4 (idx=4)
    let batch_id = setup_batch_with_tcs(&db, da_id, prompt_id, model_id, &cids[0..=3]);

    for &cid in &cids[0..=2] {
        let tcs = db.transformation_chapters().list_by_chapter(cid).unwrap();
        db.transformation_chapters().mark_done(tcs[0].id, String::new(), 0, 0).unwrap();
    }
    db.workflow_results().create_for_batch_with_slots(batch_id, &cids[0..=3]).unwrap();
    db.workflow_results().write_content_by_chapter(batch_id, cids[0], "TX-1").unwrap();
    db.workflow_results().write_content_by_chapter(batch_id, cids[1], "TX-2").unwrap();
    db.workflow_results().write_content_by_chapter(batch_id, cids[2], "TX-3").unwrap();

    let tn_id = db.transformation_novels().list_by_data_asset(da_id).unwrap()[0].id;
    // 当前章 = ch4 (idx=4, cids[3])
    let ch4: Chapter = db.chapters().get(cids[3]).unwrap().unwrap();
    let job = JobSpec {
        tc_id: db.transformation_chapters().list_by_chapter(cids[3]).unwrap()[0].id,
        tn_id,
        mode: PromptKind::Compress,
        chapter: ch4,
        prompt: make_prompt(prompt_id),
        model_config: make_model_config(model_id),
        ctx_prev_original: 0,
        ctx_prev_transformed: 2,
        ctx_next_original: 0,
    };

    let prep = read_context(&db, &job).expect("read_context ok");
    assert_eq!(prep.prev_tx.len(), 2);
    assert_eq!(prep.prev_tx[0].1, "TX-2", "prev_tx[0] 必须是最近邻章 ch2");
    assert_eq!(prep.prev_tx[1].1, "TX-3", "prev_tx[1] 必须是上一邻章 ch3 —— bug 路径会拿到 TX-1");
}

/// ctx_prev_transformed=0 时,read_context 应完全跳过 prev_n 查询,直接产空 vec。
/// 旧实现不管 0 与否都会跑 SQL(虽然 .take(0) 也产空 vec,但多一次查询)。
#[test]
fn prev_tx_zero_count_skips_query() {
    let (db, da_id, cids) = seed_chapters(3);

    let prompt_id = db.prompts().insert(&make_prompt(0)).unwrap();
    let model_id = db.model_configs().insert(&NewModelConfig {
        name: "Test".into(),
        base_url: "http://localhost".into(),
        api_key: "x".into(),
        model: "test-model".into(),
        max_tokens: None,
        max_context: Some(8000),
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
    }).unwrap();
    // 不用建 batch/tc,因为 take=0 会短路。

    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();

    let ch3: Chapter = db.chapters().get(cids[2]).unwrap().unwrap();
    let job = JobSpec {
        tc_id: 0,
        tn_id,
        mode: PromptKind::Compress,
        chapter: ch3,
        prompt: make_prompt(prompt_id),
        model_config: make_model_config(model_id),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    };

    let prep = read_context(&db, &job).expect("read_context ok");
    assert!(prep.prev_tx.is_empty(),
        "ctx_prev_transformed=0 应产空 prev_tx,得到 {:?}", prep.prev_tx);
}