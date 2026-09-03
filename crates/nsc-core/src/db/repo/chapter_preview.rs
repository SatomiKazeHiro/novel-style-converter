use std::sync::MutexGuard;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{ChapterPreviewRow, PreviewStatus};

pub struct ChapterPreviewRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> ChapterPreviewRepo<'a> {
    /// 插入一条 status='generating' 的预览行,返回新 id。
    /// `custom_input` 为 None 时存 NULL —— 用户没填附加指令,与原 transform 路径 byte-equal。
    pub fn insert_generating(
        &self,
        batch_id: i64,
        chapter_id: i64,
        custom_input: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO chapter_previews \
             (batch_id, chapter_id, custom_input, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'generating', ?4, ?4)",
            params![batch_id, chapter_id, custom_input, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 标记预览完成:写入 preview_content + tokens + updated_at = 当前 UTC。
    pub fn update_done(
        &self,
        id: i64,
        preview_content: &str,
        tokens_in: i32,
        tokens_out: i32,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE chapter_previews \
             SET status='done', preview_content=NULLIF(?2,''), \
                 tokens_in=?3, tokens_out=?4, error=NULL, updated_at=?5 \
             WHERE id=?1",
            params![id, preview_content, tokens_in, tokens_out, now],
        )?;
        Ok(())
    }

    /// 标记预览失败:写入 error + updated_at = 当前 UTC。
    pub fn update_failed(&self, id: i64, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE chapter_previews \
             SET status='failed', error=?2, updated_at=?3 \
             WHERE id=?1",
            params![id, error, now],
        )?;
        Ok(())
    }

    /// 同一 (batch_id, chapter_id) 下的全部预览,按 id DESC 排序 —— UI tab 默认按新→旧展示。
    pub fn list_by_chapter(
        &self,
        batch_id: i64,
        chapter_id: i64,
    ) -> Result<Vec<ChapterPreviewRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE batch_id = ?1 AND chapter_id = ?2 ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map(params![batch_id, chapter_id], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<ChapterPreviewRow>> {
        let mut stmt = self.conn.prepare(&format!("{SELECT_SQL} WHERE id = ?1"))?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM chapter_previews WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 提交预览后清理:删除该 (batch_id, chapter_id) 下所有 preview,返回删除行数。
    pub fn delete_by_chapter(&self, batch_id: i64, chapter_id: i64) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM chapter_previews WHERE batch_id = ?1 AND chapter_id = ?2",
            params![batch_id, chapter_id],
        )?;
        Ok(n)
    }
}

const SELECT_SQL: &str =
    "SELECT id, batch_id, chapter_id, custom_input, preview_content, \
            tokens_in, tokens_out, error, status, created_at, updated_at \
     FROM chapter_previews";

fn parse_ts(idx: usize, s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            idx, rusqlite::types::Type::Text, Box::new(e)))
}

fn from_row(row: &Row) -> rusqlite::Result<ChapterPreviewRow> {
    let status_s: String = row.get(8)?;
    let created: String = row.get(9)?;
    let updated: String = row.get(10)?;
    let status = PreviewStatus::from_str(&status_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    Ok(ChapterPreviewRow {
        id: row.get(0)?,
        batch_id: row.get(1)?,
        chapter_id: row.get(2)?,
        custom_input: row.get(3)?,
        preview_content: row.get(4)?,
        tokens_in: row.get(5)?,
        tokens_out: row.get(6)?,
        error: row.get(7)?,
        status,
        created_at: parse_ts(9, &created)?,
        updated_at: parse_ts(10, &updated)?,
    })
}
