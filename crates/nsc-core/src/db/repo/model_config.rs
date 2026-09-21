use std::sync::MutexGuard;
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{ModelConfig, NewModelConfig};

pub struct ModelConfigRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> ModelConfigRepo<'a> {
    pub fn insert(&self, m: &NewModelConfig) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO model_configs \
             (name, base_url, api_key, model, max_tokens, max_context, temperature, disable_thinking, concurrency) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                m.name, m.base_url, m.api_key, m.model,
                m.max_tokens, m.max_context, m.temperature, if m.disable_thinking { 1 } else { 0 }, m.concurrency,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 默认列表:仅返回 `archived = 0` 的活动行(UI 主表格)。
    /// `include_archived = true` 时同时返回归档行(用于“显示已归档”切换)。
    pub fn list(&self, include_archived: bool) -> Result<Vec<ModelConfig>> {
        let sql = if include_archived {
            "SELECT id, name, base_url, api_key, model, max_tokens, max_context, temperature, disable_thinking, concurrency, archived \
             FROM model_configs ORDER BY archived ASC, id DESC"
        } else {
            "SELECT id, name, base_url, api_key, model, max_tokens, max_context, temperature, disable_thinking, concurrency, archived \
             FROM model_configs WHERE archived = 0 ORDER BY id DESC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 按 id 查单行 —— **不**过滤 archived。`BatchScheduler` / `transformation_chapters`
    /// 读 path 必须能拿到归档行,否则历史 tc 引用解析会断。
    pub fn get(&self, id: i64) -> Result<Option<ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, base_url, api_key, model, max_tokens, max_context, temperature, disable_thinking, concurrency, archived \
             FROM model_configs WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn update(&self, m: &ModelConfig) -> Result<()> {
        // update 不动 archived(软删只能通过 archive()/restore())。
        self.conn.execute(
            "UPDATE model_configs SET \
             name=?2, base_url=?3, api_key=?4, model=?5, \
             max_tokens=?6, max_context=?7, temperature=?8, disable_thinking=?9, concurrency=?10 \
             WHERE id=?1",
            params![
                m.id, m.name, m.base_url, m.api_key, m.model,
                m.max_tokens, m.max_context, m.temperature, if m.disable_thinking { 1 } else { 0 }, m.concurrency,
            ],
        )?;
        Ok(())
    }

    /// 软删:`archived = 1` + `api_key = ''` —— 后者保证密钥不随归档条目被任何 dump 出来。
    /// 行保留以便 `transformation_chapters.model_config_id` 仍能查到历史 model 元数据(name / base_url / model / concurrency)做展示。
    /// 仍能查到历史 model 元数据(name / base_url / model / concurrency)做展示。
    pub fn archive(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE model_configs SET archived = 1, api_key = '' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 取消软删:恢复 `archived = 0`。注意:被抹掉的 `api_key` **不会** 自动恢复,
    /// 用户需要重新编辑并保存。
    pub fn restore(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE model_configs SET archived = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }
}

fn from_row(row: &Row) -> rusqlite::Result<ModelConfig> {
    Ok(ModelConfig {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        api_key: row.get(3)?,
        model: row.get(4)?,
        max_tokens: row.get(5)?,
        max_context: row.get(6)?,
        temperature: row.get(7)?,
        disable_thinking: row.get::<_, i64>(8)? != 0,
        concurrency: row.get(9)?,
        archived: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::NewModelConfig;
    use crate::db::Db;

    fn new_cfg(name: &str) -> NewModelConfig {
        NewModelConfig {
            name: name.into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: "sk-secret".into(),
            model: "gpt-x".into(),
            max_tokens: Some(4096),
            max_context: Some(128000),
            temperature: Some(0.7),
            disable_thinking: false,
            concurrency: 2,
        }
    }

    /// insert → get 的字段往返(含 Option 字段与 bool → INTEGER 的转换)。
    #[test]
    fn insert_and_get_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&new_cfg("m1")).unwrap();
        let got = r.get(id).unwrap().unwrap();
        assert_eq!(got.name, "m1");
        assert_eq!(got.base_url, "https://api.example.com/v1");
        assert_eq!(got.api_key, "sk-secret");
        assert_eq!(got.model, "gpt-x");
        assert_eq!(got.max_tokens, Some(4096));
        assert_eq!(got.max_context, Some(128000));
        assert_eq!(got.temperature, Some(0.7));
        assert!(!got.disable_thinking);
        assert_eq!(got.concurrency, 2);
        assert_eq!(got.archived, 0);
    }

    /// 可选字段留空时往返仍是 None(不是 0)。
    #[test]
    fn null_optionals_stay_null() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&NewModelConfig {
            name: "n".into(), base_url: "u".into(), api_key: "k".into(),
            model: "m".into(), max_tokens: None, max_context: None, temperature: None,
            disable_thinking: true, concurrency: 1,
        }).unwrap();
        let got = r.get(id).unwrap().unwrap();
        assert!(got.max_tokens.is_none());
        assert!(got.max_context.is_none());
        assert!(got.temperature.is_none());
        assert!(got.disable_thinking, "bool 应经 INTEGER 正确往返");
    }

    /// `list(false)` 隐藏归档行;`list(true)` 返回全部 —— UI「显示已归档」开关靠它。
    #[test]
    fn list_filters_archived_unless_requested() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let active = r.insert(&new_cfg("active")).unwrap();
        let gone = r.insert(&new_cfg("archived")).unwrap();
        r.archive(gone).unwrap();

        let visible = r.list(false).unwrap();
        assert_eq!(visible.len(), 1, "默认列表应隐藏归档行");
        assert_eq!(visible[0].id, active);

        let all = r.list(true).unwrap();
        assert_eq!(all.len(), 2, "include_archived 应返回全部");
        assert!(all.iter().any(|m| m.id == gone));
    }

    /// **`get` 不过滤 archived** —— BatchScheduler / transformation_chapters 的读路径
    /// 必须能拿到归档行,否则历史 tc 引用解析会断。这条是刻意的不对称设计。
    #[test]
    fn get_returns_archived_row() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&new_cfg("m")).unwrap();
        r.archive(id).unwrap();
        let got = r.get(id).expect("get 应能读到归档行").unwrap();
        assert_eq!(got.archived, 1);
    }

    /// archive 抹掉 api_key(密钥不随归档条目被 dump 出去),但保留其余元数据。
    /// 这是安全相关的行为,值得钉死。
    #[test]
    fn archive_wipes_api_key_but_keeps_metadata() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&new_cfg("m")).unwrap();
        r.archive(id).unwrap();
        let got = r.get(id).unwrap().unwrap();
        assert_eq!(got.archived, 1);
        assert_eq!(got.api_key, "", "归档必须抹掉 api_key");
        assert_eq!(got.name, "m", "其余元数据保留供历史展示");
        assert_eq!(got.base_url, "https://api.example.com/v1");
        assert_eq!(got.model, "gpt-x");
        assert_eq!(got.concurrency, 2);
    }

    /// restore 只把 archived 置 0;**api_key 不会自动恢复**,用户需重新编辑保存。
    #[test]
    fn restore_does_not_bring_back_api_key() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&new_cfg("m")).unwrap();
        r.archive(id).unwrap();
        r.restore(id).unwrap();
        let got = r.get(id).unwrap().unwrap();
        assert_eq!(got.archived, 0);
        assert_eq!(got.api_key, "", "restore 不应恢复已抹掉的 key");
    }

    /// `update` 不动 archived —— 软删只能经 archive()/restore()。
    #[test]
    fn update_does_not_change_archived() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&new_cfg("m")).unwrap();
        r.archive(id).unwrap();

        let mut edited = r.get(id).unwrap().unwrap();
        edited.name = "改名".into();
        edited.api_key = "sk-new".into(); // 用户重新填 key
        edited.archived = 0; // 试图顺手"恢复"——不该生效
        r.update(&edited).unwrap();

        let after = r.get(id).unwrap().unwrap();
        assert_eq!(after.name, "改名");
        assert_eq!(after.api_key, "sk-new", "update 应写入新的 api_key");
        assert_eq!(after.archived, 1, "update 不应改动 archived");
    }

    /// update 覆盖全部可编辑字段(含 Option 与 bool)。
    #[test]
    fn update_overwrites_editable_fields() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        let id = r.insert(&new_cfg("m")).unwrap();
        let mut m = r.get(id).unwrap().unwrap();
        m.max_tokens = None;
        m.max_context = Some(32000);
        m.temperature = None;
        m.disable_thinking = true;
        m.concurrency = 5;
        r.update(&m).unwrap();

        let after = r.get(id).unwrap().unwrap();
        assert!(after.max_tokens.is_none(), "改成 None 应真的写 NULL");
        assert_eq!(after.max_context, Some(32000));
        assert!(after.temperature.is_none());
        assert!(after.disable_thinking);
        assert_eq!(after.concurrency, 5);
    }

    /// 不存在的 id:get 返回 None(由调用方决定是否报错),archive/restore 静默无行可改。
    #[test]
    fn missing_id_is_not_an_error() {
        let db = Db::open_in_memory().unwrap();
        let r = db.model_configs();
        assert!(r.get(99999).unwrap().is_none());
        r.archive(99999).unwrap();
        r.restore(99999).unwrap();
    }
}
