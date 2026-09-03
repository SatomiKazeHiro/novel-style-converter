use std::sync::MutexGuard;
use chrono::{DateTime, Utc};
use rusqlite::{params, Row};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::models::{Batch, BatchStatus, NewBatch, OnFailurePolicy};

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
        // COALESCE for the 7 fields added in migration 0029 — schema 是 nullable,
        // Batch struct 字段是 i32 / i64 / String(非 Option),read 端遇到 NULL 直接抛
        // Invalid column type Null(migration 0029 backfill 引用了不存在的
        // transformation_chapters.ctx_next_transformed → ctx_next_transformed 永远 NULL)。
        // COALESCE 是「schema nullable 时安全降级到 i32 默认值 0 / i64 默认值 0 /
        // mode 默认 'compress'」,0 = ctx_next_transformed 的「无后文」语义,与
        // WorkflowCreate 默认值(commit 1a7d845)对齐;不是 fallback。
        let mut stmt = self.conn.prepare(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at, \
              COALESCE(prompt_id, 0) AS prompt_id, \
              COALESCE(model_config_id, 0) AS model_config_id, \
              COALESCE(mode, 'compress') AS mode, \
              COALESCE(ctx_prev_original, 0) AS ctx_prev_original, \
              COALESCE(ctx_prev_transformed, 0) AS ctx_prev_transformed, \
              COALESCE(ctx_next_original, 0) AS ctx_next_original, \
              COALESCE(ctx_next_transformed, 0) AS ctx_next_transformed \
             FROM batches WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? { Ok(Some(batch_from_row(row)?)) } else { Ok(None) }
    }

    pub fn list_by_tn(&self, tn_id: i64) -> Result<Vec<Batch>> {
        // 同 get() 的 COALESCE 注释 —— 7 个新增列 schema nullable,read 端做 NULL→默认值
        // 安全降级,避免 batch_from_row 抛 Invalid column type Null。
        let mut stmt = self.conn.prepare(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at, \
              COALESCE(prompt_id, 0) AS prompt_id, \
              COALESCE(model_config_id, 0) AS model_config_id, \
              COALESCE(mode, 'compress') AS mode, \
              COALESCE(ctx_prev_original, 0) AS ctx_prev_original, \
              COALESCE(ctx_prev_transformed, 0) AS ctx_prev_transformed, \
              COALESCE(ctx_next_original, 0) AS ctx_next_original, \
              COALESCE(ctx_next_transformed, 0) AS ctx_next_transformed \
             FROM batches WHERE transformation_novel_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![tn_id], batch_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 设 status 同时自动维护 started_at / ended_at 时间戳。
    /// - Running:started_at 已有则不动,首次写入;**ended_at 清空**(stopped → running 转移语义)。
    /// - Completed/Terminated/Cancelled/Stopped:ended_at 设 NOW。
    /// - 其它:仅改 status。
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
            BatchStatus::Completed | BatchStatus::Terminated | BatchStatus::Cancelled | BatchStatus::Stopped => {
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
