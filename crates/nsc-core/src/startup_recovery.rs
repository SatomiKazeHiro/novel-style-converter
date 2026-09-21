//! Startup safe-recovery (spec §7).
//!
//! 前一次崩溃可能让数据库里残留: Running 状态的 batch,
//! 以及 Running 或 Pending 状态的 tc。本模块在应用启动时一次性把它们
//! 收口成 settled 终态,避免 worker 启动后看到半个还在 Running 的 tc 行。
//! 不自动重新调用模型 —— 用户进入工作流详情后可主动重试空槽。
//!
//! 收口规则:
//! - `transformation_chapters.status='running'` → `failed`,错误为 "进程中断,安全停止",
//!   `completed_at` 用当前时间补齐。
//! - `transformation_chapters.status='pending'` → `skipped`,`completed_at` 用当前时间。
//! - `batches.status='running'` → `stopped`,`ended_at` 优先取已有 `started_at`,其次 `created_at`。

use rusqlite::params;

use crate::error::Result;

pub fn run(conn: &rusqlite::Connection) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "UPDATE transformation_chapters \
         SET status='failed', error='进程中断,安全停止', \
             completed_at = COALESCE(completed_at, ?1) \
         WHERE status='running'",
        params![now],
    )?;

    tx.execute(
        "UPDATE transformation_chapters \
         SET status='skipped', \
             completed_at = COALESCE(completed_at, ?1) \
         WHERE status='pending'",
        params![now],
    )?;

    tx.execute(
        "UPDATE batches \
         SET status='stopped', \
             ended_at = COALESCE(ended_at, started_at, created_at) \
         WHERE status='running'",
        [],
    )?;

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::batch::{NewBatch, OnFailurePolicy};
    use crate::models::prompt::PromptKind;
    use crate::models::{
        NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
        NewTransformationNovel, NewUpload, Prompt, TransformStatus,
    };

    /// 建最小环境:1 upload / da / tn / prompt / model / batch(running) + 3 章。
    /// 三章先都建成 pending,由各用例改成它要的初始状态。
    fn seed(db: &crate::db::Db) -> (i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "r1".into(), filename: "f.txt".into(), byte_size: 1,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 2,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(), note: String::new(),
        }).unwrap();
        let prompt_id = db.prompts().insert(&Prompt {
            id: 0, name: "p".into(), kind: PromptKind::Compress,
            template: "{{chapter_content}}".into(), is_builtin: false, archived: 0,
        }).unwrap();
        let model_id = db.model_configs().insert(&NewModelConfig {
            name: "m".into(), base_url: "http://localhost".into(), api_key: "k".into(),
            model: "m".into(), max_tokens: None, max_context: None, temperature: None,
            disable_thinking: false, concurrency: 1,
        }).unwrap();
        // batch 建出来是 pending;这里直接改成 running(模拟"崩溃时正在跑")
        let batch_id = db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id, label: None,
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id, model_config_id: model_id, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap();
        db.batches().set_status(batch_id, crate::models::BatchStatus::Running).unwrap();

        let mut cids = Vec::new();
        for i in 1..=3 {
            let cid = db.chapters().insert(&NewChapter {
                data_asset_id: da_id, idx: i, title: format!("c{i}"),
                body: format!("正文{i}"), word_count: 3, ..Default::default()
            }).unwrap();
            cids.push(cid);
            db.transformation_chapters().insert(&NewTransformationChapter {
                transformation_novel_id: tn_id, chapter_id: cid,
                mode: PromptKind::Compress, prompt_id, model_config_id: model_id,
                ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
                batch_id: Some(batch_id), style_ref_chapter_id: None,
            }).unwrap();
        }
        (batch_id, cids)
    }

    fn fresh() -> (crate::db::Db, i64, Vec<i64>) {
        let db = crate::db::Db::open_in_memory().unwrap();
        let (batch_id, cids) = seed(&db);
        (db, batch_id, cids)
    }

    /// 注意:本文件的用例都必须**先把 repo guard 释放掉**再调 `run(&db.lock())` ——
    /// `db.xxx()` 的 MutexGuard 不可重入,一边持有它一边取 db 锁就是自死锁
    /// (表现为测试永久挂住,不是失败)。所以统一用"取 id → 释放 → run → 重新取"。

    /// 主契约:崩溃遗留的 running 章 → failed + 错误说明;pending 章 → skipped。
    /// 两者都补 completed_at(running 的错误文案是用户唯一能看到的线索)。
    #[test]
    fn settles_running_and_pending_chapters() {
        let (db, _b, cids) = fresh();
        let (tids, running_tid) = {
            let t = db.transformation_chapters();
            let tids: Vec<i64> = cids.iter()
                .map(|c| t.list_by_chapter(*c).unwrap()[0].id)
                .collect();
            t.mark_running(tids[0]).unwrap();
            // 其余两章保持 pending
            (tids.clone(), tids[0])
        };
        let _ = running_tid;

        run(&db.lock()).unwrap();

        let t = db.transformation_chapters();
        let a = t.get(tids[0]).unwrap().unwrap();
        assert_eq!(a.status, TransformStatus::Failed, "running 应收口为 failed");
        assert_eq!(a.error.as_deref(), Some("进程中断,安全停止"));
        assert!(a.completed_at.is_some(), "failed 应补 completed_at");

        for tid in [tids[1], tids[2]] {
            let p = t.get(tid).unwrap().unwrap();
            assert_eq!(p.status, TransformStatus::Skipped, "pending 应收口为 skipped");
            assert!(p.completed_at.is_some(), "skipped 应补 completed_at");
        }
    }

    /// running batch → stopped,且 ended_at 被写上(优先 started_at)。
    #[test]
    fn settles_running_batch_to_stopped() {
        let (db, batch_id, _cids) = fresh();
        run(&db.lock()).unwrap();
        let b = db.batches().get(batch_id).unwrap().unwrap();
        assert_eq!(b.status, crate::models::BatchStatus::Stopped);
        assert!(b.ended_at.is_some(), "stopped 应有 ended_at");
    }

    /// 幂等:连续跑两次结果一致(第二次没有 running/pending 可收口)。
    /// 启动期每个进程都会跑一次,不能因为重复执行而改变数据。
    #[test]
    fn is_idempotent() {
        let (db, batch_id, cids) = fresh();
        let tid = {
            let t = db.transformation_chapters();
            let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
            t.mark_running(tid).unwrap();
            tid
        };

        run(&db.lock()).unwrap();
        let after_first = {
            let t = db.transformation_chapters();
            let tc = t.get(tid).unwrap().unwrap();
            (tc.status, tc.completed_at)
        };
        let batch_first = db.batches().get(batch_id).unwrap().unwrap().status;
        run(&db.lock()).unwrap();
        let after_second = {
            let t = db.transformation_chapters();
            let tc = t.get(tid).unwrap().unwrap();
            (tc.status, tc.completed_at)
        };
        let batch_second = db.batches().get(batch_id).unwrap().unwrap().status;
        assert_eq!(after_first, after_second, "recovery 必须幂等");
        assert_eq!(batch_first, batch_second, "batch 状态也必须幂等");
    }

    /// 已终结的章节不受影响(done / failed / skipped 不被改写)。
    #[test]
    fn leaves_settled_chapters_untouched() {
        let (db, _b, cids) = fresh();
        let tids: Vec<i64> = {
            let t = db.transformation_chapters();
            let tids: Vec<i64> = cids.iter()
                .map(|c| t.list_by_chapter(*c).unwrap()[0].id)
                .collect();
            t.mark_done(tids[0], "已完成的正文".into(), Some(7), Some(9)).unwrap();
            t.mark_failed(tids[1], "原有失败原因".into()).unwrap();
            t.mark_skipped(tids[2], "用户主动跳过".into()).unwrap();
            tids
        };

        run(&db.lock()).unwrap();

        let t = db.transformation_chapters();
        let done = t.get(tids[0]).unwrap().unwrap();
        assert_eq!(done.status, TransformStatus::Done);
        assert_eq!(done.result_content.as_deref(), Some("已完成的正文"));
        assert_eq!(done.tokens_in, Some(7));

        let failed = t.get(tids[1]).unwrap().unwrap();
        assert_eq!(failed.status, TransformStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("原有失败原因"),
            "已有失败原因不应被 recovery 覆盖");

        let skipped = t.get(tids[2]).unwrap().unwrap();
        assert_eq!(skipped.status, TransformStatus::Skipped);
        assert_eq!(skipped.error.as_deref(), Some("用户主动跳过"));
    }

    /// 已有的 completed_at 不被覆盖(COALESCE(completed_at, ?1))。
    #[test]
    fn preserves_existing_completed_at() {
        let (db, _b, cids) = fresh();
        let tid = {
            let t = db.transformation_chapters();
            let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
            t.mark_running(tid).unwrap();
            tid
        };
        // 手工塞一个"崩溃前"的 completed_at
        db.lock().execute(
            "UPDATE transformation_chapters SET completed_at='2020-01-01T00:00:00+00:00' WHERE id=?1",
            params![tid],
        ).unwrap();
        run(&db.lock()).unwrap();
        let got = db.transformation_chapters().get(tid).unwrap().unwrap();
        assert_eq!(got.status, TransformStatus::Failed);
        assert!(got.completed_at.unwrap().to_rfc3339().starts_with("2020-01-01"),
            "已有 completed_at 不应被覆盖");
    }

    /// 非 running 的 batch 不被改写(paused / stopped / completed 等)。
    #[test]
    fn leaves_non_running_batches_untouched() {
        let (db, batch_id, _cids) = fresh();
        db.batches().set_status(batch_id, crate::models::BatchStatus::Paused).unwrap();
        run(&db.lock()).unwrap();
        assert_eq!(
            db.batches().get(batch_id).unwrap().unwrap().status,
            crate::models::BatchStatus::Paused,
            "只有 running 的 batch 才该被收口"
        );
    }
}
