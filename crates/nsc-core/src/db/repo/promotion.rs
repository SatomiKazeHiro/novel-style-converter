use std::sync::MutexGuard;
use chrono::Utc;
use rusqlite::params;
use crate::error::{Error, Result};
use crate::models::{DataAsset, DataAssetKind};
/// promotion 一次 SELECT 读出的行:(tc_id, chapter_id, tc_status, idx, title, body, wrc_content)。
/// word_count 不再读 —— promote 落库的字数从实际 body 重算,跟落库的 body 永远一致。
type PromotionRow = (i64, i64, String, i32, String, String, Option<String>);

pub struct PromotionRepo<'a> { pub(crate) conn: MutexGuard<'a, rusqlite::Connection> }

impl<'a> PromotionRepo<'a> {
    /// 从一个 Stopped workflow 派生新的 promoted data_asset + N 个 chapter。
    /// 单事务:校验 batch.status=stopped → 读所有 tc + chapter + wrc → 写 da + 写 chapters。
    pub fn create_promoted_from_workflow(
        &self,
        batch_id: i64,
        title: String,
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();

        // 1. batch 存在 + stopped
        let batch_status: String = tx.query_row(
            "SELECT status FROM batches WHERE id=?1",
            params![batch_id], |r| r.get(0),
        ).map_err(|_| Error::NotFound(format!("batch {batch_id} 不存在")))?;
        if batch_status != "stopped" {
            return Err(Error::Validation(format!(
                "workflow 必须 Stopped 才能转正(当前 {batch_status})"
            )));
        }

        // 2. 读 source_data_asset_id 和 upload_id
        let source_da_id: i64 = tx.query_row(
            "SELECT tn.data_asset_id FROM transformation_novels tn
             JOIN batches b ON b.transformation_novel_id = tn.id WHERE b.id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        let upload_id: i64 = tx.query_row(
            "SELECT upload_id FROM data_assets WHERE id=?1",
            params![source_da_id], |r| r.get(0),
        )?;

        // 3. 读所有 tc + chapter + wrc
        let rows: Vec<PromotionRow> = {
            let mut stmt = tx.prepare(
                "SELECT tc.id, tc.chapter_id, tc.status,
                        c.idx, c.title, c.body,
                        wrc.content
                 FROM transformation_chapters tc
                 JOIN chapters c ON c.id = tc.chapter_id
                 LEFT JOIN workflow_results wr ON wr.batch_id = tc.batch_id
                 LEFT JOIN workflow_result_chapters wrc
                     ON wrc.workflow_result_id = wr.id AND wrc.chapter_id = tc.chapter_id
                 WHERE tc.batch_id = ?1 ORDER BY c.idx ASC",
            )?;
            let collected = stmt.query_map(params![batch_id], |r| Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?,
                r.get(4)?, r.get(5)?, r.get(6)?,
            )))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(stmt);
            collected
        };
        if rows.is_empty() {
            return Err(Error::Validation("workflow 无章节".into()));
        }

        // 4. 前置校验
        for (tc_id, _cid, tc_status, _idx, _t, _b, wrc_content) in &rows {
            match tc_status.as_str() {
                "done" => {
                    if wrc_content.is_none() {
                        return Err(Error::Validation(format!(
                            "数据损坏:tc {tc_id} done 但 wrc.content IS NULL"
                        )));
                    }
                }
                "failed" | "skipped" => {}
                other => {
                    return Err(Error::Validation(format!(
                        "workflow 含未完成任务(tc {tc_id} status={other})"
                    )));
                }
            }
        }

        // 5. INSERT promoted da
        tx.execute(
            "INSERT INTO data_assets
                (upload_id, title, parsed_at, kind, source_workflow_id, source_data_asset_id)
             VALUES (?1, ?2, ?3, ?6, ?4, ?5)",
            params![upload_id, title, now, batch_id, source_da_id, DataAssetKind::Promoted.as_str()],
        )?;
        let new_da_id = tx.last_insert_rowid();

