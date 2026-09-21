//! Startup cleanup (one-shot, gated by marker table).
//!
//! æµè¯é¶æ®µåè®¸ç ´åæ§ schema æ¹å¨,æ¹å¨åæ®ççæ§æ°æ®å¯è½è·æ°çº¦æå²çªã
//! è¿ç§æ¸çç¨ `cleanup_markers` è¡¨è®°å + æ¶é´æ³,åªå¨ç¬¬ä¸æ¬¡å¯å¨æ¶è·,
//! è·å®å marker,ä»¥åä¸åæ§è¡ã
//!
//! æ°å¢æ¸çé¡¹:å¨ `CLEANUPS` éå ä¸è¡ ãã å(è®°å½åæ°åæ¶é´æ³)ãSQL å¤æ¡ statements ç¨ `;` åéã
//! æµè¯æ¶æ¸åºåæ³éè·,ç´æ¥ `DELETE FROM cleanup_markers`ã

use rusqlite::{params, Connection};

use crate::error::Result;

/// å½åæ¸çé¡¹åè¡¨ãæ°å¢é¡¹ç®æ¶åªé append å°è¿éã
/// åå­æ¹ä¸ä¸ª = éæ°ææ(å ä¸º marker æªå°è¾¾)ã
const CLEANUPS: &[(&str, &str)] = &[
    (
        "2026_08_14_clear_test_data",
        // ä¿ç uploads / prompts / model_configs;å¶ä½è¡¨æ¸ç©ºè®©ç¨æ·éæ°èµ°ä¸éæµç¨ã
        // é¡ºåº:tc â workflow_results â batches â data_assets â ai_call_logsã
        // tc.batch_id æ¯ NO ACTION ä¸ä¼ cascade,å¾åå ;
        // data_assets CASCADE æ¸ chaptersã
        "DELETE FROM transformation_chapters;DELETE FROM workflow_result_chapters;DELETE FROM workflow_results;DELETE FROM batches;DELETE FROM transformation_novels;DELETE FROM data_assets;DELETE FROM ai_call_logs;",
    ),
    (
        // 老版 migration 0023 写的是 `UPDATE chapters SET idx = idx + 1`,
        // 触发 UNIQUE(data_asset_id, idx) 约束失败、Db::open panic 退出,
        // 所以迁移修复必须放到 startup_cleanup(在 Db::open 之后跑)。
        // 两步法:先整体偏移到无冲突区间(10亿),再回退 -999_999_999,等价于 +1。
        // 偏移值 10亿 + idx 都 < i32::MAX(21.47亿),安全。
        // 新数据由 commit_data_asset 用 (i + 1) as i32 保证 1-based,无需再迁。
        "fix_chapter_idx_to_one_based",
        "UPDATE chapters SET idx = idx + 1000000000;UPDATE chapters SET idx = idx - 999999999;",
    ),
];

