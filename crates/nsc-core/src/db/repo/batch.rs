use std::sync::MutexGuard;
use chrono::{DateTime, Utc};
use rusqlite::{params, Row};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::models::{Batch, BatchStatus, NewBatch, OnFailurePolicy};

/// `batch_from_row` 期望的列清单 —— **唯一真相**。
///
/// 抽出来的原因:这份清单曾在三处各写一遍,其中 `BatchScheduler::stop_workflow`
/// 事务内回读那处漏了 7 列(prompt_id / model_config_id / mode / 4 个 ctx_*),
/// 导致 `batch_from_row` 抛 `InvalidColumnIndex(8)` —— **"停止工作流"整条路径
/// 每次真正执行都失败**。列清单与它的解析函数必须成对演进,收口到一处后,
/// 任何按 `batch_from_row` 解析的查询都以本常量为前缀,漏列不再可能。
///
/// COALESCE 的原因:migration 0029 新增的 7 列 schema 是 nullable,而 `Batch`
/// 结构体字段是 i32 / i64 / String(非 Option),遇 NULL 会抛
/// `Invalid column type Null`(0029 的 backfill 引用了当时不存在的
/// transformation_chapters.ctx_next_transformed → 该列永远 NULL)。
/// 这是「schema nullable 时的安全降级」,不是 fallback。
pub(crate) const BATCH_COLUMNS: &str = "\
    id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at, \
    COALESCE(prompt_id, 0) AS prompt_id, \
    COALESCE(model_config_id, 0) AS model_config_id, \
    COALESCE(mode, 'compress') AS mode, \
    COALESCE(ctx_prev_original, 0) AS ctx_prev_original, \
    COALESCE(ctx_prev_transformed, 0) AS ctx_prev_transformed, \
    COALESCE(ctx_next_original, 0) AS ctx_next_original, \
    COALESCE(ctx_next_transformed, 0) AS ctx_next_transformed";

pub struct BatchRepo<'a> { pub(crate) conn: MutexGuard<'a, rusqlite::Connection> }

