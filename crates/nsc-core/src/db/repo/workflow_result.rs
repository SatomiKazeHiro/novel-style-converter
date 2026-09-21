use std::sync::MutexGuard;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::Result;
use crate::models::workflow_result::{WorkflowResult, WorkflowResultChapter};

pub struct WorkflowResultRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> WorkflowResultRepo<'a> {
    /// 在同一事务内创建结果集 + N 个空结果槽;任一失败回滚。
    /// `INSERT OR IGNORE` 保证对同一 batch 重复调用也是幂等的——Task 3 启动时
    /// 偶发重试路径会撞这里,这里靠 schema 上的 UNIQUE(batch_id) 收口。
    pub fn create_for_batch_with_slots(
        &self,
        batch_id: i64,
        chapter_ids: &[i64],
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT OR IGNORE INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
            params![batch_id, now],
        )?;
        let result_id: i64 = tx.query_row(
            "SELECT id FROM workflow_results WHERE batch_id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        for cid in chapter_ids {
            tx.execute(
                "INSERT OR IGNORE INTO workflow_result_chapters \
                 (workflow_result_id, chapter_id, content, created_at, updated_at) \
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                params![result_id, cid, now],
            )?;
        }
        tx.commit()?;
        Ok(result_id)
    }

    pub fn get_by_batch(&self, batch_id: i64) -> Result<Option<WorkflowResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, batch_id, created_at FROM workflow_results WHERE batch_id = ?1",
        )?;
        let mut rows = stmt.query(params![batch_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_result(row)?))
        } else { Ok(None) }
    }

    pub fn list_chapters(&self, result_id: i64) -> Result<Vec<WorkflowResultChapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, workflow_result_id, chapter_id, content, created_at, updated_at \
             FROM workflow_result_chapters WHERE workflow_result_id = ?1 ORDER BY chapter_id ASC",
        )?;
        let rows = stmt.query_map(params![result_id], row_to_chapter)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 取 (batch_id, chapter_id) 对应已写入的 content —— queue.rs 拿 prev_transformed 用。
    ///
    /// §3.3:worker 路径收口后**真内容在 `workflow_result_chapters.content`**;
    /// `transformation_chapters.result_content` 只作为预览/兼容路径的落点
    /// (mark_done 仍会写它,worker 传空串时经 NULLIF 落成 NULL)。本方法就是
    /// 给 caller 拿那个权威内容的入口。
    pub fn get_content_by_batch_and_chapter(
        &self,
        batch_id: i64,
        chapter_id: i64,
    ) -> Result<Option<String>> {
        let content: Option<Option<String>> = self.conn
            .query_row(
                "SELECT content FROM workflow_result_chapters \
                 WHERE chapter_id = ?2 \
                   AND workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id = ?1)",
                params![batch_id, chapter_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(content.flatten())
    }

        /// 按 (batch_id, chapter_id) 写入内容;槽不存在或结果集缺失时静默 noop,
    /// 让 worker 回调和 retry 路径无需先查 slot id。
    pub fn write_content_by_chapter(
        &self,
        batch_id: i64,
        chapter_id: i64,
        content: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE workflow_result_chapters SET content = ?3, updated_at = ?4 \
             WHERE chapter_id = ?2 \
               AND workflow_result_id = (SELECT id FROM workflow_results WHERE batch_id = ?1)",
            params![batch_id, chapter_id, content, now],
        )?;
        Ok(())
    }
}

fn row_to_result(row: &Row<'_>) -> rusqlite::Result<WorkflowResult> {
    let created: String = row.get(2)?;
    let dt = DateTime::parse_from_rfc3339(&created)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(WorkflowResult { id: row.get(0)?, batch_id: row.get(1)?, created_at: dt })
}

