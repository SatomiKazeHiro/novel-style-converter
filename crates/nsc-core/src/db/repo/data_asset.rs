use std::sync::MutexGuard;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};

use crate::error::Result;
use crate::models::{DataAsset, DataAssetKind, NewDataAsset};

pub struct DataAssetWithUpload {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub filename: String,
    pub byte_size: i64,
    pub word_count: i64,
    pub tn_count: i64,
    pub kind: DataAssetKind,
    pub source_workflow_id: Option<i64>,
    pub source_data_asset_id: Option<i64>,
    pub promoted_count: i64,
}

pub struct DataAssetRepo<'a> { pub(crate) conn: MutexGuard<'a, rusqlite::Connection> }

fn from_row(row: &Row) -> rusqlite::Result<DataAsset> {
    let parsed_at_s: String = row.get(3)?;
    let parsed_at = DateTime::parse_from_rfc3339(&parsed_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
    let kind_s: String = row.get(5)?;
    let kind = crate::models::DataAssetKind::parse(&kind_s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(KindParseError(kind_s.clone())))
    })?;

#[derive(Debug)]
struct KindParseError(String);
impl std::fmt::Display for KindParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown kind: {}", self.0)
    }
}
impl std::error::Error for KindParseError {}
    let source_workflow_id: Option<i64> = row.get(6)?;
    let source_data_asset_id: Option<i64> = row.get(7)?;
    let note: String = row.get(8)?;
    Ok(DataAsset {
        id: row.get(0)?,
        upload_id: row.get(1)?,
        title: row.get(2)?,
        parsed_at,
        source_filename: row.get(4)?,
        kind,
        source_workflow_id,
        source_data_asset_id,
        note,
    })
}

