use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::Upload;
use nsc_core::upload;

#[derive(Debug, Serialize)]
pub struct UploadSummary {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub uploaded_at: String,
    pub file_path: String,
    pub word_count: i64,
}

#[derive(Debug, Serialize)]
pub struct DataAssetRef {
    pub id: i64,
    pub title: String,
    pub chapters_count: i64,
    pub tn_count: i64,
}

#[derive(Debug, Serialize)]
pub struct UploadDeletePreview {
    pub id: i64,
    pub filename: String,
    pub derived_data_assets: Vec<DataAssetRef>,
}

#[derive(Debug, Deserialize)]
pub struct UploadFilePayload {
    pub file_path: String,
    pub filename: String,
}

fn uploads_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("novel-style-converter").join("uploads")
}

fn to_summary(u: &Upload) -> UploadSummary {
    UploadSummary {
        id: u.id,
        sha256: u.sha256.clone(),
        filename: u.filename.clone(),
        byte_size: u.byte_size,
        uploaded_at: u.uploaded_at.to_rfc3339(),
        file_path: u.file_path.clone(),
        word_count: u.word_count,
    }
}

pub fn read_upload_original_text(u: &Upload) -> Result<String, String> {
    if u.original_text.is_empty() {
        return Err(format!(
            "upload {} ({}) 的 original_text 为空，文件路径 {}。请重新上传该文件。",
            u.id, u.filename, u.file_path
        ));
    }
    Ok(u.original_text.clone())
}

#[tauri::command]
pub fn list_uploads(db: State<'_, Arc<Db>>) -> Result<Vec<UploadSummary>, String> {
    let ups = db.uploads().list().map_err(|e| e.to_string())?;
    Ok(ups.iter().map(to_summary).collect())
}

#[tauri::command]
pub fn upload_file(
    db: State<'_, Arc<Db>>,
    payload: UploadFilePayload,
) -> Result<UploadSummary, String> {
    let dir = uploads_dir();
    let source = PathBuf::from(&payload.file_path);
    let u = upload::upload_file(&db, &source, &payload.filename, &dir)
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&u))
}

/// 删 upload 前预览：列出该 upload 派生的所有 data_asset（含 title/章节数/TN 数）。
/// 前端弹窗用此信息提示用户"这些 data_asset 会变孤儿，需要去 data_asset 页手动删"。
/// 注意：data_asset 不再被 CASCADE 删除（migration 0015 已断 FK）。
#[tauri::command]
pub fn preview_upload_deletion(
    db: State<'_, Arc<Db>>,
    upload_id: i64,
) -> Result<UploadDeletePreview, String> {
    let u = db.uploads().get(upload_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {upload_id} 不存在"))?;
    let derived: Vec<DataAssetRef> = {
        let das = db.data_assets().find_by_upload(upload_id).map_err(|e| e.to_string())?;
        das.into_iter().map(|d| {
            let chapters_count = db.chapters().list_by_data_asset(d.id).map(|v| v.len() as i64).unwrap_or(0);
            let tn_count = db.transformation_novels().list_by_data_asset(d.id).map(|v| v.len() as i64).unwrap_or(0);
            DataAssetRef {
                id: d.id,
                title: d.title,
                chapters_count,
                tn_count,
            }
        }).collect()
    };
    Ok(UploadDeletePreview {
        id: u.id,
        filename: u.filename.clone(),
        derived_data_assets: derived,
    })
}

/// 直接删除 upload + 文件。FK 已断，data_asset 不会被带走。
/// 前端在弹窗预览后用户确认后调用此命令。
#[tauri::command]
pub fn delete_upload(db: State<'_, Arc<Db>>, id: i64) -> Result<(), String> {
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    let _ = std::fs::remove_file(&u.file_path);
    db.uploads().delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_upload(db: State<'_, Arc<Db>>, id: i64) -> Result<UploadSummary, String> {
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    Ok(to_summary(&u))
}

#[tauri::command]
pub fn get_upload_text(db: State<'_, Arc<Db>>, id: i64) -> Result<Response, String> {
    let text = {
        let u = db.uploads().get(id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {id} 不存在"))?;
        if u.original_text.is_empty() {
            return Err(format!("upload {} 的 original_text 为空。请重新上传该文件。", u.id));
        }
        u.original_text.clone()
    };
    Ok(Response::new(text.into_bytes()))
}

/// 改 upload.original_text（清洗/手动编辑）。同步刷 word_count。
/// 注意：不影响已存在的 data_asset（它们有独立 source_filename 副本 + chapter.body）。
#[tauri::command]
pub fn update_upload_text(
    db: State<'_, Arc<Db>>,
    id: i64,
    text: String,
) -> Result<(), String> {
    if db.uploads().get(id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("upload {id} 不存在"));
    }
    db.uploads().set_original_text(id, &text).map_err(|e| e.to_string())
}


/// 按字节区间返回 upload 文本。offset/length 都会向最近的 UTF-8 字符边界对齐,
/// 避免在多字节字符中间切断。前端按固定步长串行请求以实现大文件懒加载:
/// 单次返回 ≤ CHUNK_LOAD_STEP 字节,UI 边收边显示,避免一次性渲染 N MB textarea 卡顿。
#[tauri::command]
pub fn get_upload_text_chunk(
    db: State<'_, Arc<Db>>,
    id: i64,
    byte_offset: usize,
    byte_length: usize,
) -> Result<Response, String> {
    let text = {
        let u = db.uploads().get(id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {id} 不存在"))?;
        if u.original_text.is_empty() {
            return Err(format!(
                "upload {} 的 original_text 为空，请重新上传该文件",
                u.id,
            ));
        }
        u.original_text.clone()
    };
    let bytes = text.as_bytes();
    let total = bytes.len();
    if byte_offset >= total {
        return Ok(Response::new(Vec::new()));
    }
    let end_byte = (byte_offset + byte_length).min(total);
    let mut start = byte_offset;
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = end_byte;
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(Response::new(bytes[start..end].to_vec()))
}