impl<'a> BatchRepo<'a> {
    /// 插入一条 batch(status='pending')。返回新 id。
    pub fn insert(&self, b: &NewBatch) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let policy_s = policy_to_str(b.on_failure_policy);
        // mode 必须是 "compress" / "style" 之一(spec §3.1 wire-level 一致)。
        let mode_s = match b.mode.as_str() {
            "compress" | "style" => b.mode.as_str(),
            other => return Err(Error::Validation(format!("unknown batch mode: {other}"))),
        };
        self.conn.execute(
            "INSERT INTO batches \
             (transformation_novel_id, label, on_failure_policy, status, created_at, \
              prompt_id, model_config_id, mode, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, ctx_next_transformed) \
             VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                b.transformation_novel_id, b.label, policy_s, now,
                b.prompt_id, b.model_config_id, mode_s,
                b.ctx_prev_original, b.ctx_prev_transformed, b.ctx_next_original, b.ctx_next_transformed,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<Batch>> {
        // 列清单与 COALESCE 语义见 BATCH_COLUMNS 的文档。
        let sql = format!("SELECT {} FROM batches WHERE id = ?1", BATCH_COLUMNS);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? { Ok(Some(batch_from_row(row)?)) } else { Ok(None) }
    }

    pub fn list_by_tn(&self, tn_id: i64) -> Result<Vec<Batch>> {
        // 列清单见 BATCH_COLUMNS。
        let sql = format!(
            "SELECT {} FROM batches WHERE transformation_novel_id = ?1 ORDER BY id DESC",
            BATCH_COLUMNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![tn_id], batch_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 设 status 同时自动维护 started_at / ended_at 时间戳。
    /// - Running:started_at 已有则不动,首次写入;**ended_at 清空**(stopped → running 转移语义)。
    /// - Paused / Stopped / Completed / Terminated / Cancelled(即"暂停或终结"):ended_at 设 NOW。
    ///   注意 Paused 必须在内:`BatchScheduler::on_chapter_failed` 走裸 SQL 给 paused 写了
    ///   ended_at,UI 也把它当「结束时间」展示(`TransformationNovelDetail` 的 ended 列)
    ///   并用它做 overview 统计 —— 这里漏掉 Paused 会让同一状态经不同路径得到不同时间戳。
    /// - Pending:仅改 status。
    pub fn set_status(&self, id: i64, status: BatchStatus) -> Result<()> {
        let status_s = status_to_str(status);
        let now = Utc::now().to_rfc3339();
        match status {
            BatchStatus::Running => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2, \
                     started_at = COALESCE(started_at, ?3), \
                     ended_at = NULL \
                     WHERE id = ?1",
                    params![id, status_s, now],
                )?;
            }
            BatchStatus::Paused
            | BatchStatus::Completed
            | BatchStatus::Terminated
            | BatchStatus::Cancelled
            | BatchStatus::Stopped => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2, ended_at = ?3 WHERE id = ?1",
                    params![id, status_s, now],
                )?;
            }
            _ => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2 WHERE id = ?1",
                    params![id, status_s],
                )?;
            }
        }
        Ok(())
    }

    /// 改 label / on_failure_policy。只在 batch 不在 Running 时允许(上层校验)。
    pub fn update(&self, b: &Batch) -> Result<()> {
        let policy_s = policy_to_str(b.on_failure_policy);
        self.conn.execute(
            "UPDATE batches SET label = ?2, on_failure_policy = ?3 WHERE id = ?1",
            params![b.id, b.label, policy_s],
        )?;
        Ok(())
    }

    /// 删除一条 batch。
    /// 仅允许 deleted-status 的 batch 被删(防止误删正在被 worker 处理的任务):
    /// stopped / completed / terminated / cancelled —— 这四个状态 batch 不会再被
    /// scheduler 触碰,可以安全整行删。
    /// - 派生语义:data_assets.source_workflow_id 已在 0021 挂好 ON DELETE SET NULL,
    ///   promoted da 自动把"来源工作流"抹掉,da + da.chapters 物理保留(已是拷贝语义)。
    /// - 章节结果:workflow_results / workflow_result_chapters / transformation_chapters
    ///   在 0011 / 0027 挂好 CASCADE,跟 batch 一起删;chapter_previews 在 0024 挂好 CASCADE。
    /// - transformation_novels 不动 —— 工作流实例被删,工程模板保留。
    pub fn delete(&self, id: i64) -> Result<()> {
        let status_s: String = self.conn.query_row(
            "SELECT status FROM batches WHERE id = ?1",
            params![id],
            |r| r.get(0),
        ).map_err(|_| Error::NotFound(format!("batch {id} 不存在")))?;
        if !matches!(status_s.as_str(),
            "stopped" | "completed" | "terminated" | "cancelled")
        {
            return Err(Error::Validation(format!(
                "仅 stopped/completed/terminated/cancelled 工作流可删除(当前 {status_s})"
            )));
        }
        let n = self.conn.execute("DELETE FROM batches WHERE id = ?1", params![id])?;
        debug_assert_eq!(n, 1, "DELETE 应恰好影响 1 行");
        Ok(())
    }

    /// 统计批号各状态计数(给 UI tab badge 用)。
    pub fn count_by_status(&self, tn_id: i64) -> Result<BatchStatusCount> {
        let mut stmt = self.conn.prepare(
            "SELECT status, COUNT(*) FROM batches WHERE transformation_novel_id = ?1 GROUP BY status",
        )?;
        let rows = stmt.query_map(params![tn_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut counts = BatchStatusCount::default();
        for row in rows {
            let (s, n) = row?;
            match s.as_str() {
                "pending" => counts.pending = n,
                "running" => counts.running = n,
                "stopped" => counts.stopped = n,
                "paused" => counts.paused = n,
                "completed" => counts.completed = n,
                "terminated" => counts.terminated = n,
                "cancelled" => counts.cancelled = n,
                _ => {}
            }
        }
        Ok(counts)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BatchStatusCount {
    pub pending: i64,
    pub running: i64,
    pub stopped: i64,
    pub paused: i64,
    pub completed: i64,
    pub terminated: i64,
    pub cancelled: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::prompt::PromptKind;
    use crate::models::{
        NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
        NewTransformationNovel, NewUpload, Prompt,
    };

    // repo guard 不可重入:别一边持有 `db.xxx()` 的 guard 一边调另一个 `db.yyy()`。

    /// 最小环境:upload / da / tn / prompt / model + 3 章。
    fn seed(db: &Db) -> (i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "b1".into(), filename: "f.txt".into(), byte_size: 1,
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
        let mut cids = Vec::new();
        for i in 1..=3 {
            cids.push(db.chapters().insert(&NewChapter {
                data_asset_id: da_id, idx: i, title: format!("c{i}"),
                body: format!("正文{i}"), word_count: 3, ..Default::default()
            }).unwrap());
        }
        (tn_id, {
            let _ = (prompt_id, model_id);
            cids
        })
    }

    fn new_batch(tn_id: i64, policy: OnFailurePolicy) -> NewBatch {
        NewBatch {
            transformation_novel_id: tn_id,
            label: Some("l".into()),
            on_failure_policy: policy,
            prompt_id: 1, model_config_id: 1, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }
    }

    /// insert 的初始契约:status=pending,两个时间戳都还没写(没开始也没结束)。
    #[test]
    fn insert_starts_pending_without_timestamps() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        let b = db.batches().get(id).unwrap().unwrap();
        assert_eq!(b.status, BatchStatus::Pending);
        assert!(b.started_at.is_none(), "pending 不该有 started_at");
        assert!(b.ended_at.is_none(), "pending 不该有 ended_at");
        assert_eq!(b.on_failure_policy, OnFailurePolicy::PauseAndReview);
        assert_eq!(b.label.as_deref(), Some("l"));
    }

    /// Running:首次进入写 started_at 并**清空** ended_at(stopped → running 重启语义)。
    #[test]
    fn set_running_writes_started_at_and_clears_ended_at() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();

        db.batches().set_status(id, BatchStatus::Running).unwrap();
        let b = db.batches().get(id).unwrap().unwrap();
        assert_eq!(b.status, BatchStatus::Running);
        let first_started = b.started_at.expect("running 应写 started_at");
        assert!(b.ended_at.is_none());

        // 先停(写 ended_at),再重启:started_at 保留原值、ended_at 被清空
        db.batches().set_status(id, BatchStatus::Stopped).unwrap();
        let stopped = db.batches().get(id).unwrap().unwrap();
        assert_eq!(stopped.status, BatchStatus::Stopped);
        assert!(stopped.ended_at.is_some(), "stopped 应写 ended_at");

        db.batches().set_status(id, BatchStatus::Running).unwrap();
        let restarted = db.batches().get(id).unwrap().unwrap();
        assert_eq!(restarted.started_at, Some(first_started), "重启不应改写 started_at");
        assert!(restarted.ended_at.is_none(), "重启应清空 ended_at");
    }

    /// 终结态(paused / stopped / cancelled / terminated / completed)都写 ended_at。
    #[test]
    fn terminal_statuses_write_ended_at() {
        for st in [
            BatchStatus::Paused, BatchStatus::Stopped, BatchStatus::Cancelled,
            BatchStatus::Terminated, BatchStatus::Completed,
        ] {
            let db = Db::open_in_memory().unwrap();
            let (tn_id, _) = seed(&db);
            let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
            db.batches().set_status(id, st).unwrap();
            let b = db.batches().get(id).unwrap().unwrap();
            assert_eq!(b.status, st);
            assert!(b.ended_at.is_some(), "{st:?} 应写 ended_at");
        }
    }

    /// 非 running / 非终结态(pending)只改状态,不碰时间戳。
    #[test]
    fn pending_status_only_changes_status() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        db.batches().set_status(id, BatchStatus::Running).unwrap();
        let started = db.batches().get(id).unwrap().unwrap().started_at;
        // 回到 pending(仅改 status 分支)
        db.batches().set_status(id, BatchStatus::Pending).unwrap();
        let b = db.batches().get(id).unwrap().unwrap();
        assert_eq!(b.status, BatchStatus::Pending);
        assert_eq!(b.started_at, started, "回到 pending 不应改动 started_at");
        assert!(b.ended_at.is_none());
    }

    /// 删除只允许"终态"batch —— 正在跑的不能被误删。
    #[test]
    fn delete_rejects_running_batch() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        db.batches().set_status(id, BatchStatus::Running).unwrap();
        let err = db.batches().delete(id).unwrap_err();
        assert!(err.to_string().contains("仅"), "应拒绝删除运行中的 batch,实际: {err}");
        assert!(db.batches().get(id).unwrap().is_some(), "被拒后行应仍在");

        // pending 同样不允许
        let id2 = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::SkipFailed)).unwrap();
        assert!(db.batches().delete(id2).is_err());
    }

    /// 删除终态 batch 行确实被移除;不存在的 id 报 NotFound。
    #[test]
    fn delete_removes_terminal_batch_and_reports_missing() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        db.batches().set_status(id, BatchStatus::Stopped).unwrap();
        db.batches().delete(id).unwrap();
        assert!(db.batches().get(id).unwrap().is_none());

        let err = db.batches().delete(99999).unwrap_err();
        assert!(err.to_string().contains("不存在"), "不存在的 id 应报 NotFound: {err}");
    }

    /// 删除 batch 级联清掉它的 tc 行(0027 挂的 ON DELETE CASCADE)。
    #[test]
    fn delete_cascades_to_transformation_chapters() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, cids) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        for cid in &cids {
            db.transformation_chapters().insert(&NewTransformationChapter {
                transformation_novel_id: tn_id, chapter_id: *cid,
                mode: PromptKind::Compress, prompt_id: 1, model_config_id: 1,
                ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
                batch_id: Some(id), style_ref_chapter_id: None,
            }).unwrap();
        }
        assert_eq!(db.transformation_chapters().count_by_batch(id).unwrap(), 3);

        db.batches().set_status(id, BatchStatus::Stopped).unwrap();
        db.batches().delete(id).unwrap();

        assert_eq!(
            db.transformation_chapters().count_by_batch(id).unwrap(), 0,
            "删 batch 应级联清掉 tc 行"
        );
    }

    /// update 只改 label / on_failure_policy,不动 status 与时间戳。
    #[test]
    fn update_changes_only_label_and_policy() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        db.batches().set_status(id, BatchStatus::Stopped).unwrap();
        let before = db.batches().get(id).unwrap().unwrap();

        let mut edited = before.clone();
        edited.label = Some("新标签".into());
        edited.on_failure_policy = OnFailurePolicy::SkipFailed;
        edited.status = BatchStatus::Running; // 不该被 update 写进去
        db.batches().update(&edited).unwrap();

        let after = db.batches().get(id).unwrap().unwrap();
        assert_eq!(after.label.as_deref(), Some("新标签"));
        assert_eq!(after.on_failure_policy, OnFailurePolicy::SkipFailed);
        assert_eq!(after.status, before.status, "update 不应改 status");
        assert_eq!(after.started_at, before.started_at);
        assert_eq!(after.ended_at, before.ended_at);
    }

    /// count_by_status 按 tn 分组统计,且不串到别的 tn。
    #[test]
    fn count_by_status_groups_by_tn() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let a = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::PauseAndReview)).unwrap();
        let b = db.batches().insert(&new_batch(tn_id, OnFailurePolicy::SkipFailed)).unwrap();
        db.batches().set_status(a, BatchStatus::Running).unwrap();
        db.batches().set_status(b, BatchStatus::Stopped).unwrap();

        let c = db.batches().count_by_status(tn_id).unwrap();
        assert_eq!(c.running, 1);
        assert_eq!(c.stopped, 1);
        assert_eq!(c.pending, 0);

        let other = db.batches().count_by_status(99999).unwrap();
        assert_eq!(other.running + other.stopped, 0, "别的 tn 不应串进来");
    }

    /// list_by_tn 返回该 tn 的 batch(按 id DESC),并带齐 7 个迁移新增列的值。
    #[test]
    fn list_by_tn_returns_batches_with_context_columns() {
        let db = Db::open_in_memory().unwrap();
        let (tn_id, _) = seed(&db);
        let id = db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id, label: Some("x".into()),
            on_failure_policy: OnFailurePolicy::SkipFailed,
            prompt_id: 7, model_config_id: 9, mode: "style".into(),
            ctx_prev_original: 2, ctx_prev_transformed: 1,
            ctx_next_original: 3, ctx_next_transformed: 4,
        }).unwrap();
        let list = db.batches().list_by_tn(tn_id).unwrap();
        assert_eq!(list.len(), 1);
        let b = &list[0];
        assert_eq!(b.id, id);
        assert_eq!(b.prompt_id, 7);
        assert_eq!(b.model_config_id, 9);
        assert_eq!(b.mode, "style");
        assert_eq!(b.ctx_prev_original, 2);
        assert_eq!(b.ctx_prev_transformed, 1);
        assert_eq!(b.ctx_next_original, 3);
        assert_eq!(b.ctx_next_transformed, 4);
    }
}