impl<'a> DataAssetRepo<'a> {
    pub fn insert(&self, d: &NewDataAsset) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO data_assets (upload_id, title, parsed_at, source_filename) VALUES (?1, ?2, ?3, ?4)",
            params![d.upload_id, d.title, now, d.source_filename],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note              FROM data_assets WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note              FROM data_assets ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn find_by_upload(&self, upload_id: i64) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note              FROM data_assets WHERE upload_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![upload_id], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_with_upload(&self) -> Result<Vec<DataAssetWithUpload>> {
        let mut stmt = self.conn.prepare(
            "SELECT da.id, da.upload_id, da.title, da.parsed_at,                     COALESCE(u.filename, da.source_filename) AS filename,                     COALESCE(u.byte_size, 0) AS byte_size,                     COALESCE((SELECT SUM(c.word_count) FROM chapters c WHERE c.data_asset_id = da.id), 0) AS word_count,                     COALESCE(tn.cnt, 0) AS tn_count,                     da.kind, da.source_workflow_id, da.source_data_asset_id,                     COALESCE(da_derived.cnt, 0) AS promoted_count              FROM data_assets da              LEFT JOIN uploads u ON u.id = da.upload_id              LEFT JOIN (SELECT data_asset_id, COUNT(*) AS cnt                         FROM transformation_novels GROUP BY data_asset_id) tn                    ON tn.data_asset_id = da.id              LEFT JOIN (SELECT source_data_asset_id, COUNT(*) AS cnt                         FROM data_assets WHERE source_data_asset_id IS NOT NULL                         GROUP BY source_data_asset_id) da_derived                    ON da_derived.source_data_asset_id = da.id              GROUP BY da.id              ORDER BY da.id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let parsed_at_s: String = row.get(3)?;
            let parsed_at = DateTime::parse_from_rfc3339(&parsed_at_s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let kind_s: String = row.get(8)?;
            let kind = DataAssetKind::parse(&kind_s).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(KindErr(kind_s.clone())))
            })?;
            Ok(DataAssetWithUpload {
                id: row.get(0)?,
                upload_id: row.get(1)?,
                title: row.get(2)?,
                parsed_at,
                filename: row.get(4)?,
                byte_size: row.get(5)?,
                word_count: row.get(6)?,
                tn_count: row.get(7)?,
                kind,
                source_workflow_id: row.get(9)?,
                source_data_asset_id: row.get(10)?,
                promoted_count: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let exists: bool = self.conn.query_row(
            "SELECT 1 FROM data_assets WHERE id = ?1",
            params![id],
            |_| Ok(true),
        ).optional()?.unwrap_or(false);
        if !exists {
            return Err(crate::error::Error::NotFound(format!("data_asset {} 不存在", id)));
        }
        self.conn.execute("DELETE FROM data_assets WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[derive(Debug)]
struct KindErr(String);
impl std::fmt::Display for KindErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown data_asset kind: {}", self.0)
    }
}
impl std::error::Error for KindErr {}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use std::sync::Arc;
    use crate::models::{
        BatchStatus, DataAssetKind, NewBatch, NewChapter, NewDataAsset,
        NewTransformationChapter, NewTransformationNovel, NewUpload,
        OnFailurePolicy, PromptKind,
    };

    fn fresh_db() -> Arc<Db> {
        let dir = tempfile::tempdir().unwrap();
        Db::open(&dir.path().join("test.db")).unwrap()
    }

    /// 最小种子:1 upload + 1 source da + 1 chapter。
    fn seed_source(db: &Db) -> i64 {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "x".into(),
            filename: "f.txt".into(),
            byte_size: 100,
            file_path: "/tmp/f.txt".into(),
            original_text: "原文章内容".into(),
            word_count: 4,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id,
            title: "源".into(),
            source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            idx: 1,
            title: "c1".into(),
            body: "原文章1".into(),
            word_count: 5,
            ..Default::default()
        }).unwrap();
        da_id
    }

    /// 构造一个可转正的 workflow + stopped。
    fn build_promotable_workflow(db: &Db, da_id: i64) -> i64 {
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id,
            title: "tn".into(),
            note: "".into(),
        }).unwrap();
        let batch_id = db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id,
            label: Some("w1".into()),
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id: 1, model_config_id: 1, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap();
        let chapter_id = db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            idx: 2,
            title: "c2".into(),
            body: "原文章2".into(),
            word_count: 5,
            ..Default::default()
        }).unwrap();
        let tc_id = db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id,
            mode: PromptKind::Compress,
            prompt_id: 1,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            batch_id: Some(batch_id),
            style_ref_chapter_id: None,
        }).unwrap();
        db.workflow_results().create_for_batch_with_slots(batch_id, &[chapter_id]).unwrap();
        db.workflow_results().write_content_by_chapter(batch_id, chapter_id, "转换后".into()).unwrap();
        db.transformation_chapters().mark_done(tc_id, "转换后".into(), Some(5), Some(5)).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();
        batch_id
    }

    #[test]
    fn list_with_upload_includes_kind_and_source_fields() {
        let db = fresh_db();
        let da_id = seed_source(&db);

        let rows = db.data_assets().list_with_upload().unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.id, da_id);
        assert_eq!(row.kind, DataAssetKind::Source);
        assert_eq!(row.source_workflow_id, None);
        assert_eq!(row.source_data_asset_id, None);
        assert_eq!(row.promoted_count, 0);
        assert_eq!(row.tn_count, 0);
        assert_eq!(row.word_count, 5);
        assert_eq!(row.byte_size, 100);
    }

    #[test]
    fn list_with_upload_counts_promoted() {
        let db = fresh_db();
        let da_id = seed_source(&db);
        let batch_id = build_promotable_workflow(&db, da_id);

        // 第一次转正
        db.promotion().create_promoted_from_workflow(batch_id, "p1".into()).unwrap();
        // 第二次转正(允许重复)
        db.promotion().create_promoted_from_workflow(batch_id, "p2".into()).unwrap();

        let rows = db.data_assets().list_with_upload().unwrap();
        // 1 source + 2 promoted = 3 rows
        assert_eq!(rows.len(), 3);

        let source_row = rows.iter().find(|r| r.id == da_id).unwrap();
        assert_eq!(source_row.kind, DataAssetKind::Source);
        assert_eq!(source_row.promoted_count, 2);

        let promoted_rows: Vec<_> = rows.iter().filter(|r| r.kind == DataAssetKind::Promoted).collect();
        assert_eq!(promoted_rows.len(), 2);
        for p in promoted_rows {
            assert_eq!(p.source_data_asset_id, Some(da_id));
            assert_eq!(p.source_workflow_id, Some(batch_id));
            assert_eq!(p.promoted_count, 0);
        }
    }

    // ── insert / get / list / find_by_upload / delete ───────────────────────
    //
    // 上面两个测试走的是 promotion + list_with_upload 这条线;基础 CRUD 此前没测。

    fn one_upload(db: &Db, sha: &str) -> i64 {
        db.uploads().insert(&NewUpload {
            sha256: sha.into(), filename: "f.txt".into(), byte_size: 100,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 2,
        }).unwrap()
    }

    /// insert → get 往返:kind 默认 'source'、note 默认空串、source_* 为 None。
    #[test]
    fn insert_defaults_kind_to_source_and_note_to_empty() {
        let db = fresh_db();
        let upload_id = one_upload(&db, "d1");
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "我的资产".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();

        let got = db.data_assets().get(da_id).unwrap().unwrap();
        assert_eq!(got.upload_id, upload_id);
        assert_eq!(got.title, "我的资产");
        assert_eq!(got.source_filename, "f.txt");
        assert_eq!(got.kind, DataAssetKind::Source, "新建资产默认是 source");
        assert_eq!(got.note, "", "note 默认为空串");
        assert!(got.source_workflow_id.is_none());
        assert!(got.source_data_asset_id.is_none());
    }

    /// **同一 upload 可以有多个 data_asset**(upload_id 上无 UNIQUE —— 0015 起的
    /// 软引用设计)。这是"重新解析同一文件"能留下多条历史的前提。
    #[test]
    fn multiple_data_assets_can_share_one_upload() {
        let db = fresh_db();
        let upload_id = one_upload(&db, "d2");
        let a = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "第一次解析".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        let b = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "第二次解析".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        assert_ne!(a, b);

        let found: Vec<i64> = db.data_assets().find_by_upload(upload_id).unwrap()
            .iter().map(|d| d.id).collect();
        assert_eq!(found, vec![b, a], "find_by_upload 应返回该 upload 的全部资产(id DESC)");
    }

    /// find_by_upload 对无资产的 upload 返回空;list 全局按 id DESC。
    #[test]
    fn find_by_upload_scopes_and_list_orders_desc() {
        let db = fresh_db();
        let u1 = one_upload(&db, "d3");
        let u2 = one_upload(&db, "d4");
        assert!(db.data_assets().find_by_upload(u1).unwrap().is_empty(), "还没解析 → 空");

        let a = db.data_assets().insert(&NewDataAsset {
            upload_id: u1, title: "a".into(), source_filename: "f".into(), ..Default::default()
        }).unwrap();
        let b = db.data_assets().insert(&NewDataAsset {
            upload_id: u2, title: "b".into(), source_filename: "f".into(), ..Default::default()
        }).unwrap();

        assert_eq!(db.data_assets().find_by_upload(u2).unwrap().len(), 1, "只返回自己的");
        let ids: Vec<i64> = db.data_assets().list().unwrap().iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![b, a], "list 按 id DESC(最新在前)");
    }

    /// get 对不存在的 id 返回 None。
    #[test]
    fn get_missing_returns_none() {
        let db = fresh_db();
        assert!(db.data_assets().get(99999).unwrap().is_none());
    }

    /// 删 data_asset 级联清掉它的 chapters(0005 挂的 ON DELETE CASCADE)——
    /// 顺序是 upload → da → chapters,所以删 da 必须把章节一起带走。
    #[test]
    fn delete_cascades_to_chapters() {
        let db = fresh_db();
        let da_id = seed_source(&db);
        assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 1);

        db.data_assets().delete(da_id).unwrap();

        assert!(db.data_assets().get(da_id).unwrap().is_none(), "da 应被删");
        assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 0,
            "删 da 应级联删 chapters");
    }

    /// 库里出现未知 kind 字符串时**报错而不是静默降级** ——
    /// 静默当成 source/promoted 会让"派生资产"被误认为源资产,影响转正与统计。
    #[test]
    fn unknown_kind_in_db_is_reported_not_defaulted() {
        let db = fresh_db();
        let da_id = seed_source(&db);
        db.lock().execute("UPDATE data_assets SET kind = 'not_a_kind' WHERE id = ?1",
            rusqlite::params![da_id]).unwrap();

        let err = db.data_assets().get(da_id).unwrap_err();
        assert!(
            err.to_string().contains("unknown") || err.to_string().contains("kind"),
            "未知 kind 应报错并说明原因,实际: {err}"
        );
    }
}