        // 6. INSERT N 个 chapter
        for (_tc_id, chapter_id, tc_status, idx, chapter_title, chapter_body, wrc_content) in &rows {
            let (body, source_kind) = if tc_status == "done" {
                (wrc_content.as_ref().unwrap().clone(), "transformed")
            } else {
                (chapter_body.clone(), "original")
            };
            let body_word_count = crate::text::word_count(&body) as i32;
            tx.execute(
                "INSERT INTO chapters
                    (data_asset_id, idx, title, body, word_count, source_kind, source_chapter_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![new_da_id, idx, chapter_title, body, body_word_count, source_kind, chapter_id],
            )?;
        }

        tx.commit()?;
        Ok(new_da_id)
    }

    pub fn count_by_workflow(&self, batch_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM data_assets WHERE source_workflow_id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn list_by_workflow(&self, batch_id: i64) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note
             FROM data_assets WHERE source_workflow_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![batch_id], |row| {
            let parsed_at_s: String = row.get(3)?;
            let parsed_at = chrono::DateTime::parse_from_rfc3339(&parsed_at_s)
                .map(|d| d.with_timezone(&chrono::Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let kind_s: String = row.get(5)?;
            let kind = DataAssetKind::parse(&kind_s).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(KindErr(kind_s.clone())))
            })?;
            Ok(DataAsset {
                id: row.get(0)?, upload_id: row.get(1)?, title: row.get(2)?, parsed_at,
                source_filename: row.get(4)?, kind,
                source_workflow_id: row.get(6)?, source_data_asset_id: row.get(7)?,
                note: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_by_upload(&self, upload_id: i64) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note
             FROM data_assets WHERE upload_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![upload_id], |row| {
            let parsed_at_s: String = row.get(3)?;
            let parsed_at = chrono::DateTime::parse_from_rfc3339(&parsed_at_s)
                .map(|d| d.with_timezone(&chrono::Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let kind_s: String = row.get(5)?;
            let kind = DataAssetKind::parse(&kind_s).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(KindErr(kind_s.clone())))
            })?;
            Ok(DataAsset {
                id: row.get(0)?, upload_id: row.get(1)?, title: row.get(2)?, parsed_at,
                source_filename: row.get(4)?, kind,
                source_workflow_id: row.get(6)?, source_data_asset_id: row.get(7)?,
                note: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
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

    fn seed_chain(db: &Db) -> (i64, i64, i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "x".into(),
            filename: "f.txt".into(),
            byte_size: 10,
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
        let mut tc_ids = vec![];
        for i in 0..3 {
            let chapter_id = db.chapters().insert(&NewChapter {
                data_asset_id: da_id,
                idx: i + 1,
                title: format!("c{}", i + 1),
                body: format!("原文章{}很长很长内容", i + 1),
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
            tc_ids.push(tc_id);
        }
        (upload_id, da_id, batch_id, tc_ids)
    }

    /// 单章节版本:与 seed_chain 类似但只创建 1 个 chapter + 1 个 tc。
    /// 用于不依赖多章节混合语义的测试(如允许重复转正、删除源 da 等)。
    fn seed_single_chapter(db: &Db) -> (i64, i64, i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "x".into(),
            filename: "f.txt".into(),
            byte_size: 10,
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
            idx: 1,
            title: "c1".into(),
            body: "原文章1很长很长内容".into(),
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
        (upload_id, da_id, batch_id, vec![tc_id])
    }

    #[test]
    fn promote_happy_path_done_and_failed_and_skipped() {
        let db = fresh_db();
        let (_up, da_id, batch_id, tc_ids) = seed_chain(&db);

        // tc[0]=done(写 wrc.content),tc[1]=failed,tc[2]=skipped
        db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2, 3]).unwrap();
        db.workflow_results().write_content_by_chapter(batch_id, 1, "转换后文本A".into()).unwrap();
        db.transformation_chapters().mark_done(tc_ids[0], "转换后文本A".into(), 10, 20).unwrap();
        db.transformation_chapters().mark_failed(tc_ids[1], "测试失败".into()).unwrap();
        db.transformation_chapters().mark_skipped(tc_ids[2], "用户跳过".into()).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();

        let new_da_id = db.promotion().create_promoted_from_workflow(batch_id, "派生测试".into()).unwrap();

        let new_da = db.data_assets().get(new_da_id).unwrap().unwrap();
        assert_eq!(new_da.kind, DataAssetKind::Promoted);
        assert_eq!(new_da.source_workflow_id, Some(batch_id));
        assert_eq!(new_da.source_data_asset_id, Some(da_id));
        assert_eq!(new_da.title, "派生测试");

        let chapters = db.chapters().list_by_data_asset(new_da_id).unwrap();
        assert_eq!(chapters.len(), 3);
        // tc[0] done → wrc.content
        assert_eq!(chapters[0].body, "转换后文本A");
        assert_eq!(chapters[0].source_kind, "transformed");
        // tc[1] failed → 原 chapter.body
        assert!(chapters[1].body.starts_with("原文章2"));
        assert_eq!(chapters[1].source_kind, "original");
        // tc[2] skipped → 原 chapter.body
        assert!(chapters[2].body.starts_with("原文章3"));
        assert_eq!(chapters[2].source_kind, "original");
    }

    #[test]
    fn promote_rejects_running_batch() {
        let db = fresh_db();
        let (_u, _d, batch_id, _) = seed_chain(&db);
        // 默认 batch.status 可能是 pending 或 running,只需非 stopped 即可报错
        let result = db.promotion().create_promoted_from_workflow(batch_id, "t".into());
        assert!(result.is_err(), "running/pending batch should be rejected");
        let err = format!("{:?}", result.err().unwrap());
        assert!(err.contains("Stopped") || err.contains("Validation"));
    }

    #[test]
    fn promote_rejects_done_tc_with_null_content() {
        let db = fresh_db();
        let (_u, _d, batch_id, tc_ids) = seed_chain(&db);
        // 直接 mark_done 但不写 wrc.content(模拟数据损坏)
        // mark_done 内部会写 tc.result_content,但 wrc.content 仍 NULL
        db.transformation_chapters().mark_done(tc_ids[0], "".into(), 0, 0).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();

        let result = db.promotion().create_promoted_from_workflow(batch_id, "t".into());
        assert!(result.is_err());
        let err = format!("{:?}", result.err().unwrap());
        assert!(err.contains("数据损坏") || err.contains("Validation"));
    }

    #[test]
    fn promote_allows_repeat_appends_new_da() {
        let db = fresh_db();
        let (_u, _d, batch_id, tc_ids) = seed_single_chapter(&db);
        db.workflow_results().create_for_batch_with_slots(batch_id, &[1]).unwrap();
        db.workflow_results().write_content_by_chapter(batch_id, 1, "A".into()).unwrap();
        db.transformation_chapters().mark_done(tc_ids[0], "A".into(), 1, 1).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();

        let id1 = db.promotion().create_promoted_from_workflow(batch_id, "v1".into()).unwrap();
        let id2 = db.promotion().create_promoted_from_workflow(batch_id, "v2".into()).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(db.promotion().count_by_workflow(batch_id).unwrap(), 2);
    }

    #[test]
    fn delete_source_da_sets_null_on_promoted() {
        let db = fresh_db();
        let (_u, da_id, batch_id, tc_ids) = seed_single_chapter(&db);
        db.workflow_results().create_for_batch_with_slots(batch_id, &[1]).unwrap();
        db.workflow_results().write_content_by_chapter(batch_id, 1, "A".into()).unwrap();
        db.transformation_chapters().mark_done(tc_ids[0], "A".into(), 1, 1).unwrap();
        db.batches().set_status(batch_id, BatchStatus::Stopped).unwrap();

        let promoted_id = db.promotion().create_promoted_from_workflow(batch_id, "p".into()).unwrap();
        db.data_assets().delete(da_id).unwrap();

        let promoted_after = db.data_assets().get(promoted_id).unwrap().unwrap();
        assert!(promoted_after.source_data_asset_id.is_none());
        assert_eq!(promoted_after.kind, DataAssetKind::Promoted);
    }
}
