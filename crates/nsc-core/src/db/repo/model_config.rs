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
