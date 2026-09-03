//! promotion_word_count —— TDD RED for `PromotionRepo::create_promoted_from_workflow`.
//!
//! ## Bug under test
//! `create_promoted_from_workflow` 把每个 promoted chapter 的 `word_count` 写成
//! `c.word_count` —— 即源 chapter 的 word_count,无视 `tc.status='done'` 时 body
//! 已经替换成了 `wrc.content`(转换后文本,长度一般跟源不同)。
//!
//! 转正章节的 `word_count` 应该跟它落库的 `body` 同步。`source_kind='transformed'`
//! 的章节,body 来自 `wrc.content`,所以 `word_count` 应该是 `word_count(wrc.content)`。
//!
//! ## 本测试做什么
//! 1. 建 1 个 upload → 1 个 data_asset → 3 个源章节,body 长度 100/200/300 chars(全
//!    中文,word_count == char_count 避开 word_count 与字节数口径偏差,断言更清晰)。
//! 2. 建 1 transformation_novel + 1 batch,3 个 transformation_chapters 全 done。
//! 3. `workflow_results` 写 3 个 `wrc.content`,长度 30/40/50 chars —— 跟源明显不同。
//! 4. 把 batch 标 `stopped`,调 `create_promoted_from_workflow`。
//! 5. 读回 promoted data_asset 的 3 个 chapter,断言:
//!    - `word_count` 等于 `word_count(wrc.content)`,**不**等于 `word_count(c.body)`
//!    - `source_kind == "transformed"`(确认走到了 transformed 分支,而非 original)
//!    - `body` 等于 `wrc.content`
//!
//! ## 期望结果
//! 失败:`word_count` 仍是源 chapter 的(100/200/300),不是转换后的(30/40/50)。
//! `assert_eq!` 会明确打印"chapter N: word_count X should equal Y";
//! `assert_ne!` 是 anti-regression 防呆:就算断言写错,也能明确告知"promote 没更新它"。
//!
//! ## 不修改任何源码
//! 本文件只放新测试,不动 `crates/nsc-core/src/db/repo/promotion.rs`。

use nsc_core::db::Db;
use nsc_core::models::batch::{BatchStatus, NewBatch, OnFailurePolicy};
use nsc_core::models::prompt::PromptKind;
use nsc_core::text::word_count;

/// 构造 1 个可被 promote 的 stopped workflow + 3 个 done 的 transformation_chapters。
///
/// - 源 chapter.body 用全中文(汉字算 char_count==word_count,口径稳定)
/// - wrc.content 用全中文,长度 30/40/50(与源 100/200/300 区分明显)
///
/// 返回 (db, batch_id, originals, transformeds),transformeds 与 originals 长度差
/// 很大,任何 word_count==original 的实现都能被这条断言抓出来。
fn setup_promotable_workflow() -> (Db, i64, Vec<String>, Vec<String>) {
    let db = Db::open_in_memory().unwrap();

    let originals: Vec<String> = vec![
        "原".repeat(100),
        "原".repeat(200),
        "原".repeat(300),
    ];
    let transformeds: Vec<String> = vec![
        "转".repeat(30),
        "转".repeat(40),
        "转".repeat(50),
    ];

    let upload_id = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "f.txt".into(),
        byte_size: 600,
        file_path: String::new(),
        original_text: originals.concat(),
        word_count: word_count(&originals.concat()) as i64,
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id,
        title: "源 DA".into(),
        source_filename: "f.txt".into(),
        ..Default::default()
    }).unwrap();

    let mut chapter_ids = Vec::new();
    for (i, body) in originals.iter().enumerate() {
        let cid = db.chapters().insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: (i + 1) as i32,
            title: format!("第{}章", i + 1),
            body: body.clone(),
            word_count: word_count(body) as i64 as i32,
            ..Default::default()
        }).unwrap();
        chapter_ids.push(cid);
    }

    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();

    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("w1".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
        prompt_id: 0,
        model_config_id: 0,
        mode: "compress".into(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
    }).unwrap();

    // 3 个 transformation_chapters,全 done
    let mut tc_ids = Vec::new();
    for (i, &chapter_id) in chapter_ids.iter().enumerate() {
        let tc_id = db.transformation_chapters().insert(&nsc_core::models::NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id,
            mode: PromptKind::Compress,
            prompt_id: 0,
            model_config_id: 0,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            batch_id: Some(batch_id),
            style_ref_chapter_id: None,
        }).unwrap();
        tc_ids.push(tc_id);
        // wrc.content 必须写上,否则 promote 会因为"data corruption: done 但 wrc NULL"失败
        db.workflow_results().create_for_batch_with_slots(batch_id, &chapter_ids).unwrap();
        db.workflow_results().write_content_by_chapter(batch_id, chapter_id, &transformeds[i]).unwrap();
        // mark_done 的 result_content 内部用 NULLIF(空),传非空就行,跟 wrc.content 一致
        db.transformation_chapters().mark_done(tc_id, transformeds[i].clone(), 1, 1).unwrap();
    }
    db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();

    (db, batch_id, originals, transformeds)
}

#[test]
fn transformed_promoted_chapter_word_count_reflects_post_transform_body() {
    let (db, batch_id, originals, transformeds) = setup_promotable_workflow();

    // Sanity:fixture 的 original / transformed 长度差很大 —— 抓错 body 一定会翻车
    for i in 0..3 {
        let ow = word_count(&originals[i]) as i32;
        let tw = word_count(&transformeds[i]) as i32;
        assert_ne!(
            ow, tw,
            "fixture 设计失败:original[{i}] 与 transformed[{i}] 长度都是 {ow}"
        );
    }

    // 执行 promote
    let new_da_id = db
        .promotion()
        .create_promoted_from_workflow(batch_id, "Promoted Test".into())
        .expect("promote 应该成功 —— batch=stopped、3 tc 全 done 且 wrc.content 已写");

    // 读回 promoted da 的章节,按 idx 排序
    let promoted = db.chapters().list_by_data_asset(new_da_id).unwrap();
    assert_eq!(promoted.len(), 3, "promoted da 应有 3 章");

    for (i, p) in promoted.iter().enumerate() {
        let expected_wc = word_count(&transformeds[i]) as i32;
        let original_wc = word_count(&originals[i]) as i32;

        // 1. 核心断言:word_count 必须等于转换后正文的字数
        assert_eq!(
            p.word_count, expected_wc,
            "chapter idx={}: word_count={} 应等于转换后正文的字数 {} \
             (transformed body len={}, original body len={})",
            p.idx, p.word_count, expected_wc,
            transformeds[i].chars().count(),
            originals[i].chars().count(),
        );

        // 2. Anti-regression:word_count 不应该还是原章节的
        //    万一上面 assert_eq! 写错,这条仍然能抓住 "promote 没更新它" 这件事
        assert_ne!(
            p.word_count, original_wc,
            "chapter idx={}: word_count={} 还等于原章节的 {} —— \
             promote 没有按转换后正文更新 word_count",
            p.idx, p.word_count, original_wc,
        );

        // 3. 走到 transformed 分支的旁证
        assert_eq!(
            p.source_kind, "transformed",
            "chapter idx={}: source_kind={} 应为 transformed",
            p.idx, p.source_kind,
        );
        assert_eq!(
            p.body, transformeds[i],
            "chapter idx={}: body 不等于 wrc.content",
            p.idx,
        );
    }
}