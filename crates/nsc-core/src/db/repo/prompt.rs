use std::sync::MutexGuard;
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{Prompt, PromptKind};
use crate::prompts::builtin_prompts;

/// repo 层写时用的"内容更新"结构 —— 不可改 id / is_builtin / archived。
/// 这是 §3.9 的落地:`PromptRepo::update` 不再接受 is_builtin / archived 字段,
/// 杜绝命令层之外的调用者误改。
#[derive(Debug, Clone)]
pub struct PromptUpdate<'a> {
    pub id: i64,
    pub name: &'a str,
    pub kind: PromptKind,
    pub template: &'a str,
}

pub struct PromptRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> PromptRepo<'a> {
    /// 默认列表:仅返回 `archived = 0` 的活动行(UI 主表格)。
    /// `include_archived = true` 时同时返回归档行(用于"显示已归档"切换)。
    pub fn list(&self, include_archived: bool) -> Result<Vec<Prompt>> {
        let sql = if include_archived {
            "SELECT id, name, kind, template, is_builtin, archived \
             FROM prompts ORDER BY archived ASC, id ASC"
        } else {
            "SELECT id, name, kind, template, is_builtin, archived \
             FROM prompts WHERE archived = 0 ORDER BY id ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 按 id 查单行 —— **不**过滤 archived。`BatchScheduler` / `transformation_chapters`
    /// 读 path 必须能拿到归档行,否则历史 tc 引用解析会断。
    pub fn get(&self, id: i64) -> Result<Option<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, template, is_builtin, archived \
             FROM prompts WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    /// insert —— 不带 archived(is_builtin 由调用方传,builtin 种子 1,用户 0)。
    pub fn insert(&self, p: &Prompt) -> Result<i64> {
        let kind = match p.kind {
            PromptKind::Compress => "compress",
            PromptKind::Style => "style",
        };
        self.conn.execute(
            "INSERT INTO prompts (name, kind, template, is_builtin) VALUES (?1, ?2, ?3, ?4)",
            params![p.name, kind, p.template, p.is_builtin as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 更新 name / kind / template —— id / is_builtin / archived **不可改**。
    /// 命令层先 `get` 旧行,再拿 `is_builtin` 拼 PromptUpdate;repo 不做这事。
    pub fn update(&self, u: &PromptUpdate<'_>) -> Result<()> {
        let kind = match u.kind {
            PromptKind::Compress => "compress",
            PromptKind::Style => "style",
        };
        self.conn.execute(
            "UPDATE prompts SET name=?2, kind=?3, template=?4 WHERE id=?1",
            params![u.id, u.name, kind, u.template],
        )?;
        Ok(())
    }

    /// 软删:`archived = 1`。行保留(builtin 行亦可软删 —— 用户可以"归档 builtin 不再用")。
    /// 与 model 不一样的是 prompt 没有密钥,所以只改 archived,不动其他字段。
    pub fn archive(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE prompts SET archived = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 取消软删:恢复 `archived = 0`。
    pub fn restore(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE prompts SET archived = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 统计 `transformation_chapters` 表里 prompt_id 等于参数的行数。
    /// 删除 prompt 前展示"被 N 个转换结果引用",N=0 不展示。
    pub fn count_by_prompt(&self, prompt_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE prompt_id = ?1",
            params![prompt_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// builtin 同步 —— 启动时保证 DB builtin 模板和 builtin.rs 源码一致。
    /// - count = 0(首次启动 / 用户全删了 builtin 行):INSERT 新行。
    /// - count > 0:UPDATE 已存在的 builtin 行的 `template` 到源码。
    ///   UI 不允许编辑 builtin(只能查看),所以 UPDATE 不会覆盖用户改动;
    ///   用户从 builtin 复制后保存的提示词 `is_builtin = 0`,也不在 UPDATE 范围。
    ///   这一步同时承担了 builtin.rs 改模板后的「启动期自动同步」——
    ///   不再需要为每次源码改动写 migration 同步 DB(参见之前 0014/0028 的做法)。
    /// - `archived = 1` 的行不在 UPDATE 范围(§1.3 trade-off 不变:
    ///   用户显式归档 builtin 后,启动不会"自作主张"复活)。
    pub fn seed_builtin_if_empty(&self) -> Result<()> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM prompts WHERE is_builtin = 1",
            [],
            |r| r.get(0),
        )?;
        let tx = self.conn.unchecked_transaction()?;
        if count == 0 {
            for bp in builtin_prompts() {
                let kind = match bp.kind {
                    PromptKind::Compress => "compress",
                    PromptKind::Style => "style",
                };
                tx.execute(
                    "INSERT INTO prompts (name, kind, template, is_builtin) VALUES (?1, ?2, ?3, 1)",
                    params![bp.name, kind, bp.template],
                )?;
            }
        } else {
            for bp in builtin_prompts() {
                tx.execute(
                    "UPDATE prompts SET template = ?1 \
                     WHERE name = ?2 AND is_builtin = 1 AND archived = 0",
                    params![bp.template, bp.name],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn from_row(row: &Row) -> rusqlite::Result<Prompt> {
    let kind_s: String = row.get(2)?;
    let is_builtin: i64 = row.get(4)?;
    Ok(Prompt {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: match kind_s.as_str() {
            "compress" => PromptKind::Compress,
            _ => PromptKind::Style,
        },
        template: row.get(3)?,
        is_builtin: is_builtin != 0,
        archived: row.get(5)?,
    })
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::models::{Prompt, PromptKind};

    fn fresh_db() -> Db {
        Db::open_in_memory().expect("open in-memory db")
    }

    #[test]
    fn seed_inserts_when_table_empty() {
        let db = fresh_db();
        db.prompts().seed_builtin_if_empty().expect("seed");
        let all = db.prompts().list(false).expect("list");
        let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"compress_default"));
        assert!(names.contains(&"style_default"));
        // builtin.rs 当前模板必须含 {{prev_original}} / {{prev_transformed}} / {{next_original}}
        let style = all.iter().find(|p| p.name == "style_default").unwrap();
        assert!(style.template.contains("{{prev_original}}"));
        assert!(style.template.contains("{{prev_transformed}}"));
        assert!(style.template.contains("{{next_original}}"));
    }

    /// 用户当前 DB 状态 —— style_default 模板被某个 migration 不当执行污染了。
    /// seed_builtin_if_empty 必须能自动恢复(template 重新对齐 builtin.rs 源码)。
    #[test]
    fn seed_updates_existing_rows_to_match_source() {
        let db = fresh_db();
        db.prompts().insert(&Prompt {
            id: 0, name: "compress_default".into(), kind: PromptKind::Compress,
            template: "outdated compress template".into(), is_builtin: true, archived: 0,
        }).expect("insert compress");
        db.prompts().insert(&Prompt {
            id: 0, name: "style_default".into(), kind: PromptKind::Style,
            template: "garbage\r
WHERE name = 'foo';\r
UPDATE prompts SET template = 'broken".into(),
            is_builtin: true, archived: 0,
        }).expect("insert style");
        db.prompts().seed_builtin_if_empty().expect("seed");
        let style = db.prompts().list(false).expect("list").into_iter()
            .find(|p| p.name == "style_default").expect("style row");
        assert!(style.template.contains("{{prev_original}}"));
        assert!(style.template.contains("{{next_original}}"));
        assert!(!style.template.contains("UPDATE prompts SET template"));
        assert!(!style.template.contains("WHERE name = 'foo'"));
    }

    /// archived=1 的 builtin 行不在 seed 覆盖范围,符合 §1.3 trade-off。
    /// 真实业务流:用户在 UI 归档 builtin → 重启 → seed 不应复活。
    /// 注意 `insert()` 不带 archived 字段(只有 archive()/restore() 改它),
    /// 所以测试必须 insert 后调 archive() 来模拟归档状态。
    #[test]
    fn seed_skips_archived_builtins() {
        let db = fresh_db();
        let id = db.prompts().insert(&Prompt {
            id: 0, name: "compress_default".into(), kind: PromptKind::Compress,
            template: "user modified".into(), is_builtin: true, archived: 0,
        }).expect("insert");
        db.prompts().archive(id).expect("archive");
        db.prompts().seed_builtin_if_empty().expect("seed");
        let p = db.prompts().list(true).expect("list").into_iter()
            .find(|p| p.name == "compress_default").expect("compress row");
        assert_eq!(p.template, "user modified");
        assert_eq!(p.archived, 1);
    }

    // ── CRUD 与归档(上面三个测试只覆盖 seed 路径) ──────────────────────────

    fn user_prompt(name: &str, kind: PromptKind, tmpl: &str) -> Prompt {
        Prompt {
            id: 0, name: name.into(), kind, template: tmpl.into(),
            is_builtin: false, archived: 0,
        }
    }

    /// insert → get 往返:kind 的枚举 ↔ 字符串转换、is_builtin 的 bool ↔ INTEGER 转换。
    #[test]
    fn insert_and_get_roundtrip_both_kinds() {
        let db = fresh_db();
        for kind in [PromptKind::Compress, PromptKind::Style] {
            let id = db.prompts().insert(&user_prompt("p", kind, "模板")).unwrap();
            let got = db.prompts().get(id).unwrap().unwrap();
            assert_eq!(got.kind, kind, "kind 应原样往返");
            assert_eq!(got.name, "p");
            assert_eq!(got.template, "模板");
            assert!(!got.is_builtin, "用户 prompt 的 is_builtin 应为 false");
            assert_eq!(got.archived, 0);
        }
        let id = db.prompts().insert(&Prompt {
            id: 0, name: "b".into(), kind: PromptKind::Compress,
            template: "x".into(), is_builtin: true, archived: 0,
        }).unwrap();
        assert!(db.prompts().get(id).unwrap().unwrap().is_builtin, "builtin=true 应往返");
    }

    /// `list(false)` 隐藏归档行,`list(true)` 返回全部并按 archived ASC 排
    /// (活动的在前,归档的在后)。
    #[test]
    fn list_filters_archived_unless_requested() {
        let db = fresh_db();
        let active = db.prompts().insert(&user_prompt("active", PromptKind::Compress, "t")).unwrap();
        let gone = db.prompts().insert(&user_prompt("gone", PromptKind::Style, "t")).unwrap();
        db.prompts().archive(gone).unwrap();

        let visible = db.prompts().list(false).unwrap();
        assert_eq!(visible.len(), 1, "默认列表应隐藏归档行");
        assert_eq!(visible[0].id, active);

        let all = db.prompts().list(true).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, active, "活动的应排在归档的前面");
        assert_eq!(all[1].id, gone);
    }

    /// **`get` 不过滤 archived** —— BatchScheduler / transformation_chapters 的读路径
    /// 必须能拿到归档行,否则历史 tc 引用解析会断。与 model_config 同一套不对称设计。
    #[test]
    fn get_returns_archived_row() {
        let db = fresh_db();
        let id = db.prompts().insert(&user_prompt("p", PromptKind::Compress, "t")).unwrap();
        db.prompts().archive(id).unwrap();
        let got = db.prompts().get(id).expect("get 应能读到归档行").unwrap();
        assert_eq!(got.archived, 1);
    }

    /// update 只改 name / kind / template;id 不变,is_builtin 与 archived **都不可改**
    /// (§3.9:PromptUpdate 结构本身就不含这两个字段,从类型上杜绝误改)。
    #[test]
    fn update_changes_only_name_kind_template() {
        let db = fresh_db();
        let id = db.prompts().insert(&Prompt {
            id: 0, name: "旧名".into(), kind: PromptKind::Compress,
            template: "旧模板".into(), is_builtin: true, archived: 0,
        }).unwrap();

        db.prompts().update(&PromptUpdate {
            id, name: "新名", kind: PromptKind::Style, template: "新模板",
        }).unwrap();

        let after = db.prompts().get(id).unwrap().unwrap();
        assert_eq!(after.name, "新名");
        assert_eq!(after.kind, PromptKind::Style);
        assert_eq!(after.template, "新模板");
        assert!(after.is_builtin, "update 不应改动 is_builtin");
        assert_eq!(after.archived, 0, "update 不应改动 archived");

        // 归档行被 update 后仍是归档的
        db.prompts().archive(id).unwrap();
        db.prompts().update(&PromptUpdate {
            id, name: "又改名", kind: PromptKind::Compress, template: "t2",
        }).unwrap();
        let after2 = db.prompts().get(id).unwrap().unwrap();
        assert_eq!(after2.name, "又改名");
        assert_eq!(after2.archived, 1, "update 不应把归档行复活");
    }

    /// archive → restore 往返;prompt 没有密钥,所以归档**不动** template
    /// (与 model_config 归档抹 api_key 的行为刻意不同)。
    #[test]
    fn archive_and_restore_keep_template_intact() {
        let db = fresh_db();
        let id = db.prompts().insert(&user_prompt("p", PromptKind::Compress, "重要模板")).unwrap();

        db.prompts().archive(id).unwrap();
        let archived = db.prompts().get(id).unwrap().unwrap();
        assert_eq!(archived.archived, 1);
        assert_eq!(archived.template, "重要模板", "prompt 无密钥,归档不该抹模板");

        db.prompts().restore(id).unwrap();
        let restored = db.prompts().get(id).unwrap().unwrap();
        assert_eq!(restored.archived, 0);
        assert_eq!(restored.template, "重要模板");
    }

    /// 不存在的 id:get 返回 None;archive / restore 静默无行可改(不报错)。
    #[test]
    fn missing_id_is_not_an_error() {
        let db = fresh_db();
        assert!(db.prompts().get(99999).unwrap().is_none());
        db.prompts().archive(99999).unwrap();
        db.prompts().restore(99999).unwrap();
    }

    /// `count_by_prompt` 统计被多少条 transformation_chapters 引用 ——
    /// UI 在删除前用它提示"被 N 个转换结果引用"。
    #[test]
    fn count_by_prompt_counts_referencing_chapters() {
        use crate::models::batch::{NewBatch, OnFailurePolicy};
        use crate::models::{
            NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
            NewTransformationNovel, NewUpload,
        };

        let db = fresh_db();
        let prompt_id = db.prompts().insert(&user_prompt("p", PromptKind::Compress, "t")).unwrap();
        let other_prompt = db.prompts().insert(&user_prompt("q", PromptKind::Style, "t")).unwrap();
        assert_eq!(db.prompts().count_by_prompt(prompt_id).unwrap(), 0, "尚无引用时为 0");

        let upload_id = db.uploads().insert(&NewUpload {
            sha256: "pu1".into(), filename: "f.txt".into(), byte_size: 1,
            file_path: "/t".into(), original_text: "x".into(), word_count: 1,
        }).unwrap();
        let da_id = db.data_assets().insert(&NewDataAsset {
            upload_id, title: "da".into(), source_filename: "f.txt".into(),
            ..Default::default()
        }).unwrap();
        let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
            data_asset_id: da_id, title: "tn".into(), note: String::new(),
        }).unwrap();
        let model_id = db.model_configs().insert(&NewModelConfig {
            name: "m".into(), base_url: "http://x".into(), api_key: "k".into(),
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
        // 两个章节引 prompt_id,另一个引 other_prompt
        let mut refs = 0;
        for (i, pid) in [prompt_id, prompt_id, other_prompt].iter().enumerate() {
            let cid = db.chapters().insert(&NewChapter {
                data_asset_id: da_id, idx: i as i32 + 1, title: format!("c{i}"),
                body: "正文".into(), word_count: 2, ..Default::default()
            }).unwrap();
            db.transformation_chapters().insert(&NewTransformationChapter {
                transformation_novel_id: tn_id, chapter_id: cid,
                mode: PromptKind::Compress, prompt_id: *pid, model_config_id: model_id,
                ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
                batch_id: Some(batch_id), style_ref_chapter_id: None,
            }).unwrap();
            if *pid == prompt_id { refs += 1; }
        }
        assert_eq!(refs, 2);
        assert_eq!(db.prompts().count_by_prompt(prompt_id).unwrap(), 2,
            "应数出引用该 prompt 的 tc 行数");
        assert_eq!(db.prompts().count_by_prompt(other_prompt).unwrap(), 1);
    }
}