fn row_to_chapter(row: &Row<'_>) -> rusqlite::Result<WorkflowResultChapter> {
    let created: String = row.get(4)?;
    let updated: String = row.get(5)?;
    let parse = |s: String, idx: usize| DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e)));
    Ok(WorkflowResultChapter {
        id: row.get(0)?,
        workflow_result_id: row.get(1)?,
        chapter_id: row.get(2)?,
        content: row.get(3)?,
        created_at: parse(created, 4)?,
        updated_at: parse(updated, 5)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::models::batch::{NewBatch, OnFailurePolicy};
    use crate::models::prompt::PromptKind;
    use crate::models::{
        NewChapter, NewDataAsset, NewModelConfig, NewTransformationNovel, NewUpload, Prompt,
    };

    // 注意:repo 的 MutexGuard 不可重入。别一边持有 `db.xxx()` 的 guard 一边调
    // `db.yyy()` / `db.lock()` —— 会自死锁(测试永久挂住而非失败)。

    /// 建 upload + da + 3 章 + tn + prompt + model + batch(pending)。
    fn seed(db: &Db) -> (i64, Vec<i64>) {
        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "w1".into(), filename: "f.txt".into(), byte_size: 1,
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

    /// 建槽:结果集 1:1 挂 batch,每章一个空槽(content 初始为 NULL)。
    #[test]
    fn create_makes_one_empty_slot_per_chapter() {
        let (db, batch_id, cids) = fresh();
        let r = db.workflow_results();
        let result_id = r.create_for_batch_with_slots(batch_id, &cids).unwrap();
        assert!(result_id > 0);

        let wr = r.get_by_batch(batch_id).unwrap().expect("结果集应存在");
        assert_eq!(wr.id, result_id);

        let slots = r.list_chapters(result_id).unwrap();
        assert_eq!(slots.len(), 3, "每章一个槽");
        for s in &slots {
            assert!(s.content.is_none(), "新槽的 content 应为 NULL(尚未写入)");
        }
        // 未写入时读取返回 None(不是 Some(""))
        for cid in &cids {
            assert!(r.get_content_by_batch_and_chapter(batch_id, *cid).unwrap().is_none());
        }
    }

    /// `INSERT OR IGNORE` + `UNIQUE(batch_id)` ⇒ 对同一 batch 重复调用是幂等的,
    /// 不会产生第二个结果集、也不会重复插槽。
    #[test]
    fn create_is_idempotent_for_same_batch() {
        let (db, batch_id, cids) = fresh();
        let r = db.workflow_results();
        let first = r.create_for_batch_with_slots(batch_id, &cids).unwrap();
        let second = r.create_for_batch_with_slots(batch_id, &cids).unwrap();
        assert_eq!(first, second, "同一 batch 只应有一个结果集");
        assert_eq!(r.list_chapters(first).unwrap().len(), 3, "不应重复插槽");

        // 部分章节重复调用同样安全(补插缺的槽)
        let third = r.create_for_batch_with_slots(batch_id, &cids[0..1]).unwrap();
        assert_eq!(third, first);
        assert_eq!(r.list_chapters(first).unwrap().len(), 3);
    }

    /// 写入正文后能按 (batch_id, chapter_id) 读回 —— 这是 prev_transformed 的来源。
    #[test]
    fn write_then_read_content_by_chapter() {
        let (db, batch_id, cids) = fresh();
        let r = db.workflow_results();
        r.create_for_batch_with_slots(batch_id, &cids).unwrap();

        r.write_content_by_chapter(batch_id, cids[1], "第一章的改写正文").unwrap();
        assert_eq!(
            r.get_content_by_batch_and_chapter(batch_id, cids[1]).unwrap().as_deref(),
            Some("第一章的改写正文")
        );
        // 其它槽仍为 NULL,不受影响
        assert!(r.get_content_by_batch_and_chapter(batch_id, cids[0]).unwrap().is_none());
        assert!(r.get_content_by_batch_and_chapter(batch_id, cids[2]).unwrap().is_none());

        // 覆盖写:重跑同一章应替换内容
        r.write_content_by_chapter(batch_id, cids[1], "第二次的正文").unwrap();
        assert_eq!(
            r.get_content_by_batch_and_chapter(batch_id, cids[1]).unwrap().as_deref(),
            Some("第二次的正文")
        );
    }

    /// 槽不存在 / 结果集不存在时**静默 noop 而非报错** —— 设计如此:
    /// worker 回调与 retry 路径无需先查 slot id。
    #[test]
    fn write_is_silent_noop_when_slot_missing() {
        let (db, batch_id, cids) = fresh();
        let r = db.workflow_results();

        // 结果集还没建:写入不应报错
        r.write_content_by_chapter(batch_id, cids[0], "内容").unwrap();
        assert!(r.get_content_by_batch_and_chapter(batch_id, cids[0]).unwrap().is_none());

        // 建了结果集但没有该章的槽
        let rid = r.create_for_batch_with_slots(batch_id, &cids[0..1]).unwrap();
        r.write_content_by_chapter(batch_id, cids[2], "没有槽的章节").unwrap();
        assert!(r.get_content_by_batch_and_chapter(batch_id, cids[2]).unwrap().is_none());
        assert_eq!(r.list_chapters(rid).unwrap().len(), 1);
    }

    /// 空串写入是"有内容但为空" —— 读取返回 Some("") 而非 None。
    /// 这个区分很重要:调用方靠 None 判断"还没转",靠 Some("") 判断"转过但为空"。
    #[test]
    fn empty_string_content_is_distinguishable_from_missing() {
        let (db, batch_id, cids) = fresh();
        let r = db.workflow_results();
        r.create_for_batch_with_slots(batch_id, &cids).unwrap();
        r.write_content_by_chapter(batch_id, cids[0], "").unwrap();
        assert_eq!(
            r.get_content_by_batch_and_chapter(batch_id, cids[0]).unwrap().as_deref(),
            Some(""),
            "空串是已写入的空内容,不是 NULL"
        );
        assert!(r.get_content_by_batch_and_chapter(batch_id, cids[1]).unwrap().is_none());
    }

    /// 没有结果集时 get_by_batch 返回 None(而不是报错)。
    #[test]
    fn get_by_batch_returns_none_when_absent() {
        let (db, batch_id, _cids) = fresh();
        assert!(db.workflow_results().get_by_batch(batch_id).unwrap().is_none());
    }

    /// list_chapters 按 chapter_id 升序(UI 展示顺序)。
    #[test]
    fn list_chapters_is_ordered_by_chapter_id() {
        let (db, batch_id, cids) = fresh();
        let r = db.workflow_results();
        let rid = r.create_for_batch_with_slots(batch_id, &cids).unwrap();
        let ids: Vec<i64> = r.list_chapters(rid).unwrap().iter().map(|s| s.chapter_id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }
}
