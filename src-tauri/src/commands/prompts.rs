use std::sync::Arc;

use nsc_core::db::repo::prompt::PromptUpdate;
use nsc_core::db::Db;
use nsc_core::models::{Prompt, PromptKind};
use serde::Deserialize;
use tauri::State;

/// 默认列表(仅 `archived = 0`)。各 dialog 拉 prompt 下拉用。
#[tauri::command]
pub fn list_prompts(db: State<'_, Arc<Db>>) -> Result<Vec<Prompt>, String> {
    db.prompts().list(false).map_err(|e| e.to_string())
}

/// 含归档的列表 —— Prompts.vue "显示已归档"开关用。
#[tauri::command]
pub fn list_prompts_including_archived(db: State<'_, Arc<Db>>) -> Result<Vec<Prompt>, String> {
    db.prompts().list(true).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_prompt(db: State<'_, Arc<Db>>, id: i64) -> Result<Prompt, String> {
    db.prompts()
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("prompt {id} 不存在"))
}

/// `upsert_prompt` 入参。`id == 0` 走 insert(返回新 id);>0 走 update(返回传入 id)。
/// 内层 DTO 字段名直接对应 Rust 字段 —— 后端 `PromptKind` 用 `#[serde(rename_all="snake_case")]`
/// 自动映射 `"compress"` / `"style"`。
#[derive(Debug, Deserialize)]
pub struct PromptInput {
    pub id: i64,
    pub name: String,
    pub kind: PromptKind,
    pub template: String,
}

#[tauri::command]
pub fn upsert_prompt(
    db: State<'_, Arc<Db>>,
    payload: PromptInput,
) -> Result<i64, String> {
    if payload.id == 0 {
        let new = Prompt {
            id: 0,
            name: payload.name,
            kind: payload.kind,
            template: payload.template,
            is_builtin: false,
            archived: 0,
        };
        db.prompts().insert(&new).map_err(|e| e.to_string())
    } else {
        // update 路径:不读 is_builtin(用户 upsert 永远不动 builtin 标记),
        // 但读出来用于 PromptUpdate 仅做"内容更新"。
        let existing = db
            .prompts()
            .get(payload.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("prompt {} 不存在", payload.id))?;
        if existing.is_builtin {
            return Err("内置 prompt 不可编辑 — 请先复制为用户 prompt".into());
        }
        let update = PromptUpdate {
            id: existing.id,
            name: &payload.name,
            kind: payload.kind,
            template: &payload.template,
        };
        db.prompts().update(&update).map_err(|e| e.to_string())?;
        Ok(update.id)
    }
}

/// 软删:`archived = 1`。builtin 行可被软删(seed_builtin_if_empty 看到 archived=1
/// 仍算 count >= 1,不再种入)。
#[tauri::command]
pub fn delete_prompt(db: State<'_, Arc<Db>>, id: i64) -> Result<(), String> {
    db.prompts().archive(id).map_err(|e| e.to_string())
}

/// 取消软删:恢复 `archived = 0`。
#[tauri::command]
pub fn restore_prompt(db: State<'_, Arc<Db>>, id: i64) -> Result<(), String> {
    db.prompts().restore(id).map_err(|e| e.to_string())
}

/// 统计 prompt 被 `transformation_chapters` 引用的次数。
/// 前端删除 prompt 前展示"被 N 个转换结果引用",N=0 不展示。
#[tauri::command]
pub fn count_prompt_usage(db: State<'_, Arc<Db>>, prompt_id: i64) -> Result<i64, String> {
    db.prompts()
        .count_by_prompt(prompt_id)
        .map_err(|e| e.to_string())
}
