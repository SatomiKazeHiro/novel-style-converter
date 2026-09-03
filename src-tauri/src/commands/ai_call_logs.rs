use std::sync::Arc;

use nsc_core::db::Db;
use nsc_core::models::{AiCallBusiness, AiCallLogFilter, AiCallStatus};
use serde::Deserialize;
use tauri::State;

/// 列表过滤 DTO —— 前端发 snake_case,内层字段保持 Rust 原名。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AiCallLogFilterDto {
    pub business: Option<String>,
    pub model_config_id: Option<i64>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    /// 跳过行数(>=0)。传统 OFFSET 翻页,UI "第 N 页"导航。
    /// 负数视为 0,不抛错 —— UI 误传也最多是回到第 1 页,不是破坏性错误。
    pub offset: Option<i64>,
}

impl AiCallLogFilterDto {
    /// pub(super) 仅给同 crate 的测试用 —— IPC 入口还是 `list_ai_call_logs` 那个 wrapper。
    pub(super) fn into_filter(self) -> Result<AiCallLogFilter, String> {
        let business = match self.business.as_deref() {
            None => None,
            Some("transform_chapter") => Some(AiCallBusiness::TransformChapter),
            Some("test_model") => Some(AiCallBusiness::TestModel),
            Some("regenerate_preview") => Some(AiCallBusiness::RegeneratePreview),
            Some(other) => return Err(format!("unknown business: {other}")),
        };
        let status = match self.status.as_deref() {
            None => None,
            Some("success") => Some(AiCallStatus::Success),
            Some("failed") => Some(AiCallStatus::Failed),
            Some(other) => return Err(format!("unknown status: {other}")),
        };
        let offset = self.offset.filter(|n| *n >= 0);
        Ok(AiCallLogFilter {
            business,
            model_config_id: self.model_config_id,
            status,
            limit: self.limit,
            offset,
        })
    }
}

/// list 返回包装:(日志 + 总行数)。total 与 logs 在同一连接上串行查,
/// UI 用 total 算 "共 N 条 / 共 X 页"。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AiCallLogPage {
    pub logs: Vec<nsc_core::models::AiCallLog>,
    pub total: i64,
}

/// 列表:按 filter 过滤,时间倒序,带 offset 翻页。返回 (logs, total)。
#[tauri::command]
pub fn list_ai_call_logs(
    db: State<'_, Arc<Db>>,
    filter: AiCallLogFilterDto,
) -> Result<AiCallLogPage, String> {
    let f = filter.into_filter()?;
    let (logs, total) = db.ai_call_logs().list(&f).map_err(|e| e.to_string())?;
    Ok(AiCallLogPage { logs, total })
}

/// 单行详情:UI 详情页拉。
#[tauri::command]
pub fn get_ai_call_log(
    db: State<'_, Arc<Db>>,
    id: i64,
) -> Result<Option<nsc_core::models::AiCallLog>, String> {
    db.ai_call_logs().get(id).map_err(|e| e.to_string())
}

/// 清空全部日志 —— UI 看板"清空"按钮专用,返回删除行数供 toast。
#[tauri::command]
pub fn clear_ai_call_logs(
    db: State<'_, Arc<Db>>,
) -> Result<usize, String> {
    db.ai_call_logs().clear().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsc_core::models::AiCallBusiness;

    #[test]
    fn into_filter_accepts_all_three_businesses() {
        for (raw, expected) in [
            (Some("transform_chapter"), Some(AiCallBusiness::TransformChapter)),
            (Some("test_model"), Some(AiCallBusiness::TestModel)),
            (Some("regenerate_preview"), Some(AiCallBusiness::RegeneratePreview)),
        ] {
            let dto = AiCallLogFilterDto { business: raw.map(|s| s.to_string()), ..Default::default() };
            let f = dto.into_filter().expect("must accept");
            assert_eq!(f.business, expected, "business={:?}", raw);
        }
    }

    #[test]
    fn into_filter_rejects_unknown_business_with_error() {
        let dto = AiCallLogFilterDto { business: Some("nonsense".into()), ..Default::default() };
        assert!(dto.into_filter().is_err());
    }

    /// 负数 offset 被 filter 过滤成 None,list() 内部 unwrap_or(0) —— 与"视为 0"行为一致。
    /// 这里验证中间层不再保留负数,而不是直接断言 Some(0)。
    #[test]
    fn into_filter_drops_negative_offset() {
        let dto = AiCallLogFilterDto { offset: Some(-5), ..Default::default() };
        let f = dto.into_filter().expect("must accept");
        assert_eq!(f.offset, None);
    }
}
