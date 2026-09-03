use std::sync::MutexGuard;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{
    NewTransformationChapter, PromptKind, TransformationChapter, TransformStatus,
};

pub struct TransformationChapterRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> TransformationChapterRepo<'a> {
    pub fn insert(&self, t: &NewTransformationChapter) -> Result<i64> {
        let mode = match t.mode {
            PromptKind::Compress => "compress",
            PromptKind::Style => "style",
        };
        self.conn.execute(
            "INSERT INTO transformation_chapters \
             (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
              batch_id, style_ref_chapter_id, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
            params![
                t.transformation_novel_id, t.chapter_id, mode, t.prompt_id, t.model_config_id,
                t.ctx_prev_original, t.ctx_prev_transformed, t.ctx_next_original,
                t.batch_id, t.style_ref_chapter_id,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<TransformationChapter>> {
        let sql = format!("{SELECT_SQL} WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    /// 同一章节的所有转换(历史全留,按 id desc)。
    pub fn list_by_chapter(&self, chapter_id: i64) -> Result<Vec<TransformationChapter>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE chapter_id = ?1 ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map(params![chapter_id], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 同一 transformation_novel 的所有转换。
    pub fn list_by_transformation_novel(
        &self,
        transformation_novel_id: i64,
    ) -> Result<Vec<TransformationChapter>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE transformation_novel_id = ?1 ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map(params![transformation_novel_id], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_by_status(&self, status: TransformStatus) -> Result<Vec<TransformationChapter>> {
        let s = status_str(status);
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE status = ?1 ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map(params![s], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 同一 batch 内所有 tc 行,按 chapter_idx ASC 排(join chapters 表)。
    /// 排序列 idx 在重排序时稳定;同 idx 用 tc.id 兜底。
    pub fn list_by_batch(&self, batch_id: i64) -> Result<Vec<TransformationChapter>> {
        // 显式列前缀避免 SELECT id 歧义(chapters / transformation_chapters 都有 id)。
        let sql = "SELECT transformation_chapters.id, transformation_chapters.transformation_novel_id, \
                    transformation_chapters.chapter_id, transformation_chapters.mode, \
                    transformation_chapters.prompt_id, transformation_chapters.model_config_id, \
                    transformation_chapters.ctx_prev_original, \
                    transformation_chapters.ctx_prev_transformed, \
                    transformation_chapters.ctx_next_original, \
                    transformation_chapters.status, transformation_chapters.result_content, \
                    transformation_chapters.tokens_in, transformation_chapters.tokens_out, \
                    transformation_chapters.error, transformation_chapters.started_at, \
                    transformation_chapters.completed_at, transformation_chapters.batch_id, \
                    transformation_chapters.style_ref_chapter_id \
             FROM transformation_chapters \
             JOIN chapters c ON c.id = transformation_chapters.chapter_id \
             WHERE transformation_chapters.batch_id = ?1 \
             ORDER BY c.idx ASC, transformation_chapters.id ASC".to_string();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![batch_id], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 同一 batch 内 tc 行数(给 UI 进度条用)。
    pub fn count_by_batch(&self, batch_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id = ?1",
            params![batch_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn mark_running(&self, id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters SET status='running', started_at=?2 WHERE id=?1",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn mark_done(&self, id: i64, result_content: String, tokens_in: i32, tokens_out: i32) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        // NULLIF(?2,'') 让 worker 在 spec §5.x 收口后传空串时,result_content 列保持 NULL
        // (正文写在 workflow_result_chapters.content 槽);其他调用方传实际正文时不变。
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='done', result_content=NULLIF(?2,''), tokens_in=?3, tokens_out=?4, completed_at=?5 \
             WHERE id=?1",
            params![id, result_content, tokens_in, tokens_out, now],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, id: i64, error: String) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='failed', error=?2, completed_at=?3 WHERE id=?1",
            params![id, error, now],
        )?;
        Ok(())
    }

    /// 标 skipped —— 保留 error 字段（用户事后能看到原因）；清空 result_content 与 tokens。
    pub fn mark_skipped(&self, id: i64, error: String) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='skipped', error=?2, result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
                 completed_at=?3 WHERE id=?1",
            params![id, error, now],
        )?;
        Ok(())
    }

    pub fn reset_to_pending(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='pending', result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
                 error=NULL, started_at=NULL, completed_at=NULL \
             WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }
}

const SELECT_SQL: &str =
    "SELECT id, transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
            ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
            status, result_content, tokens_in, tokens_out, \
            error, started_at, completed_at, batch_id, style_ref_chapter_id \
     FROM transformation_chapters";

fn parse_ts(idx: usize, s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            idx, rusqlite::types::Type::Text, Box::new(e)))
}

fn from_row(row: &Row) -> rusqlite::Result<TransformationChapter> {
    let mode_s: String = row.get(3)?;
    let status_s: String = row.get(9)?;
    let started: Option<String> = row.get(14)?;
    let completed: Option<String> = row.get(15)?;
    Ok(TransformationChapter {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        chapter_id: row.get(2)?,
        mode: match mode_s.as_str() {
            "compress" => PromptKind::Compress,
            _ => PromptKind::Style,
        },
        prompt_id: row.get(4)?,
        model_config_id: row.get(5)?,
        ctx_prev_original: row.get(6)?,
        ctx_prev_transformed: row.get(7)?,
        ctx_next_original: row.get(8)?,
        status: match status_s.as_str() {
            "pending" => TransformStatus::Pending,
            "running" => TransformStatus::Running,
            "done" => TransformStatus::Done,
            "failed" => TransformStatus::Failed,
            "skipped" => TransformStatus::Skipped,
            _ => TransformStatus::Cancelled,
        },
        result_content: row.get(10)?,
        tokens_in: row.get(11)?,
        tokens_out: row.get(12)?,
        error: row.get(13)?,
        started_at: started.as_deref().map(|s| parse_ts(14, s)).transpose()?,
        completed_at: completed.as_deref().map(|s| parse_ts(15, s)).transpose()?,
        batch_id: row.get(16)?,
        style_ref_chapter_id: row.get(17)?,
    })
}

fn status_str(s: TransformStatus) -> &'static str {
    match s {
        TransformStatus::Pending => "pending",
        TransformStatus::Running => "running",
        TransformStatus::Done => "done",
        TransformStatus::Failed => "failed",
        TransformStatus::Skipped => "skipped",
        TransformStatus::Cancelled => "cancelled",
    }
}