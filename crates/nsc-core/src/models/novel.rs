use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// State 1: 一次上传 = 一份原始 .txt 文件。sha256 去重。
/// 章节结构不在此处,章节切片在 data_assets(已拆离)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub uploaded_at: DateTime<Utc>,
    pub file_path: String,
    /// 原文整篇。data_assets 的章节切片通过 byte_start/byte_end 在此坐标系定位。
    pub original_text: String,
    /// 字数(zh-aware:汉字 + 字母 + 数字)。upload_file() 时一次算好。
    /// UI 列表展示用,避免每次 list 都对原文做字符串扫描。
    pub word_count: i64,
}

#[derive(Debug, Clone)]
pub struct NewUpload {
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub file_path: String,
    pub original_text: String,
    pub word_count: i64,
}

/// 转换小说工作区,引用某个 data_asset(已解析的 State 2)。
/// 同一 data_asset 可被多本 transformation_novel 引用(fan-out)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationNovel {
    pub id: i64,
    pub data_asset_id: i64,
    pub title: String,
    pub created_at: DateTime<Utc>,
    /// 用户填的备注。空串等价于"无备注"。
    /// UI 在 TN 详情页头部只读展示,目前没有编辑入口。
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct NewTransformationNovel {
    pub data_asset_id: i64,
    pub title: String,
    /// 见 TransformationNovel.note。空串等价于"无备注"。
    pub note: String,
}
