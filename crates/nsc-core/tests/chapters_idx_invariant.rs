//! 防御性测试:`chapters.idx` 必须严格 1..N 连续(per data_asset)。
//!
//! 历史上 `fix_chapter_idx_to_one_based` 出现过 0-based bug — startup 兜底,
//! 但写入路径上没有断言。把 invariant 用测试钉死。
//!
//! Invariant: 对任意 data_asset_id,
//! - MIN(idx) == 1
//! - MAX(idx) == COUNT(*)
//! - COUNT(DISTINCT idx) == COUNT(*)  (no duplicates)
//! - 无 idx 空洞:对每行,前一行 idx+1 == 当前 idx

use nsc_core::db::Db;
use nsc_core::models::NewChapter;

#[test]
fn idx_is_strictly_one_based_and_continuous_per_data_asset() {
    let db = Db::open_in_memory().unwrap();
    // 准备 2 个 data_asset
    let upload = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da1 = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id: upload,
        title: "DA1".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();
    let da2 = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id: upload,
        title: "DA2".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();

    // DA1: 5 章,idx 1..5
    for i in 1..=5 {
        db.chapters().insert(&NewChapter {
            data_asset_id: da1,
            idx: i,
            title: format!("DA1-c{i}"),
            body: format!("b{i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap();
    }

    // DA2: 3 章,idx 1..3 (独立于 DA1)
    for i in 1..=3 {
        db.chapters().insert(&NewChapter {
            data_asset_id: da2,
            idx: i,
            title: format!("DA2-c{i}"),
            body: format!("b{i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap();
    }

    // Invariant assertion:每 da 的 idx 是 1..N 严格连续
    assert_idx_invariant_holds(&db, da1);
    assert_idx_invariant_holds(&db, da2);
}

#[test]
#[should_panic]
fn idx_invariant_catches_zero_based_data() {
    let db = Db::open_in_memory().unwrap();
    let upload = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id: upload,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();

    // 历史 bug 复现:idx 从 0 开始
    for i in 0..3 {
        db.chapters().insert(&NewChapter {
            data_asset_id: da,
            idx: i,
            title: format!("c{i}"),
            body: format!("b{i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap();
    }

    // 应该失败:MIN(idx)=0, 不是 1
    assert_idx_invariant_holds(&db, da); // ← 应该 panic
}

#[test]
#[should_panic]
fn idx_invariant_catches_gaps() {
    let db = Db::open_in_memory().unwrap();
    let upload = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id: upload,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();

    // 创建 idx=1, 跳过 idx=2, 创建 idx=3 — 中间有洞
    db.chapters().insert(&NewChapter {
        data_asset_id: da,
        idx: 1,
        title: "c1".into(),
        body: "b".into(),
        word_count: 1,
        ..Default::default()
    }).unwrap();
    db.chapters().insert(&NewChapter {
        data_asset_id: da,
        idx: 3,
        title: "c3".into(),
        body: "b".into(),
        word_count: 1,
        ..Default::default()
    }).unwrap();

    // 应该失败:有空洞(1 → 3, 缺 2)
    assert_idx_invariant_holds(&db, da);
}

fn assert_idx_invariant_holds(db: &Db, data_asset_id: i64) {
    let conn = db.lock();

    // COUNT
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chapters WHERE data_asset_id = ?1",
        rusqlite::params![data_asset_id],
        |r| r.get(0),
    ).unwrap();

    if count == 0 {
        return; // 空 da 无 idx invariant
    }

    // MIN idx
    let min_idx: i64 = conn.query_row(
        "SELECT MIN(idx) FROM chapters WHERE data_asset_id = ?1",
        rusqlite::params![data_asset_id],
        |r| r.get(0),
    ).unwrap();

    // MAX idx
    let max_idx: i64 = conn.query_row(
        "SELECT MAX(idx) FROM chapters WHERE data_asset_id = ?1",
        rusqlite::params![data_asset_id],
        |r| r.get(0),
    ).unwrap();

    // DISTINCT count
    let distinct_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT idx) FROM chapters WHERE data_asset_id = ?1",
        rusqlite::params![data_asset_id],
        |r| r.get(0),
    ).unwrap();

    assert_eq!(
        min_idx, 1,
        "data_asset_id={}: MIN(idx)={}, expected 1 (1-based)",
        data_asset_id, min_idx
    );
    assert_eq!(
        max_idx, count,
        "data_asset_id={}: MAX(idx)={}, count={}, expected MAX=COUNT (no gaps)",
        data_asset_id, max_idx, count
    );
    assert_eq!(
        distinct_count, count,
        "data_asset_id={}: COUNT(DISTINCT idx)={}, count={}, expected equal (no duplicates)",
        data_asset_id, distinct_count, count
    );
}