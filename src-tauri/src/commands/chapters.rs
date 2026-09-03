use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::Chapter;
use nsc_core::splitter::{ChapterSplitter, DefaultSplitter, SplitResult};

/// parse 阶段返回的章节预览：title + body（不再带 byte 偏移）。
/// 用户在 parse.vue 编辑标题 / 合并 / 加 marker 重切时，后端直接返回完整正文。
#[derive(Debug, Serialize)]
pub struct ChapterSegment {
    pub title: String,
    pub content: String,
    pub word_count: i32,
    /// 标题在 upload.original_text 里的 0-based 行号 —— parse 页专用,
    /// 让前端在 split 结果上挂"跳到原文行 / 重切"等动作时拿得到原文坐标。
    pub title_line: i32,
}

#[derive(Debug, Serialize)]
pub struct ChapterMeta {
    pub id: i64,
    pub idx: i32,
    pub title: String,
    pub word_count: i32,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct ChapterContentRow {
    pub idx: i32,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChapterInput {
    pub title: String,
    pub content: String,
    /// 标题在 upload.original_text 里的 0-based 行号 —— parse 页提交时一并带上,
    /// commit_data_asset 直接写入 chapter.title_line,后续 list_committed_segments 能拿到。
    pub title_line: i32,
}

/// parse wizard 入口：对 upload.original_text 跑 splitter，返回 ChapterSegment 列表直接展示。
/// 不再接 markers / suppressed —— 新 splitter 模型下标题由正则确定,无需前端补 marker。
#[tauri::command]
pub fn list_chapter_segments(
    db: State<'_, Arc<Db>>,
    upload_id: i64,
) -> Result<Vec<ChapterSegment>, String> {
    let text = {
        let u = db.uploads().get(upload_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {upload_id} 不存在"))?;
        crate::commands::uploads::read_upload_original_text(&u)?
    };

    let SplitResult { chapters } = DefaultSplitter.split(&text);
    Ok(chapters
        .into_iter()
        .map(|c| ChapterSegment {
            title: c.title,
            content: c.content,
            word_count: c.word_count,
            title_line: c.title_line as i32,
        })
        .collect())
}

/// 已提交章节列表（data_asset 视角）。
#[tauri::command]
pub fn get_chapter_contents(
    db: State<'_, Arc<Db>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterContentRow>, String> {
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.into_iter().map(|c| ChapterContentRow {
        idx: c.idx,
        title: c.title.clone(),
        content: c.body,
    }).collect())
}

/// 老接口：返回已提交章节的 title + word_count + title_line（不带 byte）。
/// 兼容老 parse.vue / dataAsset store 的旧字段。
/// title_line 为 NULL = 数据是更老路径写入的(promoted da / 老 splitter),
/// 这种数据没有原文坐标,parse 页也用不上,直接 fail-fast 抛错带诊断。
#[tauri::command]
pub fn list_committed_segments(
    db: State<'_, Arc<Db>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterSegment>, String> {
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    chapters.into_iter().map(|c| {
        let title_line = c.title_line
            .ok_or_else(|| format!("chapter {} title_line 为 NULL(data_asset_id={data_asset_id})", c.id))?;
        Ok(ChapterSegment {
            title: c.title,
            content: c.body,
            word_count: c.word_count,
            title_line,
        })
    }).collect()
}

/// 列出 data_asset 下全部章节（含 body），给前端做完整视图。
#[tauri::command]
pub fn list_chapters(
    db: State<'_, Arc<Db>>,
    data_asset_id: i64,
) -> Result<Vec<ChapterMeta>, String> {
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.into_iter().map(|c| ChapterMeta {
        id: c.id,
        idx: c.idx,
        title: c.title.clone(),
        word_count: c.word_count,
        body: c.body,
    }).collect())
}

#[tauri::command]
pub fn get_chapter(
    db: State<'_, Arc<Db>>,
    chapter_id: i64,
) -> Result<Chapter, String> {
    db.chapters().get(chapter_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("chapter {chapter_id} 不存在"))
}

/// 单章正文编辑。改 body 同时按 word::count 重算 word_count 落库。
/// 不动 idx / title / source_kind / source_chapter_id —— 这些是结构字段。
/// 不校验是否被 workflow 引用：chapter_id 是 FK，引用方(workflow_result_chapters.content)
/// 是独立 TEXT 列，改源 chapter 不会破坏已落库结果。
#[tauri::command]
pub fn update_chapter_body(
    db: State<'_, Arc<Db>>,
    chapter_id: i64,
    body: String,
) -> Result<(), String> {
    db.chapters().update_body(chapter_id, &body)
        .map_err(|e| e.to_string())
}
