//! Tauri 命令层 —— 把 `nsc_core::catalog` 包成前端 invoke 接口。
//!
//! 资源定位:
//! - bundle 路径 = `<resource_dir>/models.dev/api.json`(在 `tauri.conf.json::bundle.resources` 声明)
//! - cache 路径 = `<APPDATA>/novel-style-converter/cache/models.dev/`
//!
//! 启动流程(`lib.rs`):
//! 1. 解析 resource_dir → 拼出 bundle 路径
//! 2. 解析 APPDATA → 拼出 cache 目录
//! 3. 构造 `CatalogStore` → `app.manage(store)`

use std::path::PathBuf;
use std::sync::Arc;

use nsc_core::catalog::{CatalogMeta, CatalogStatus, CatalogStore};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

/// 在 `lib.rs` 启动时构造的全局 Catalog 句柄。
pub type SharedCatalog = Arc<CatalogStore>;

/// 构造 cache 路径 —— `<APPDATA>/novel-style-converter/cache/models.dev/`。
pub fn default_cache_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base)
        .join("novel-style-converter")
        .join("cache")
        .join("models.dev")
}

/// 从 `tauri.conf.json::bundle.resources` 的声明出发,定位 bundle 文件。
///
/// `resource_dir` 在 dev / release 路径不同(dev 是 `<project>/src-tauri/target/debug`
/// 旁边那个 `resources` 目录,release 跟平台 installer 走的实际位置),
/// 但 `app.path().resource_dir()` 都给到正确的根,我们只要再 join 声明的子路径。
pub fn resolve_bundled_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().resource_dir().map_err(|e| format!("resource_dir: {e}"))?;
    Ok(dir.join("models.dev").join("api.json"))
}

/// 启动时构造 `CatalogStore`。bundle 文件不存在时仍返回实例 —— refresh 会失败但
/// `load_active_source` 会退化到 Bundled 路径并报错 NotFound,前端可显示。
pub fn build_store(app: &AppHandle) -> Result<Arc<CatalogStore>, String> {
    let bundled = resolve_bundled_path(app)?;
    let cache_dir = default_cache_dir();
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("create cache dir: {e}"))?;
    Ok(Arc::new(CatalogStore::new(bundled, cache_dir)))
}

#[derive(Debug, Serialize)]
pub struct RefreshResult {
    pub ok: bool,
    pub source: String,
    pub meta: Option<CatalogMeta>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub ok: bool,
    pub meta: Option<CatalogMeta>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn catalog_status(catalog: State<'_, SharedCatalog>) -> Result<CatalogStatus, String> {
    CatalogStatus::collect(catalog.inner()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn catalog_refresh(catalog: State<'_, SharedCatalog>) -> Result<RefreshResult, String> {
    let store = catalog.inner().clone();
    let res = store.refresh_from_http().await;
    match res {
        Ok(meta) => Ok(RefreshResult {
            ok: true,
            source: "cache".into(),
            meta: Some(meta),
            error: None,
        }),
        Err(e) => Ok(RefreshResult {
            ok: false,
            source: "bundled".into(),
            meta: None,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub fn catalog_import_drop(
    catalog: State<'_, SharedCatalog>,
    json_content: String,
) -> Result<ImportResult, String> {
    let store = catalog.inner();
    match store.import_from_json(&json_content) {
        Ok(meta) => Ok(ImportResult { ok: true, meta: Some(meta), error: None }),
        Err(e) => Ok(ImportResult { ok: false, meta: None, error: Some(e.to_string()) }),
    }
}

/// 返回当前生效的 catalog JSON 字符串(全文)。
///
/// 注意 3.7 MB —— 前端只在初始化 / 用户手动刷新时拉,不要高频调。
/// 后端不缓存内容,直接读盘,让前端拿到最新。
#[tauri::command]
pub fn catalog_read_active(
    catalog: State<'_, SharedCatalog>,
) -> Result<String, String> {
    let source = catalog.inner().load_active_source();
    catalog.inner().read_active_json(source).map_err(|e| e.to_string())
}
