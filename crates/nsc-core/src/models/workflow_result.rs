use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub id: i64,
    pub batch_id: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WorkflowResultChapter {
    pub id: i64,
    pub workflow_result_id: i64,
    pub chapter_id: i64,
    pub content: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
