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

    /// 建 upload + da,返回 da_id。sha 必须唯一(有 UNIQUE 约束)。
    fn da(db: &Db, sha: &str) -> i64 {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: sha.into(), filename: "t.txt".into(), byte_size: 0,
            file_path: String::new(), original_text: String::new(), word_count: 0,
        }).unwrap();
        db.data_assets().insert(&NewDataAsset {
            upload_id, title: "DA".into(), source_filename: "t.txt".into(),
            ..Default::default()
        }).unwrap()
    }

    fn ch(da_id: i64, idx: i32, body: &str) -> NewChapter {
        NewChapter {
            data_asset_id: da_id, idx, title: format!("第{idx}章"), body: body.into(),
            word_count: crate::text::word_count(body), ..Default::default()
        }
    }

    // ── prev_n / next_n(邻章上下文的数据源) ────────────────────────────────
    //
    // 这两个方法曾经出过 bug:旧实现拉全部旧章再 `.take(n)`,拿到的是**最旧** N 章
    // 而不是**最近** N 章。`tests/transformer_ctx.rs` 从 read_context 侧覆盖了它,
    // 但 repo 自身的语义(升序返回、边界、n 的含义)此前没有直接测试。

    /// prev_n 返回**最近** N 章,且按 idx **升序**(最近的排最后)——
    /// 这样调用方拼进 prompt 的顺序就是时间顺序。
    #[test]
    fn prev_n_returns_nearest_n_ascending() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "p1");
        for i in 1..=5 {
            db.chapters().insert(&ch(da_id, i, "正文")).unwrap();
        }
        // 当前章 idx=5,取最近 2 章 → 应是 idx 3、4(不是 1、2),且升序
        let got: Vec<i32> = db.chapters().prev_n(da_id, 5, 2).unwrap()
            .iter().map(|c| c.idx).collect();
        assert_eq!(got, vec![3, 4], "应是最近 2 章且升序(idx 3, 4),实际 {got:?}");

        // 取 4 章 → 1..=4
        let got: Vec<i32> = db.chapters().prev_n(da_id, 5, 4).unwrap()
            .iter().map(|c| c.idx).collect();
        assert_eq!(got, vec![1, 2, 3, 4]);
    }

    /// prev_n 在首章处返回空(没有"上一章"),n=0 也返回空。
    #[test]
    fn prev_n_boundary_is_empty() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "p2");
        for i in 1..=3 {
            db.chapters().insert(&ch(da_id, i, "正文")).unwrap();
        }
        assert!(db.chapters().prev_n(da_id, 1, 2).unwrap().is_empty(), "首章没有上一章");
        assert!(db.chapters().prev_n(da_id, 3, 0).unwrap().is_empty(), "n=0 应返回空");
    }

    /// next_n 返回**接下来** N 章,按 idx 升序。
    #[test]
    fn next_n_returns_following_n_ascending() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "p3");
        for i in 1..=5 {
            db.chapters().insert(&ch(da_id, i, "正文")).unwrap();
        }
        let got: Vec<i32> = db.chapters().next_n(da_id, 1, 2).unwrap()
            .iter().map(|c| c.idx).collect();
        assert_eq!(got, vec![2, 3], "应是紧接着的 2 章");
        // 末章之后为空
        assert!(db.chapters().next_n(da_id, 5, 2).unwrap().is_empty());
    }

    /// prev_n / next_n 必须**限定在同一个 data_asset 内** ——
    /// 否则邻章上下文会串到别的书里去。
    #[test]
    fn neighbor_queries_are_scoped_to_one_data_asset() {
        let db = Db::open_in_memory().unwrap();
        let a = da(&db, "p4");
        let b = da(&db, "p5");
        for i in 1..=3 {
            db.chapters().insert(&ch(a, i, "A 的正文")).unwrap();
            db.chapters().insert(&ch(b, i, "B 的正文")).unwrap();
        }
        let prev_bodies: Vec<String> = db.chapters().prev_n(a, 3, 5).unwrap()
            .iter().map(|c| c.body.clone()).collect();
        assert!(
            prev_bodies.iter().all(|t| t.starts_with('A')),
            "不应取到另一个 data_asset 的章节: {prev_bodies:?}"
        );
        let next_bodies: Vec<String> = db.chapters().next_n(a, 1, 5).unwrap()
            .iter().map(|c| c.body.clone()).collect();
        assert!(next_bodies.iter().all(|t| t.starts_with('A')));
    }

    /// list_by_data_asset 按 idx 升序(与插入顺序无关)。
    #[test]
    fn list_by_data_asset_is_ordered_by_idx() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "p6");
        // 乱序插入
        for i in [3, 1, 5, 2, 4] {
            db.chapters().insert(&ch(da_id, i, "正文")).unwrap();
        }
        let idxs: Vec<i32> = db.chapters().list_by_data_asset(da_id).unwrap()
            .iter().map(|c| c.idx).collect();
        assert_eq!(idxs, vec![1, 2, 3, 4, 5]);
    }

    /// `UNIQUE(data_asset_id, idx)`:同一资产内 idx 重复必须失败
    /// (调度器与邻章查询都依赖 idx 唯一)。
    #[test]
    fn duplicate_idx_within_same_data_asset_is_rejected() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "p7");
        db.chapters().insert(&ch(da_id, 1, "正文")).unwrap();
        assert!(db.chapters().insert(&ch(da_id, 1, "另一段")).is_err(),
            "同 da 内重复 idx 应撞唯一约束");
        // 不同 da 用同一个 idx 是允许的(每本书各自从 1 开始)
        let other = da(&db, "p8");
        assert!(db.chapters().insert(&ch(other, 1, "正文")).is_ok());
    }

    // ── insert_many / update_body / recompute ────────────────────────────────

    /// insert_many 一次写入多章,且按 items 顺序落库。
    #[test]
    fn insert_many_writes_all_chapters() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "m1");
        let items: Vec<NewChapter> = (1..=4).map(|i| ch(da_id, i, "正文")).collect();
        db.chapters().insert_many(da_id, &items).unwrap();
        let idxs: Vec<i32> = db.chapters().list_by_data_asset(da_id).unwrap()
            .iter().map(|c| c.idx).collect();
        assert_eq!(idxs, vec![1, 2, 3, 4]);
    }

    /// insert_many 是**事务**:中途撞唯一约束时整批回滚,不留半批数据。
    /// (导入章节时若留半批,用户会看到"少了几章"且不知为何。)
    #[test]
    fn insert_many_rolls_back_entirely_on_conflict() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "m2");
        db.chapters().insert(&ch(da_id, 2, "已存在的第2章")).unwrap();

        let items = vec![ch(da_id, 1, "新1"), ch(da_id, 2, "撞车的2"), ch(da_id, 3, "新3")];
        assert!(db.chapters().insert_many(da_id, &items).is_err(), "应撞唯一约束");

        let after: Vec<i32> = db.chapters().list_by_data_asset(da_id).unwrap()
            .iter().map(|c| c.idx).collect();
        assert_eq!(after, vec![2], "失败的批次应整体回滚,只剩原本那一章: {after:?}");
    }

    /// update_body:更新正文并**按统一口径重算 word_count**,同时写 edited_at;
    /// 结构字段(idx / title / source_kind)不动。
    #[test]
    fn update_body_recomputes_word_count_and_marks_edited() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "u1");
        let id = db.chapters().insert(&ch(da_id, 7, "原文")).unwrap();
        let before = db.chapters().get(id).unwrap().unwrap();
        assert!(before.edited_at.is_none(), "从未编辑过应为 None");

        db.chapters().update_body(id, "改后的正文内容").unwrap();

        let after = db.chapters().get(id).unwrap().unwrap();
        assert_eq!(after.body, "改后的正文内容");
        assert_eq!(after.word_count, 7, "字数应按新正文重算");
        assert!(after.edited_at.is_some(), "编辑后应写 edited_at");
        assert_eq!(after.idx, before.idx, "idx 是结构字段,不该动");
        assert_eq!(after.title, before.title);
        assert_eq!(after.source_kind, before.source_kind);
    }

    /// recompute 重算所有非空正文的章节字数;空正文跳过。
    #[test]
    fn recompute_overwrites_stale_word_counts() {
        let db = Db::open_in_memory().unwrap();
        let da_id = da(&db, "rc1");
        let a = db.chapters().insert(&ch(da_id, 1, "三个字")).unwrap();
        let b = db.chapters().insert(&ch(da_id, 2, "")).unwrap();
        db.lock().execute("UPDATE chapters SET word_count = 999 WHERE id = ?1", params![a]).unwrap();

        let n = db.chapters().recompute_all_word_count().unwrap();
        assert_eq!(n, 1, "只统计有正文的章节");
        assert_eq!(db.chapters().get(a).unwrap().unwrap().word_count, 3);
        assert_eq!(db.chapters().get(b).unwrap().unwrap().word_count, 0);
    }
}
