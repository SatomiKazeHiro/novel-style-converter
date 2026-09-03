use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

use super::migrate::SCHEMAS;
use super::repo::{
    AiCallLogRepo, BatchRepo, ChapterPreviewRepo, ChapterRepo, DataAssetRepo,
    ModelConfigRepo, OverviewRepo, PromptRepo, TransformationChapterRepo,
    TransformationNovelRepo, UploadRepo, WorkflowResultRepo,
};

/// 跨线程共享 DB 句柄 —— 内部 Connection 通过 Mutex 串行化所有访问。
///
/// ## 为什么是 Arc<Mutex<Connection>>(根治,不是配置层面的修补)
///
/// 设计目标:全应用只存在 一个 Connection。main thread / 2 个
/// JobQueue worker / recorder writer / notifier 闭包 —— 这些原本各自
/// Db::connect 一份独立 Connection 抢同一份 DB 文件,WAL mode +
/// busy_timeout 5s 把 SQLITE_BUSY 概率降一些,但只要再撞一次,notifier
/// 里的 eprintln 一吞,数据就丢了(tc.status='done' 在 worker 自家 connection 上写成功,
/// wrc.content 在 notifier 的新 connection 上写失败 → 用户看到 "AI 调用成功但内容没写")。
///
/// 把所有线程强制收敛到同一份 Connection:
/// - Db::open / Db::connect 都返回 Arc<Self>,底层共享同一个 Mutex<Connection>
/// - 写物理串行化(mutex 持有),SQLITE_BUSY 从根上消除
/// - WAL mode 保留: synchronous=NORMAL + WAL 提供崩溃恢复 + 多连接读并发能力
/// - notifier 错误不再吞:错误传回 worker,落 tc.error + batch 状态变化,
///   让 UI 看到、用户能重试。
#[derive(Debug)]
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// 打开(或创建)DB,跑迁移,返回共享句柄。
    /// 业务入口 —— lib.rs 用一份,worker / scheduler / recorder 都克隆 Arc 共享。
    pub fn open(path: &Path) -> Result<Arc<Self>> {
        let conn = Self::open_connection(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        let db = Arc::new(Self { conn: Mutex::new(conn) });
        run_schemas(&db.lock())?;
        UploadRepo { conn: db.lock() }.backfill_word_count()?;
        UploadRepo { conn: db.lock() }.recompute_all_word_count()?;
        ChapterRepo { conn: db.lock() }.recompute_all_word_count()?;
        Ok(db)
    }

    /// 复用 path 上的 Db:打开一个新 Connection 但仍返回 Arc<Self>。
    /// 生产路径不再调用 —— lib.rs 用 Arc::clone 共享同一个 Arc<Db>。
    /// 保留给需要独立事务隔离的测试场景。
    pub fn connect(path: &Path) -> Result<Arc<Self>> {
        let conn = Self::open_connection(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        Ok(Arc::new(Self { conn: Mutex::new(conn) }))
    }

    fn open_connection(path: &Path, flags: OpenFlags) -> Result<Connection> {
        let conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    /// 内存 DB,给单元测试用 —— 不需要 Arc(测试不跨线程),直接返回 Db。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        run_schemas(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 取出底层 Connection 的临时借用。生命周期受 Db 本身,
    /// guard drop 时锁释放。事务代码用这条 (db.lock().unchecked_transaction())。
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("db lock poisoned")
    }

    /// 直接执行一段 SQL —— 启动期迁移 / 测试 fixture 用。
    pub fn execute_batch(&self, sql: &str) -> rusqlite::Result<()> {
        let guard = self.conn.lock().expect("db lock");
        guard.execute_batch(sql)
    }

    pub fn uploads(&self) -> UploadRepo<'_> { UploadRepo { conn: self.lock() } }
    pub fn chapters(&self) -> ChapterRepo<'_> { ChapterRepo { conn: self.lock() } }
    pub fn transformation_novels(&self) -> TransformationNovelRepo<'_> {
        TransformationNovelRepo { conn: self.lock() }
    }
    pub fn transformation_chapters(&self) -> TransformationChapterRepo<'_> {
        TransformationChapterRepo { conn: self.lock() }
    }
    pub fn prompts(&self) -> PromptRepo<'_> { PromptRepo { conn: self.lock() } }
    pub fn model_configs(&self) -> ModelConfigRepo<'_> { ModelConfigRepo { conn: self.lock() } }
    pub fn data_assets(&self) -> DataAssetRepo<'_> { DataAssetRepo { conn: self.lock() } }
    pub fn promotion(&self) -> crate::db::repo::promotion::PromotionRepo<'_> {
        crate::db::repo::promotion::PromotionRepo { conn: self.lock() }
    }
    pub fn batches(&self) -> BatchRepo<'_> { BatchRepo { conn: self.lock() } }
    pub fn workflow_results(&self) -> WorkflowResultRepo<'_> {
        WorkflowResultRepo { conn: self.lock() }
    }
    pub fn overview(&self) -> OverviewRepo<'_> { OverviewRepo { conn: self.lock() } }
    pub fn ai_call_logs(&self) -> AiCallLogRepo<'_> { AiCallLogRepo { conn: self.lock() } }
    pub fn chapter_previews(&self) -> ChapterPreviewRepo<'_> {
        ChapterPreviewRepo { conn: self.lock() }
    }

    pub fn seed_builtin_prompts(&self) -> Result<()> {
        self.prompts().seed_builtin_if_empty()
    }

    pub fn applied_schema_versions(&self) -> Result<Vec<String>> {
        let guard = self.conn.lock().expect("db lock");
        let mut stmt = guard.prepare(
            "SELECT version FROM schema_versions ORDER BY LENGTH(version), version",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn run_schemas(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_versions (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    for (version, sql) in SCHEMAS {
        let applied: bool = conn.query_row(
            "SELECT 1 FROM schema_versions WHERE version = ?1",
            [version],
            |_| Ok(true),
        ).unwrap_or(false);
        if applied {
            continue;
        }
        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opens_in_memory_and_seeds_schema() {
        let db = Db::open_in_memory().unwrap();
        let _uploads = db.uploads();
    }

    #[test]
    fn runtime_connection_rejects_uninitialized_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        assert!(Db::connect(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn runtime_connection_opens_while_another_connection_is_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.db");
        let initialized = Db::open(&path).unwrap();
        initialized.lock().execute_batch("BEGIN IMMEDIATE").unwrap();

        let runtime = Db::connect(&path).unwrap();

        assert_eq!(
            runtime.applied_schema_versions().unwrap(),
            initialized.applied_schema_versions().unwrap(),
        );
    }
}
