use std::sync::MutexGuard;
use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::error::Result;
use crate::models::{NewTransformationNovel, NewUpload, TransformationNovel, Upload};

pub struct UploadRepo<'a> { pub(crate) conn: MutexGuard<'a, rusqlite::Connection> }

impl<'a> UploadRepo<'a> {
    pub fn insert(&self, u: &NewUpload) -> Result<i64> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO uploads (sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count)              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![u.sha256, u.filename, u.byte_size, now.to_rfc3339(), u.file_path, u.original_text, u.word_count],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 同 hash 复用现有 upload。返回 existing id;若不存在返回 None。
    pub fn find_by_sha256(&self, sha256: &str) -> Result<Option<i64>> {
        let mut stmt = self.conn.prepare("SELECT id FROM uploads WHERE sha256 = ?1")?;
        let mut rows = stmt.query(params![sha256])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else { Ok(None) }
    }

    pub fn get(&self, id: i64) -> Result<Option<Upload>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count              FROM uploads WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<Upload>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count              FROM uploads ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 把原文整篇写回 uploads.original_text(用于清洗/重解析等需要重写原文的路径)。
    /// 同步刷新 word_count:原文变了,字数跟着变,避免 list 显示旧值。
    pub fn set_original_text(&self, id: i64, text: &str) -> Result<()> {
        let wc = crate::text::word_count(text) as i64;
        self.conn.execute(
            "UPDATE uploads SET original_text = ?2, word_count = ?3 WHERE id = ?1",
            params![id, text, wc],
        )?;
        Ok(())
    }

