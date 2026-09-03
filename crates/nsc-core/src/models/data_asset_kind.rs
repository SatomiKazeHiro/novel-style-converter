use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataAssetKind {
    Source,
    Promoted,
}

impl DataAssetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Promoted => "promoted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "source" => Some(Self::Source),
            "promoted" => Some(Self::Promoted),
            _ => None,
        }
    }
}