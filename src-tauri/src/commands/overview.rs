use std::sync::Arc;

use nsc_core::db::{Db, OverviewGraph};

/// 单次拉取整个总览页需要的图 + 统计。前端 5s 轮询时只走这条,
/// 避免多次 IPC 往返。
#[tauri::command]
pub fn get_overview_graph(db: tauri::State<'_, Arc<Db>>) -> Result<OverviewGraph, String> {
    db.overview().load_graph().map_err(|e| e.to_string())
}
