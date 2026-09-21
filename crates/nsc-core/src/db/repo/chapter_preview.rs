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
    /// `tokens_*` 为 `Option` —— provider 不返回 usage 时落 NULL(见 `ai::ChatResponse`)。
    pub fn update_done(
        &self,
        id: i64,
        preview_content: &str,
        tokens_in: Option<i32>,
        tokens_out: Option<i32>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::batch::{NewBatch, OnFailurePolicy};
    use crate::models::prompt::PromptKind;
    use crate::models::{
        NewChapter, NewDataAsset, NewModelConfig, NewTransformationNovel, NewUpload, Prompt,
    };

    /// 预览行有 FK 指向 batches / chapters,所以要建齐最小环境。
    fn seed(db: &Db) -> (i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "cv1".into(), filename: "f.txt".into(), byte_size: 1,
            file_path: "/tmp/f.txt".into(), original_text: "原文".into(), word_count: 2,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(), note: String::new(),
        }).unwrap();
        let prompt_id = db.prompts().insert(&Prompt {
            id: 0, name: "p".into(), kind: PromptKind::Compress,
            template: "{{chapter_content}}".into(), is_builtin: false, archived: 0,
        }).unwrap();
        let model_id = db.model_configs().insert(&NewModelConfig {
            name: "m".into(), base_url: "http://localhost".into(), api_key: "k".into(),
            model: "m".into(), max_tokens: None, max_context: None, temperature: None,
            disable_thinking: false, concurrency: 1,
        }).unwrap();
        let batch_id = db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id, label: None,
            on_failure_policy: OnFailurePolicy::PauseAndReview,
            prompt_id, model_config_id: model_id, mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap();
        let mut cids = Vec::new();
        for i in 1..=2 {
            cids.push(db.chapters().insert(&NewChapter {
                data_asset_id: da_id, idx: i, title: format!("c{i}"),
                body: format!("正文{i}"), word_count: 3, ..Default::default()
            }).unwrap());
        }
        (batch_id, cids)
    }

    fn fresh() -> (Db, i64, Vec<i64>) {
        let db = Db::open_in_memory().unwrap();
        let (batch_id, cids) = seed(&db);
        (db, batch_id, cids)
    }

    /// 新预览行的契约:status = generating,内容/tokens/error 全空,
    /// 且 `custom_input` 未填时存 **NULL**(与原 transform 路径 byte-equal 的前提)。
    #[test]
    fn insert_generating_starts_empty() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let id = r.insert_generating(batch_id, cids[0], None).unwrap();
        let row = r.get(id).unwrap().unwrap();
        assert_eq!(row.status, PreviewStatus::Generating);
        assert_eq!(row.batch_id, batch_id);
        assert_eq!(row.chapter_id, cids[0]);
        assert!(row.custom_input.is_none(), "未填附加指令应为 NULL");
        assert!(row.preview_content.is_none());
        assert!(row.tokens_in.is_none() && row.tokens_out.is_none());
        assert!(row.error.is_none());
    }

    /// 填写了附加指令时应原样存下(重生成预览会带它)。
    #[test]
    fn insert_generating_stores_custom_input() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let id = r.insert_generating(batch_id, cids[0], Some("更口语化")).unwrap();
        assert_eq!(r.get(id).unwrap().unwrap().custom_input.as_deref(), Some("更口语化"));
    }

    /// generating → done:写正文与 tokens,清空 error;空串经 NULLIF 落 NULL。
    #[test]
    fn update_done_writes_content_and_clears_error() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let id = r.insert_generating(batch_id, cids[0], None).unwrap();
        // 先失败一次,再成功覆盖 —— 验证 error 会被清掉
        r.update_failed(id, "上一轮失败").unwrap();
        assert_eq!(r.get(id).unwrap().unwrap().status, PreviewStatus::Failed);

        r.update_done(id, "预览正文", Some(7), Some(9)).unwrap();
        let row = r.get(id).unwrap().unwrap();
        assert_eq!(row.status, PreviewStatus::Done);
        assert_eq!(row.preview_content.as_deref(), Some("预览正文"));
        assert_eq!(row.tokens_in, Some(7));
        assert_eq!(row.tokens_out, Some(9));
        assert!(row.error.is_none(), "成功后应清空上一轮的 error");
    }

    /// 空正文经 `NULLIF(?2,'')` 落成 NULL;tokens 为 None(provider 未返回 usage)也落 NULL。
    #[test]
    fn update_done_handles_empty_content_and_missing_usage() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let id = r.insert_generating(batch_id, cids[0], None).unwrap();
        r.update_done(id, "", None, None).unwrap();
        let row = r.get(id).unwrap().unwrap();
        assert_eq!(row.status, PreviewStatus::Done);
        assert!(row.preview_content.is_none(), "空串应落 NULL");
        assert!(row.tokens_in.is_none() && row.tokens_out.is_none(), "缺 usage 应为 NULL 不是 0");
    }

    /// generating → failed:写 error,正文保持不动(可能上一轮已有内容)。
    #[test]
    fn update_failed_records_error() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let id = r.insert_generating(batch_id, cids[0], None).unwrap();
        r.update_failed(id, "provider 422").unwrap();
        let row = r.get(id).unwrap().unwrap();
        assert_eq!(row.status, PreviewStatus::Failed);
        assert_eq!(row.error.as_deref(), Some("provider 422"));
    }

    /// 同一 (batch, chapter) 可以有**多条**预览(重生成留历史),按 id DESC 新→旧。
    #[test]
    fn list_by_chapter_keeps_history_newest_first() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let a = r.insert_generating(batch_id, cids[0], None).unwrap();
        let b = r.insert_generating(batch_id, cids[0], None).unwrap();
        let c = r.insert_generating(batch_id, cids[1], None).unwrap(); // 别的章节

        let ids: Vec<i64> = r.list_by_chapter(batch_id, cids[0]).unwrap()
            .iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![b, a], "应按 id DESC,且只含该章节");
        assert_eq!(r.list_by_chapter(batch_id, cids[1]).unwrap().len(), 1);
        assert!(r.list_by_chapter(batch_id, 99999).unwrap().is_empty());
        assert!(ids.contains(&a) && !ids.contains(&c));
    }

    /// `delete_by_chapter` 清掉该章节的全部预览并返回行数(提交后清理路径),
    /// 且**不影响**其它章节的预览。
    #[test]
    fn delete_by_chapter_removes_only_that_chapter() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        r.insert_generating(batch_id, cids[0], None).unwrap();
        r.insert_generating(batch_id, cids[0], None).unwrap();
        let keep = r.insert_generating(batch_id, cids[1], None).unwrap();

        let n = r.delete_by_chapter(batch_id, cids[0]).unwrap();
        assert_eq!(n, 2, "应返回删除行数");
        assert!(r.list_by_chapter(batch_id, cids[0]).unwrap().is_empty());
        assert!(r.get(keep).unwrap().is_some(), "别的章节的预览不该被删");
    }

    /// 单条删除;不存在的 id 返回 None(不报错)。
    #[test]
    fn delete_one_and_missing_id_is_safe() {
        let (db, batch_id, cids) = fresh();
        let r = db.chapter_previews();
        let id = r.insert_generating(batch_id, cids[0], None).unwrap();
        assert!(r.get(99999).unwrap().is_none());
        r.delete(id).unwrap();
        assert!(r.get(id).unwrap().is_none());
        r.delete(99999).unwrap(); // 静默无行可删
    }

    /// 库里出现未知 status 字符串 → 报错而不是静默降级(静默会把失败的预览当成功)。
    /// 注意:`db.lock()` 与 repo guard 不能同时持有(锁不可重入),所以先裸 SQL 污染、后读。
    #[test]
    fn unknown_status_is_reported_not_defaulted() {
        let (db, batch_id, cids) = fresh();
        let id = db.chapter_previews().insert_generating(batch_id, cids[0], None).unwrap();
        {
            let conn = db.lock();
            conn.execute(
                "UPDATE chapter_previews SET status='weird' WHERE id=?1",
                params![id],
            ).unwrap();
        }
        assert!(db.chapter_previews().get(id).is_err(), "未知 status 应报错");
    }
}
