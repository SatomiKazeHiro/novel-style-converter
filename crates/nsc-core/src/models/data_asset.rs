use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAsset {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub source_filename: String,
    #[serde(default = "default_kind_source")]
    pub kind: crate::models::DataAssetKind,
    #[serde(default)]
    pub source_workflow_id: Option<i64>,
    #[serde(default)]
    pub source_data_asset_id: Option<i64>,
    #[serde(default)]
    pub note: String,
}

fn default_kind_source() -> crate::models::DataAssetKind {
    crate::models::DataAssetKind::Source
}

#[derive(Debug, Clone)]
pub struct NewDataAsset {
    pub upload_id: i64,
    pub title: String,
    pub source_filename: String,
    pub kind: crate::models::DataAssetKind,
    pub source_workflow_id: Option<i64>,
    pub source_data_asset_id: Option<i64>,
    pub note: String,
}

impl Default for NewDataAsset {
    fn default() -> Self {
        Self {
            upload_id: 0,
            title: String::new(),
            source_filename: String::new(),
            kind: crate::models::DataAssetKind::Source,
            source_workflow_id: None,
            source_data_asset_id: None,
            note: String::new(),
        }
    }
}