    /// 把 `word_count = 0` 且 `original_text` 非空的 upload 行用真实字符数回填。
    ///
    /// Migration 0007 加 `uploads.word_count` 时给老行填了默认值 0;此函数在
    /// `Db::open` 末尾跑一次,把这些行的 word_count 用已存的 original_text 重算。
    /// 幂等:重跑只触发一次 UPDATE(已经在的字数已正确)。空 original_text 的
    /// 极老 upload 留 0(原文没存进 DB,需要重传才能填)。
    ///
    /// 返回回填的行数(给日志/测试用)。
    pub fn backfill_word_count(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original_text FROM uploads              WHERE word_count = 0 AND length(original_text) > 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut updated = 0;
        for row in rows {
            let (id, text) = row?;
            let wc = crate::text::word_count(&text) as i64;
            if wc > 0 {
                self.conn.execute(
                    "UPDATE uploads SET word_count = ?2 WHERE id = ?1",
                    params![id, wc],
                )?;
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// 重新计算所有 upload 的 word_count(不再过滤 word_count = 0)。
    /// 字数定义改了(包含标点)后用这个一次性同步;幂等,Db::open 跑一次就行。
    /// 返回更新的行数(给日志/测试用)。
    pub fn recompute_all_word_count(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original_text FROM uploads              WHERE length(original_text) > 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut updated = 0;
        for row in rows {
            let (id, text) = row?;
            let wc = crate::text::word_count(&text) as i64;
            self.conn.execute(
                "UPDATE uploads SET word_count = ?2 WHERE id = ?1",
                params![id, wc],
            )?;
            updated += 1;
        }
        Ok(updated)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM uploads WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn from_row(row: &Row) -> rusqlite::Result<Upload> {
    let uploaded_at_s: String = row.get(4)?;
    let uploaded_at = DateTime::parse_from_rfc3339(&uploaded_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            4, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(Upload {
        id: row.get(0)?,
        sha256: row.get(1)?,
        filename: row.get(2)?,
        byte_size: row.get(3)?,
        uploaded_at,
        file_path: row.get(5)?,
        original_text: row.get(6)?,
        word_count: row.get(7)?,
    })
}

pub struct TransformationNovelRepo<'a> { pub(crate) conn: MutexGuard<'a, rusqlite::Connection> }

impl<'a> TransformationNovelRepo<'a> {
    /// 创建 transformation_novel。是否被引用看 `transformation_novels` 真实行,
    /// 前端按钮按 join 出来的 tn_count 走。
    pub fn insert(&self, n: &NewTransformationNovel) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO transformation_novels (data_asset_id, title, note, created_at)              VALUES (?1, ?2, ?3, ?4)",
            params![n.data_asset_id, n.title, n.note, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, note, created_at              FROM transformation_novels WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(novel_from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, note, created_at              FROM transformation_novels ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], novel_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn update(&self, n: &TransformationNovel) -> Result<()> {
        self.conn.execute(
            "UPDATE transformation_novels SET title = ?2, note = ?3 WHERE id = ?1",
            params![n.id, n.title, n.note],
        )?;
        Ok(())
    }

    pub fn list_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, note, created_at              FROM transformation_novels WHERE data_asset_id = ?1 ORDER BY id DESC"
        )?;
        let rows = stmt.query_map(params![data_asset_id], novel_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM transformation_novels WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn novel_from_row(row: &Row) -> rusqlite::Result<TransformationNovel> {
    let created_at_s: String = row.get(4)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            4, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(TransformationNovel {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        title: row.get(2)?,
        note: row.get(3)?,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::{NewDataAsset, NewUpload};

    fn upload(sha: &str, text: &str) -> NewUpload {
        NewUpload {
            sha256: sha.into(), filename: "f.txt".into(), byte_size: text.len() as i64,
            file_path: "/tmp/f.txt".into(), original_text: text.into(),
            word_count: 0, // 交给 set_original_text / backfill 去算
        }
    }

    fn fresh() -> Db {
        Db::open_in_memory().unwrap()
    }

    /// 直接改库(绕过 repo)—— 用独立语句取锁,避免与存活的 repo guard 自死锁。
    /// `db.lock()` 不可重入:一边持有 `db.xxx()` 的 guard 一边取锁会永久挂住。
    fn exec(db: &Db, sql: &str, args: &[&dyn rusqlite::ToSql]) {
        db.lock().execute(sql, args).unwrap();
    }

    fn da(db: &Db, sha: &str) -> i64 {
        let upload_id = db.uploads().insert(&upload(sha, "原文")).unwrap();
        db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap()
    }

    // ── UploadRepo ──────────────────────────────────────────────────────────

    /// `sha256` 去重依赖 `find_by_sha256` —— 它返回已存在行的 id 供上层复用。
    #[test]
    fn find_by_sha256_finds_existing_and_misses_absent() {
        let db = fresh();
        let r = db.uploads();
        let id = r.insert(&upload("sha-a", "正文")).unwrap();
        assert_eq!(r.find_by_sha256("sha-a").unwrap(), Some(id));
        assert_eq!(r.find_by_sha256("sha-b").unwrap(), None);
    }

    /// `UNIQUE(sha256)` 是去重的前提 —— 重复插入必须失败(上层靠这个 + find_by_sha256
    /// 实现"同文件不重复导入")。
    #[test]
    fn duplicate_sha256_insert_fails() {
        let db = fresh();
        let r = db.uploads();
        r.insert(&upload("dup", "第一次")).unwrap();
        assert!(r.insert(&upload("dup", "第二次")).is_err(), "重复 sha256 应撞唯一约束");
    }

    /// `set_original_text` 改正文的**同时**重算 word_count —— 清洗/编辑路径靠它。
    #[test]
    fn set_original_text_recomputes_word_count() {
        let db = fresh();
        let r = db.uploads();
        let id = r.insert(&upload("s1", "短")).unwrap();
        r.set_original_text(id, "一二三四五").unwrap();
        let u = r.get(id).unwrap().unwrap();
        assert_eq!(u.original_text, "一二三四五");
        assert_eq!(u.word_count, 5, "字数应跟着正文更新");

        r.set_original_text(id, "更短").unwrap();
        let u = r.get(id).unwrap().unwrap();
        assert_eq!(u.word_count, 2);
    }

    /// backfill 只补 `word_count = 0` 的行(幂等):已算好的不动,空原文的留 0。
    #[test]
    fn backfill_only_fills_zero_word_count_rows() {
        let db = fresh();
        let needs = db.uploads().insert(&upload("b1", "五个字啊哈")).unwrap();
        let already = db.uploads().insert(&upload("b2", "已有字数")).unwrap();
        let empty = db.uploads().insert(&upload("b3", "")).unwrap();
        exec(&db, "UPDATE uploads SET word_count = 7 WHERE id = ?1", &[&already]);

        let n = db.uploads().backfill_word_count().unwrap();
        assert_eq!(n, 1, "只应回填 word_count=0 且有正文的那一行");
        let r = db.uploads();
        assert_eq!(r.get(needs).unwrap().unwrap().word_count, 5);
        assert_eq!(r.get(already).unwrap().unwrap().word_count, 7, "已算好的不应被改");
        assert_eq!(r.get(empty).unwrap().unwrap().word_count, 0, "空原文留 0(需重传)");
        drop(r);
        // 幂等:再跑一次没有可回填的行
        assert_eq!(db.uploads().backfill_word_count().unwrap(), 0);
    }

    /// recompute 重算**所有**有正文的行(字数定义变更后一次性同步),空原文跳过。
    #[test]
    fn recompute_overwrites_all_non_empty_rows() {
        let db = fresh();
        let a = db.uploads().insert(&upload("r1", "三个字")).unwrap(); // 3 字
        let b = db.uploads().insert(&upload("r2", "两字")).unwrap();   // 2 字
        let empty = db.uploads().insert(&upload("r3", "")).unwrap();
        // 故意写错,验证 recompute 会覆盖
        exec(&db, "UPDATE uploads SET word_count = 999 WHERE id IN (?1, ?2)", &[&a, &b]);

        let n = db.uploads().recompute_all_word_count().unwrap();
        assert_eq!(n, 2, "只统计有正文的行(空串不计)");
        let r = db.uploads();
        assert_eq!(r.get(a).unwrap().unwrap().word_count, 3);
        assert_eq!(r.get(b).unwrap().unwrap().word_count, 2);
        assert_eq!(r.get(empty).unwrap().unwrap().word_count, 0);
    }

    /// list 按 id DESC(最新上传在前);get 对不存在的 id 返回 None。
    #[test]
    fn list_is_desc_and_get_missing_is_none() {
        let db = fresh();
        let r = db.uploads();
        let first = r.insert(&upload("l1", "一")).unwrap();
        let second = r.insert(&upload("l2", "二")).unwrap();
        let ids: Vec<i64> = r.list().unwrap().iter().map(|u| u.id).collect();
        assert_eq!(ids, vec![second, first]);
        assert!(r.get(99999).unwrap().is_none());
    }

    /// 删 upload **不**级联删 data_assets —— migration 0015 有意把
    /// `data_assets.upload_id` 改成**审计式软引用(无 FK、无 UNIQUE)**:
    /// "单一方向:chapter.body 是自包含的;data_asset 仅审计式引用 upload_id"。
    /// 所以删掉原文文件后,已解析出的数据资产仍然保留(它自带正文)。
    /// 这条记录该设计,免得后来者以为是漏了 cascade 而"修"回去。
    #[test]
    fn delete_upload_keeps_data_assets_by_design() {
        let db = fresh();
        let upload_id = db.uploads().insert(&upload("d1", "正文")).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        db.uploads().delete(upload_id).unwrap();
        assert!(db.uploads().get(upload_id).unwrap().is_none());
        assert!(
            db.data_assets().get(da_id).unwrap().is_some(),
            "da 应保留(0015 起 upload_id 是软引用,不再级联)"
        );
    }

    /// 而 `PRAGMA foreign_key_list(data_assets)` 里确实没有 upload_id 那条外键 ——
    /// 这是 0015 重建表时有意为之,不是漏配。用断言把这个 schema 事实钉住。
    #[test]
    fn data_assets_has_no_upload_fk_by_design() {
        let db = fresh();
        let g = db.lock();
        let mut stmt = g.prepare("PRAGMA foreign_key_list(data_assets)").unwrap();
        let from_cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            !from_cols.iter().any(|c| c == "upload_id"),
            "upload_id 不应有外键(0015 改为软引用),实际外键列: {from_cols:?}"
        );
        assert!(
            from_cols.iter().any(|c| c == "source_workflow_id"),
            "0021 新增的 source_workflow_id 外键应存在"
        );
    }

    // ── TransformationNovelRepo ─────────────────────────────────────────────

    /// tn 的字段往返,note 可为空串("无备注")。
    #[test]
    fn tn_insert_and_get_roundtrip() {
        let db = fresh();
        let da_id = da(&db, "t1");
        let r = db.transformation_novels();
        let id = r.insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "我的工程".into(), note: String::new(),
        }).unwrap();
        let tn = r.get(id).unwrap().unwrap();
        assert_eq!(tn.title, "我的工程");
        assert_eq!(tn.note, "");
        assert_eq!(tn.data_asset_id, da_id);
    }

    /// update 只改 title / note,不动 data_asset_id 与 created_at。
    #[test]
    fn tn_update_changes_only_title_and_note() {
        let db = fresh();
        let da_id = da(&db, "t2");
        let r = db.transformation_novels();
        let id = r.insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "旧".into(), note: "旧备注".into(),
        }).unwrap();
        let before = r.get(id).unwrap().unwrap();

        let mut edited = before.clone();
        edited.title = "新".into();
        edited.note = "新备注".into();
        edited.data_asset_id = 99999; // 不该被写进去
        r.update(&edited).unwrap();

        let after = r.get(id).unwrap().unwrap();
        assert_eq!(after.title, "新");
        assert_eq!(after.note, "新备注");
        assert_eq!(after.data_asset_id, before.data_asset_id, "update 不应改归属 da");
        assert_eq!(after.created_at, before.created_at);
    }

    /// 同一 data_asset 可以派生多个 tn(fan-out),list_by_data_asset 只返回该 da 的。
    #[test]
    fn tn_fanout_and_list_by_data_asset() {
        let db = fresh();
        let da_a = da(&db, "t3");
        let da_b = da(&db, "t4");
        let r = db.transformation_novels();
        let a1 = r.insert(&NewTransformationNovel {
            data_asset_id: da_a, title: "a1".into(), note: String::new() }).unwrap();
        let a2 = r.insert(&NewTransformationNovel {
            data_asset_id: da_a, title: "a2".into(), note: String::new() }).unwrap();
        let b1 = r.insert(&NewTransformationNovel {
            data_asset_id: da_b, title: "b1".into(), note: String::new() }).unwrap();

        let for_a: Vec<i64> = r.list_by_data_asset(da_a).unwrap().iter().map(|t| t.id).collect();
        assert_eq!(for_a, vec![a2, a1], "同一 da 可派生多个 tn,按 id DESC");
        let for_b: Vec<i64> = r.list_by_data_asset(da_b).unwrap().iter().map(|t| t.id).collect();
        assert_eq!(for_b, vec![b1], "不应串到别的 da");

        assert_eq!(r.list().unwrap().len(), 3, "全局 list 返回全部");
    }

    /// 删 tn 不影响它的 data_asset(工程模板与源资产是独立生命周期)。
    #[test]
    fn tn_delete_leaves_data_asset_intact() {
        let db = fresh();
        let da_id = da(&db, "t5");
        let id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "t".into(), note: String::new() }).unwrap();
        db.transformation_novels().delete(id).unwrap();
        assert!(db.transformation_novels().get(id).unwrap().is_none());
        assert!(db.data_assets().get(da_id).unwrap().is_some(), "删 tn 不该动 da");
    }

    /// 删 data_asset 级联清掉挂在它上面的 tn(0006 的 ON DELETE CASCADE)。
    #[test]
    fn deleting_data_asset_cascades_to_tn() {
        let db = fresh();
        let da_id = da(&db, "t6");
        let id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "t".into(), note: String::new() }).unwrap();
        db.data_assets().delete(da_id).unwrap();
        assert!(db.transformation_novels().get(id).unwrap().is_none(), "删 da 应级联删 tn");
    }
}
