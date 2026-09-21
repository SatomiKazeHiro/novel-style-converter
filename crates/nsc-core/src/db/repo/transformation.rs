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

    /// 标记完成。`tokens_*` 为 `Option` —— provider 不返回 usage 时落 NULL
    /// (见 `ai::ChatResponse`),而不是伪造一个数字。
    pub fn mark_done(&self, id: i64, result_content: String, tokens_in: Option<i32>, tokens_out: Option<i32>) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::batch::{NewBatch, OnFailurePolicy};
    use crate::models::{
        NewChapter, NewDataAsset, NewModelConfig, NewTransformationNovel, NewUpload, Prompt,
    };

    // 写这些测试时踩到的两个坑,后来者注意:
    //
    // 1. **repo 的 MutexGuard 不可重入**。`db.xxx()` 返回的 guard 如果作为临时
    //    receiver(`db.chapters().get(id)`),它会活到**整个语句结束**;同一语句里
    //    再取第二把锁就是自死锁(测试会永久挂住,不是失败)。要跨调用就先把 repo
    //    绑到变量分语句用,或在专用作用域里读完再写。
    // 2. **`uploads.sha256` 有 UNIQUE 约束**,fixture 里重复用同一个值会直接 panic。
    // 3. **`UNIQUE(batch_id, chapter_id)`**:同一 batch 内一个 chapter 只能有一条 tc,
    //    所以"同章历史"必须来自不同 batch。

    /// 建一套满足 FK 的最小环境。
    /// 返回 (batch_id, chapter_ids, prompt_id, model_id) —— 后两个是真主键,不能猜。
    fn seed(db: &Db) -> (i64, Vec<i64>, i64, i64) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "t1".into(), filename: "f.txt".into(), byte_size: 1,
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
        for i in 1..=3 {
            let cid = db.chapters().insert(&NewChapter {
                data_asset_id: da_id, idx: i, title: format!("c{i}"),
                body: format!("正文{i}"), word_count: 3, ..Default::default()
            }).unwrap();
            cids.push(cid);
            db.transformation_chapters().insert(&NewTransformationChapter {
                transformation_novel_id: tn_id, chapter_id: cid,
                mode: PromptKind::Compress, prompt_id, model_config_id: model_id,
                ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
                batch_id: Some(batch_id), style_ref_chapter_id: None,
            }).unwrap();
        }
        (batch_id, cids, prompt_id, model_id)
    }

    /// 为已有 batch 再插一条 tc(用真实 FK id)。
    fn insert_tc(db: &Db, batch_id: i64, chapter_id: i64, prompt_id: i64, model_id: i64) -> i64 {
        let tn_id = {
            let b = db.batches().get(batch_id).unwrap().unwrap();
            b.transformation_novel_id
        };
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id, chapter_id,
            mode: PromptKind::Style, prompt_id, model_config_id: model_id,
            ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
            batch_id: Some(batch_id), style_ref_chapter_id: None,
        }).unwrap()
    }

    /// 另建一个 batch(复制已有 batch 的配置),用于验证过滤不串号 / 跨 batch 历史。
    fn insert_batch(db: &Db, like_batch: i64) -> i64 {
        let (tn_id, prompt_id, model_id) = {
            let b = db.batches().get(like_batch).unwrap().unwrap();
            (b.transformation_novel_id, b.prompt_id, b.model_config_id)
        };
        db.batches().insert(&NewBatch {
            transformation_novel_id: tn_id,
            label: None, on_failure_policy: OnFailurePolicy::SkipFailed,
            prompt_id, model_config_id: model_id,
            mode: "compress".into(),
            ctx_prev_original: 0, ctx_prev_transformed: 0,
            ctx_next_original: 0, ctx_next_transformed: 0,
        }).unwrap()
    }

    fn fresh() -> (Db, i64, Vec<i64>, i64, i64) {
        let db = Db::open_in_memory().unwrap();
        let (batch_id, cids, prompt_id, model_id) = seed(&db);
        (db, batch_id, cids, prompt_id, model_id)
    }

    /// insert 的初始契约:status=pending,其余终结字段全空。
    #[test]
    fn insert_starts_pending_with_no_terminal_fields() {
        let (db, batch_id, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Pending);
        assert!(tc.started_at.is_none());
        assert!(tc.completed_at.is_none());
        assert!(tc.error.is_none());
        assert!(tc.result_content.is_none());
        assert!(tc.tokens_in.is_none());
        assert!(tc.tokens_out.is_none());
        assert_eq!(tc.batch_id, Some(batch_id));
    }

    /// mark_running 只写 started_at,不碰 completed_at。
    #[test]
    fn mark_running_sets_started_at_only() {
        let (db, _b, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        t.mark_running(tid).unwrap();
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Running);
        assert!(tc.started_at.is_some(), "mark_running 应写 started_at");
        assert!(tc.completed_at.is_none(), "running 不是终结态,不该有 completed_at");
    }

    /// mark_done:传空串时 `NULLIF(?2,'')` 让 result_content 保持 NULL ——
    /// worker 收口后正文写在 workflow_result_chapters,不再落这一列。
    #[test]
    fn mark_done_with_empty_content_keeps_result_content_null() {
        let (db, _b, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        t.mark_done(tid, String::new(), Some(11), Some(22)).unwrap();
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Done);
        assert!(tc.result_content.is_none(), "空串应经 NULLIF 落成 NULL");
        assert_eq!(tc.tokens_in, Some(11));
        assert_eq!(tc.tokens_out, Some(22));
        assert!(tc.completed_at.is_some());
    }

    /// tokens 为 None(provider 未返回 usage)→ 落 NULL,不伪造 0。
    #[test]
    fn mark_done_stores_null_tokens_when_usage_missing() {
        let (db, _b, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        t.mark_done(tid, "正文".into(), None, None).unwrap();
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.result_content.as_deref(), Some("正文"));
        assert!(tc.tokens_in.is_none(), "缺 usage 应是 NULL,不是 0");
        assert!(tc.tokens_out.is_none());
    }

    /// mark_skipped:保留 error(用户能看到跳过原因),但清空正文与 tokens。
    #[test]
    fn mark_skipped_keeps_error_and_clears_content_and_tokens() {
        let (db, _b, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        t.mark_done(tid, "旧正文".into(), Some(1), Some(2)).unwrap();
        t.mark_skipped(tid, "用户跳过".into()).unwrap();
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Skipped);
        assert_eq!(tc.error.as_deref(), Some("用户跳过"));
        assert!(tc.result_content.is_none(), "skipped 应清空正文");
        assert!(tc.tokens_in.is_none(), "skipped 应清空 tokens");
        assert!(tc.tokens_out.is_none());
    }

    /// reset_to_pending:重试路径的完整复位 —— 终结字段全清。
    #[test]
    fn reset_to_pending_clears_all_terminal_fields() {
        let (db, _b, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        t.mark_running(tid).unwrap();
        t.mark_done(tid, "正文".into(), Some(5), Some(6)).unwrap();
        t.reset_to_pending(tid).unwrap();
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Pending);
        assert!(tc.result_content.is_none());
        assert!(tc.tokens_in.is_none());
        assert!(tc.tokens_out.is_none());
        assert!(tc.error.is_none());
        assert!(tc.started_at.is_none(), "复位应清 started_at");
        assert!(tc.completed_at.is_none(), "复位应清 completed_at");
    }

    /// 记录当前行为:`mark_failed` **不**清空上一轮留下的 result_content/tokens。
    /// 重试路径走 reset_to_pending(会清),所以正常流程看不到脏数据;
    /// 但若将来出现"done → failed"的直接转换,这里会留下过期正文。
    #[test]
    fn mark_failed_documents_stale_content_not_cleared() {
        let (db, _b, cids, _, _) = fresh();
        let t = db.transformation_chapters();
        let tid = t.list_by_chapter(cids[0]).unwrap()[0].id;
        t.mark_done(tid, "上一轮正文".into(), Some(3), Some(4)).unwrap();
        t.mark_failed(tid, "本轮失败".into()).unwrap();
        let tc = t.get(tid).unwrap().unwrap();
        assert_eq!(tc.status, TransformStatus::Failed);
        assert_eq!(tc.error.as_deref(), Some("本轮失败"));
        assert_eq!(tc.result_content.as_deref(), Some("上一轮正文"));
        assert_eq!(tc.tokens_in, Some(3));
        assert_eq!(tc.tokens_out, Some(4));
    }

    /// list_by_batch 必须按 chapter_idx 升序 —— 调度器靠这个顺序派发章节。
    #[test]
    fn list_by_batch_orders_by_chapter_idx() {
        let (db, batch_id, _, _, _) = fresh();
        // 先取 chapter_id 序列(guard 随即释放),再查 idx。
        let chapter_ids: Vec<i64> = {
            let t = db.transformation_chapters();
            t.list_by_batch(batch_id).unwrap().iter().map(|t| t.chapter_id).collect()
        };
        assert_eq!(chapter_ids.len(), 3);
        let idxs: Vec<i32> = chapter_ids.iter()
            .map(|cid| db.chapters().get(*cid).unwrap().unwrap().idx)
            .collect();
        let mut sorted = idxs.clone();
        sorted.sort();
        assert_eq!(idxs, sorted, "list_by_batch 应按 chapter_idx 升序");
    }

    /// list_by_chapter 保留全部历史(按 id desc)。
    ///
    /// 历史来自**不同 batch** —— 同一 batch 内一个 chapter 只能有一条 tc
    /// (schema 的 `UNIQUE(batch_id, chapter_id)`,见 migration 0011/0027)。
    #[test]
    fn list_by_chapter_keeps_history_desc() {
        let (db, batch_id, cids, prompt_id, model_id) = fresh();
        let other_batch = insert_batch(&db, batch_id);
        let second = insert_tc(&db, other_batch, cids[0], prompt_id, model_id);
        let ids: Vec<i64> = db.transformation_chapters().list_by_chapter(cids[0])
            .unwrap().iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 2, "同一章节跨 batch 的两条历史都应保留");
        assert_eq!(ids[0], second, "应按 id desc,最新在前");
        assert!(ids[1] < second);
    }

    /// list_by_status / count_by_batch 的过滤正确性(不串 batch)。
    #[test]
    fn list_by_status_and_count_by_batch_filter_correctly() {
        let (db, batch_id, cids, prompt_id, model_id) = fresh();
        // 读写分阶段:读完就释放 guard,再插别的 batch 的行。
        {
            let t = db.transformation_chapters();
            let all = t.list_by_batch(batch_id).unwrap();
            assert_eq!(t.count_by_batch(batch_id).unwrap(), 3);
            t.mark_done(all[0].id, "x".into(), Some(1), Some(1)).unwrap();
            assert_eq!(t.list_by_status(TransformStatus::Pending).unwrap().len(), 2);
            assert_eq!(t.list_by_status(TransformStatus::Done).unwrap().len(), 1);
            assert_eq!(t.count_by_batch(batch_id).unwrap(), 3, "计数不随状态变化");
        }

        let other_batch = insert_batch(&db, batch_id);
        insert_tc(&db, other_batch, cids[1], prompt_id, model_id);
        let t = db.transformation_chapters();
        assert_eq!(t.count_by_batch(batch_id).unwrap(), 3, "别的 batch 不应混入");
        assert_eq!(t.count_by_batch(other_batch).unwrap(), 1);
    }

    /// 取消态(status='cancelled')的解析 —— from_row 的兜底分支,
    /// 未知字符串也落到 Cancelled(记录该宽松语义)。
    #[test]
    fn unknown_status_falls_back_to_cancelled() {
        let (db, _b, cids, _, _) = fresh();
        // 先取 id(guard 释放),再直接改库,最后重建 repo 读回 ——
        // 不要一边持有 repo guard 一边 db.lock()。
        let tid = {
            let t = db.transformation_chapters();
            t.list_by_chapter(cids[0]).unwrap()[0].id
        };
        db.lock().execute(
            "UPDATE transformation_chapters SET status='cancelled' WHERE id=?1",
            params![tid],
        ).unwrap();
        let t = db.transformation_chapters();
        assert_eq!(t.get(tid).unwrap().unwrap().status, TransformStatus::Cancelled);
    }
}