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
