use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub source_chapter_id: Option<i64>,
    /// NULL = 从未用户编辑;Some = 上次编辑时间(RFC3339)。跟 source_kind 正交。
    #[serde(default)]
    pub edited_at: Option<String>,
    /// 标题文本在 upload.original_text 里的 0-based 行号。None = 无原文坐标(promoted)。
    #[serde(default)]
    pub title_line: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct NewChapter {
    pub data_asset_id: i64,
    pub idx: i32,
    pub title: String,
    pub body: String,
    pub word_count: i32,
    pub source_kind: String,
    pub source_chapter_id: Option<i64>,
    pub title_line: Option<i32>,
}

impl Default for NewChapter {
    fn default() -> Self {
        Self {
            data_asset_id: 0,
            idx: 0,
            title: String::new(),
            body: String::new(),
            word_count: 0,
            source_kind: "original".into(),
            source_chapter_id: None,
            title_line: None,
        }
    }
}
