use std::sync::MutexGuard;
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{Chapter, NewChapter};

pub struct ChapterRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

fn chapter_from_row(row: &Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        idx: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        word_count: row.get(5)?,
        source_chapter_id: row.get(6)?,
        source_kind: row.get(7)?,
        edited_at: row.get(8)?,
        title_line: row.get(9)?,
    })
}

impl<'a> ChapterRepo<'a> {
    pub fn insert(&self, c: &NewChapter) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO chapters (data_asset_id, idx, title, body, word_count, source_kind, source_chapter_id, title_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![c.data_asset_id, c.idx, c.title, c.body, c.word_count, c.source_kind.clone(), c.source_chapter_id, c.title_line],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_many(&self, data_asset_id: i64, items: &[NewChapter]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chapters (data_asset_id, idx, title, body, word_count, title_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for c in items {
                stmt.execute(params![data_asset_id, c.idx, c.title, c.body, c.word_count, c.title_line])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count, source_chapter_id, source_kind, edited_at, title_line FROM chapters WHERE data_asset_id = ?1 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![data_asset_id], chapter_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 编辑单章正文:更新 body 并按统一口径(word::count)重算 word_count。
    /// 不动 idx / title / source_kind / source_chapter_id —— 这些是结构字段。
    pub fn update_body(&self, id: i64, new_body: &str) -> Result<()> {
        let wc = crate::text::word_count(new_body) as i64;
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE chapters SET body = ?2, word_count = ?3, edited_at = ?4 WHERE id = ?1",
            params![id, new_body, wc, now],
        )?;
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Option<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count, source_chapter_id, source_kind, edited_at, title_line FROM chapters WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(chapter_from_row(row)?))
        } else { Ok(None) }
    }

    pub fn prev_n(&self, data_asset_id: i64, before_idx: i32, n: i32) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count, source_chapter_id, source_kind, edited_at, title_line FROM chapters WHERE data_asset_id = ?1 AND idx < ?2              ORDER BY idx DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![data_asset_id, before_idx, n], chapter_from_row)?;
        let mut v: Vec<Chapter> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        v.reverse();
        Ok(v)
    }

    pub fn next_n(&self, data_asset_id: i64, after_idx: i32, n: i32) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count, source_chapter_id, source_kind, edited_at, title_line FROM chapters WHERE data_asset_id = ?1 AND idx > ?2              ORDER BY idx ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![data_asset_id, after_idx, n], chapter_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 重新计算所有 chapter 的 word_count(不再过滤 word_count = 0)。
    /// 字数定义改了(包含标点)后用这个一次性同步;幂等,Db::open 跑一次就行。
    pub fn recompute_all_word_count(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id, body FROM chapters              WHERE length(body) > 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut updated = 0;
        for row in rows {
            let (id, text) = row?;
            let wc = crate::text::word_count(&text) as i64;
            self.conn.execute(
                "UPDATE chapters SET word_count = ?2 WHERE id = ?1",
                params![id, wc],
            )?;
            updated += 1;
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::{NewDataAsset, NewUpload};

    #[test]
    fn title_line_round_trips_through_db() {
        let db = Db::open_in_memory().unwrap();
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "abc".into(),
            filename: "t.txt".into(),
            byte_size: 0,
            file_path: String::new(),
            original_text: String::new(),
            word_count: 0,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id,
            title: "DA".into(),
            source_filename: "t.txt".into(),
            ..Default::default()
        }).unwrap();
        let id = db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            title: "Chapter 1".into(),
            body: "body".into(),
            word_count: 1,
            title_line: Some(42),
            ..Default::default()
        }).unwrap();
        let got = db.chapters().get(id).unwrap().unwrap();
        assert_eq!(got.title_line, Some(42), "title_line 没被写入第 9 列或读出列序错位");
    }

    #[test]
    fn title_line_null_round_trips_through_db() {
        let db = Db::open_in_memory().unwrap();
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "def".into(),
            filename: "u.txt".into(),
            byte_size: 0,
            file_path: String::new(),
            original_text: String::new(),
            word_count: 0,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id,
            title: "DA".into(),
            source_filename: "u.txt".into(),
            ..Default::default()
        }).unwrap();
        let id = db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            title: "Promoted".into(),
            body: "x".into(),
            word_count: 1,
            ..Default::default() // title_line: None
        }).unwrap();
        let got = db.chapters().get(id).unwrap().unwrap();
        assert_eq!(got.title_line, None, "title_line: None 应存为 NULL 并读回 None");
    }
}
