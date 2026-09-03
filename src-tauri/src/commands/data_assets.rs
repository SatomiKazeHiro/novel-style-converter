use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::db::repo::data_asset::DataAssetWithUpload;
use nsc_core::models::{Chapter, NewChapter, NewDataAsset};

/// list_data_asset_chapters 返回的章节：自包含，含 body。
#[derive(Debug, Serialize)]
pub struct DataAssetChapter {
    pub id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
    /// 章节来源:transformed = 工作流转换结果;original = 原文(派生 da 失败章节回退)。
    /// 旧 da(migration 0021 之前)没有该字段,反序列化时默认 "original"。
    pub source_kind: String,
    #[serde(default)]
    pub source_chapter_id: Option<i64>,
    #[serde(default)]
    pub edited_at: Option<String>,
    /// 标题文本在 upload.original_text 里的 0-based 行号。None = 无原文坐标(promoted 章节)。
    #[serde(default)]
    pub title_line: Option<i32>,
}

impl From<&Chapter> for DataAssetChapter {
    fn from(c: &Chapter) -> Self {
        Self {
            id: c.id,
            idx: c.idx,
            title: c.title.clone(),
            body: c.body.clone(),
            word_count: c.word_count,
            source_kind: c.source_kind.clone(),
            source_chapter_id: c.source_chapter_id,
            edited_at: c.edited_at.clone(),
            title_line: c.title_line,
        }
    }
}

#[tauri::command]
pub fn list_data_asset_chapters(
    db: State<'_, Arc<Db>>,
    data_asset_id: i64,
) -> Result<Vec<DataAssetChapter>, String> {
    let chapters = db.chapters().list_by_data_asset(data_asset_id).map_err(|e| e.to_string())?;
    Ok(chapters.iter().map(DataAssetChapter::from).collect())
}

/// 新版 commit_data_asset：传入 title + 一组完整 ChapterInput（title + content）。
/// upload.original_text 在 commit 时拷到 data_asset.source_filename 之外的 metadata，
/// chapter 正文写入 chapter.body（不再走 byte 偏移）。
///
/// 允许同一 upload 创建多个 data_asset（不同清洗/不同切分）。
#[tauri::command]
pub fn commit_data_asset(
    db: State<'_, Arc<Db>>,
    upload_id: i64,
    title: String,
    chapters: Vec<crate::commands::chapters::ChapterInput>,
) -> Result<i64, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }

    let upload = db.uploads().get(upload_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {upload_id} 不存在"))?;
    let source_filename = upload.filename.clone();

    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id,
        title: title.to_string(),
        source_filename,
        ..Default::default()
    }).map_err(|e| e.to_string())?;

    let new_chapters: Vec<NewChapter> = chapters.into_iter().enumerate().map(|(i, c)| {
        let wc = nsc_core::text::word_count(&c.content);
        // idx 用 (i + 1):1-based,UI 列表里 #N 对应 idx=N,心智模型一致;
        // 此前 0-based 让 range picker 没法选 idx=0 的章节(选 1..N 会跳过第一行)。
        NewChapter {
            data_asset_id: da_id,
            idx: (i + 1) as i32,
            title: c.title,
            body: c.content,
            word_count: wc,
            title_line: Some(c.title_line),
            ..Default::default()
        }
    }).collect();

    db.chapters().insert_many(da_id, &new_chapters).map_err(|e| e.to_string())?;
    Ok(da_id)
}

#[derive(Debug, Serialize)]
pub struct DataAssetRow {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: String,
    pub filename: String,
    pub byte_size: i64,
    pub word_count: i64,
    pub tn_count: i64,
    pub kind: nsc_core::models::DataAssetKind,
    pub source_workflow_id: Option<i64>,
    pub source_data_asset_id: Option<i64>,
    pub note: String,
    pub promoted_count: i64,
}

impl From<DataAssetWithUpload> for DataAssetRow {
    fn from(d: DataAssetWithUpload) -> Self {
        Self {
            id: d.id,
            upload_id: d.upload_id,
            title: d.title,
            parsed_at: d.parsed_at.to_rfc3339(),
            filename: d.filename,
            byte_size: d.byte_size,
            word_count: d.word_count,
            tn_count: d.tn_count,
            kind: d.kind,
            source_workflow_id: d.source_workflow_id,
            source_data_asset_id: d.source_data_asset_id,
            note: String::new(),
            promoted_count: d.promoted_count,
        }
    }
}

#[tauri::command]
pub fn list_data_assets(
    db: State<'_, Arc<Db>>,
) -> Result<Vec<DataAssetRow>, String> {
    Ok(db.data_assets().list_with_upload()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(DataAssetRow::from)
        .collect())
}

/// 旧路由兼容：返回该 upload 派生的所有 data_asset.id。
/// 路由迁移（router beforeEach）取首条跳到 data_asset 详情。
#[tauri::command]
pub fn find_data_asset_by_upload(
    db: State<'_, Arc<Db>>,
    upload_id: i64,
) -> Result<Vec<i64>, String> {
    Ok(db.data_assets().find_by_upload(upload_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|d| d.id)
        .collect())
}

#[tauri::command]
pub fn delete_data_asset(
    db: State<'_, Arc<Db>>,
    data_asset_id: i64,
) -> Result<(), String> {
    db.data_assets().delete(data_asset_id)
        .map_err(|e| e.to_string())
}

/// 把一个 Stopped workflow 转正为新的 promoted data_asset。
/// 业务语义:见 spec §5.1 — 单事务,失败回滚。
#[tauri::command]
pub fn promote_workflow(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
    title: String,
) -> Result<nsc_core::models::DataAsset, String> {
    let new_id = db.promotion()
        .create_promoted_from_workflow(batch_id, title)
        .map_err(|e| e.to_string())?;
    db.data_assets().get(new_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("新 da {new_id} 找不到"))
}

/// 统计指定 workflow 已派生出多少 promoted data_asset。
#[tauri::command]
pub fn count_promoted_data_assets_by_workflow(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
) -> Result<i64, String> {
    db.promotion().count_by_workflow(batch_id).map_err(|e| e.to_string())
}

/// 列出指定 workflow 派生的所有 promoted data_asset。
#[tauri::command]
pub fn list_promoted_data_assets_for_workflow(
    db: State<'_, Arc<Db>>,
    batch_id: i64,
) -> Result<Vec<nsc_core::models::DataAsset>, String> {
    db.promotion().list_by_workflow(batch_id).map_err(|e| e.to_string())
}

/// 列出指定 upload 派生的所有 data_asset(包含 source + promoted)。
#[tauri::command]
pub fn list_data_assets_by_upload(
    db: State<'_, Arc<Db>>,
    upload_id: i64,
) -> Result<Vec<nsc_core::models::DataAsset>, String> {
    db.promotion().list_by_upload(upload_id).map_err(|e| e.to_string())
}