pub fn run(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cleanup_markers (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL,
            rowcount INTEGER NOT NULL DEFAULT 0
        )",
    )?;

    for (name, sql) in CLEANUPS {
        let applied: bool = conn
            .query_row(
                "SELECT 1 FROM cleanup_markers WHERE name = ?1",
                [name],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if applied {
            continue;
        }

        let tx = conn.unchecked_transaction()?;
        let rowcount: usize = {
            let mut count = 0usize;
            for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                count += tx.execute(stmt, [])?;
            }
            count
        };
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO cleanup_markers (name, applied_at, rowcount) VALUES (?1, ?2, ?3)",
            params![name, now, rowcount as i64],
        )?;
        tx.commit()?;
        eprintln!("[startup_cleanup] applied '{}' ({} rows)", name, rowcount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::{
        NewChapter, NewDataAsset, NewModelConfig, NewTransformationNovel, NewUpload,
    };

    const CLEAR: &str = "2026_08_14_clear_test_data";
    const IDX_FIX: &str = "fix_chapter_idx_to_one_based";

    fn fresh() -> Db {
        // 注意:`Db::open*` **不跑** startup_cleanup —— 它由 src-tauri/lib.rs 在
        // Db::open 之后调用(必须这样:migration 0023 的 idx 修复在 open 期间会
        // 撞 UNIQUE 约束 panic)。所以测试里要显式调 run()。
        Db::open_in_memory().unwrap()
    }

    fn marker_applied(db: &Db, name: &str) -> bool {
        db.lock()
            .query_row("SELECT 1 FROM cleanup_markers WHERE name=?1", [name], |_| Ok(true))
            .unwrap_or(false)
    }

    /// 建 upload + da + n 章(idx 从 `start` 起,允许造出 0-based 的旧数据)。
    fn seed_chapters(db: &Db, sha: &str, start: i32, n: i32) -> Vec<i64> {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: sha.into(), filename: "f.txt".into(), byte_size: 1,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 2,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        (0..n).map(|i| {
            db.chapters().insert(&NewChapter {
                data_asset_id: da_id, idx: start + i, title: format!("c{}", start + i),
                body: "正文".into(), word_count: 2, ..Default::default()
            }).unwrap()
        }).collect()
    }

    /// 首次运行:两项 cleanup 都被执行并写下 marker。
    ///
    /// 注意:同一轮 run 里 clear_test_data 会**先于** idx 修复执行(数组顺序),
    /// 所以空库上 idx 修复的影响行数就是 0 —— 这是正确行为,不是缺陷。
    #[test]
    fn first_run_applies_all_cleanups_and_records_markers() {
        let db = fresh();
        run(&db.lock()).unwrap();
        assert!(marker_applied(&db, CLEAR), "clear_test_data 应写下 marker");
        assert!(marker_applied(&db, IDX_FIX), "idx 修复应写下 marker");
        let rowcount: i64 = db.lock().query_row(
            "SELECT rowcount FROM cleanup_markers WHERE name=?1", [IDX_FIX], |r| r.get(0),
        ).unwrap();
        assert_eq!(rowcount, 0, "空库上 idx 修复影响 0 行");
    }

    /// 幂等:第二次运行不再执行(marker 命中即跳过)。
    /// 关键性质 —— 否则每次启动都会把 idx 再加一次。
    ///
    /// 只删 idx 的 marker、**保留** clear 的 marker:这样第二次 run 会跳过 clear
    /// (不删我们刚种的数据)、只跑 idx 修复。
    #[test]
    fn second_run_skips_applied_cleanups() {
        let db = fresh();
        run(&db.lock()).unwrap();
        db.lock().execute("DELETE FROM cleanup_markers WHERE name=?1", [IDX_FIX]).unwrap();
        let ids = seed_chapters(&db, "c2", 0, 3);

        run(&db.lock()).unwrap();
        let after_first: Vec<i32> = ids.iter()
            .map(|id| db.chapters().get(*id).unwrap().unwrap().idx)
            .collect();

        // 再来一次:若 marker 机制失效,idx 会被再加 1
        run(&db.lock()).unwrap();
        let after_second: Vec<i32> = ids.iter()
            .map(|id| db.chapters().get(*id).unwrap().unwrap().idx)
            .collect();
        assert_eq!(after_first, after_second, "已应用的 cleanup 不应重复执行");
    }

    /// idx 修复的核心行为:0-based → 1-based,且绕过 `UNIQUE(data_asset_id, idx)`。
    ///
    /// 老版 migration 0023 用 `UPDATE chapters SET idx = idx + 1` 直接撞唯一约束,
    /// 导致 `Db::open` panic。两步法(先整体 +10亿 再 -999999999)等价于 +1 但不冲突。
    #[test]
    fn idx_fix_shifts_zero_based_to_one_based_without_unique_clash() {
        let db = fresh();
        run(&db.lock()).unwrap();
        // 保留 clear 的 marker(免得第二次 run 把种下的 chapters 删掉),
        // 只让 idx 修复待跑。
        db.lock().execute("DELETE FROM cleanup_markers WHERE name=?1", [IDX_FIX]).unwrap();
        let ids = seed_chapters(&db, "c3", 0, 3);

        run(&db.lock()).unwrap();

        let idxs: Vec<i32> = ids.iter()
            .map(|id| db.chapters().get(*id).unwrap().unwrap().idx)
            .collect();
        assert_eq!(idxs, vec![1, 2, 3], "0-based 应整体 +1 成 1-based(且顺序不变)");
        assert!(marker_applied(&db, IDX_FIX), "重跑后应重新写下 marker");
    }

    /// 已 1-based 的数据再被"修复"一次会变成 2-based —— 这说明 marker 是唯一护栏,
    /// 记录该语义以便将来有人想手工删 marker 时知道后果。
    #[test]
    fn idx_fix_is_not_idempotent_without_marker() {
        let db = fresh();
        run(&db.lock()).unwrap();
        db.lock().execute("DELETE FROM cleanup_markers WHERE name=?1", [IDX_FIX]).unwrap();
        let ids = seed_chapters(&db, "c4", 1, 3);

        run(&db.lock()).unwrap();
        // 删 marker 强制重跑(模拟"marker 丢失")
        db.lock().execute("DELETE FROM cleanup_markers WHERE name=?1", [IDX_FIX]).unwrap();
        run(&db.lock()).unwrap();
        let idxs: Vec<i32> = ids.iter()
            .map(|id| db.chapters().get(*id).unwrap().unwrap().idx)
            .collect();
        assert_eq!(idxs, vec![3, 4, 5], "第一次 run 已 +1(1→2),无护栏重跑再 +1(→3)");
    }

    /// cleanup 名改一个字符 = 该清理重新生效(设计如此,注释里明说)。
    #[test]
    fn renaming_marker_makes_cleanup_effective_again() {
        let db = fresh();
        run(&db.lock()).unwrap();
        assert!(marker_applied(&db, CLEAR));
        // 改名(等价于"换了个 cleanup")
        db.lock().execute(
            "UPDATE cleanup_markers SET name='2026_08_14_clear_test_data_v2' WHERE name=?1",
            [CLEAR],
        ).unwrap();
        run(&db.lock()).unwrap();
        assert!(marker_applied(&db, CLEAR), "旧名不再有 marker → 应被重新执行");
    }

    /// clear_test_data 的取舍:清空业务数据表,但**保留** uploads / prompts /
    /// model_configs(用户重新走一遍流程即可,不必重传文件)。
    #[test]
    fn clear_cleanup_wipes_business_tables_but_keeps_uploads_and_configs() {
        let db = fresh();
        let ids = seed_chapters(&db, "c5", 1, 2);
        let prompt_id = db.prompts().insert(&crate::models::Prompt {
            id: 0, name: "p".into(), kind: crate::models::PromptKind::Compress,
            template: "{{chapter_content}}".into(), is_builtin: false, archived: 0,
        }).unwrap();
        let model_id = db.model_configs().insert(&NewModelConfig {
            name: "m".into(), base_url: "http://localhost".into(), api_key: "k".into(),
            model: "m".into(), max_tokens: None, max_context: None, temperature: None,
            disable_thinking: false, concurrency: 1,
        }).unwrap();
        let da_id = db.chapters().get(ids[0]).unwrap().unwrap().data_asset_id;
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(), note: String::new(),
        }).unwrap();

        // 删 marker 强制重跑 clear
        run(&db.lock()).unwrap();
        db.lock().execute("DELETE FROM cleanup_markers WHERE name=?1", [CLEAR]).unwrap();
        run(&db.lock()).unwrap();

        assert!(db.transformation_novels().get(tn_id).unwrap().is_none(), "tn 应被清空");
        assert!(db.data_assets().get(da_id).unwrap().is_none(), "data_asset 应被清空");
        assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 0, "chapters 应被清空");
        // 保留项
        assert!(db.prompts().get(prompt_id).unwrap().is_some(), "prompts 应保留");
        assert!(db.model_configs().get(model_id).unwrap().is_some(), "model_configs 应保留");
        assert!(!db.uploads().list().unwrap().is_empty(), "uploads 应保留");
    }
}