pub(crate) fn batch_from_row(row: &Row) -> rusqlite::Result<Batch> {
    let created_at_s: String = row.get(5)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            5, rusqlite::types::Type::Text, Box::new(e)))?;
    let started_at_s: Option<String> = row.get(6)?;
    let ended_at_s:   Option<String> = row.get(7)?;
    let parse_opt = |s: Option<String>| -> rusqlite::Result<Option<DateTime<Utc>>> {
        match s {
            None => Ok(None),
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|d| Some(d.with_timezone(&Utc)))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    6, rusqlite::types::Type::Text, Box::new(e))),
        }
    };
    Ok(Batch {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        label: row.get(2)?,
        on_failure_policy: str_to_policy(&row.get::<_, String>(3)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?,
        status: str_to_status(&row.get::<_, String>(4)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?,
        created_at,
        started_at: parse_opt(started_at_s)?,
        ended_at: parse_opt(ended_at_s)?,
        // 新增(append_chapters spec §3.2):
        prompt_id: row.get(8)?,
        model_config_id: row.get(9)?,
        mode: row.get(10)?,
        ctx_prev_original: row.get(11)?,
        ctx_prev_transformed: row.get(12)?,
        ctx_next_original: row.get(13)?,
        ctx_next_transformed: row.get(14)?,
    })
}

fn status_to_str(s: BatchStatus) -> &'static str {
    match s {
        BatchStatus::Pending    => "pending",
        BatchStatus::Running    => "running",
        BatchStatus::Stopped    => "stopped",
        BatchStatus::Paused     => "paused",
        BatchStatus::Completed  => "completed",
        BatchStatus::Terminated => "terminated",
        BatchStatus::Cancelled  => "cancelled",
    }
}
fn str_to_status(s: &str) -> rusqlite::Result<BatchStatus> {
    match s {
        "pending"    => Ok(BatchStatus::Pending),
        "running"    => Ok(BatchStatus::Running),
        "stopped"    => Ok(BatchStatus::Stopped),
        "paused"     => Ok(BatchStatus::Paused),
        "completed"  => Ok(BatchStatus::Completed),
        "terminated" => Ok(BatchStatus::Terminated),
        "cancelled"  => Ok(BatchStatus::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text,
            format!("unknown batch status: {other}").into())),
    }
}
fn policy_to_str(p: OnFailurePolicy) -> &'static str {
    match p {
        OnFailurePolicy::PauseAndReview => "pause_and_review",
        OnFailurePolicy::SkipFailed     => "skip_failed",
    }
}
fn str_to_policy(s: &str) -> rusqlite::Result<OnFailurePolicy> {
    match s {
        "pause_and_review" => Ok(OnFailurePolicy::PauseAndReview),
        "skip_failed"      => Ok(OnFailurePolicy::SkipFailed),
        // 历史库可能残留 'terminate'(0.2 之前作为 OnFailurePolicy::Terminate 写入过),
        // 旧值已不再代表"全自动终止",降级为最保守的 PauseAndReview 让数据仍可读。
        // spec 见 docs/spec.md §5.1。
        "terminate" => Ok(OnFailurePolicy::PauseAndReview),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text,
            format!("unknown on_failure_policy: {other}").into())),
    }
}
