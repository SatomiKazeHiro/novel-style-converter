//! models.dev catalog 管理 —— 三层存储:bundle (内置) / cache (用户目录) / 远端拉取。
//!
//! ## 触发场景
//! - 应用启动 → `load_active_source()` 优先 cache,再退化到 bundle (永远成功,只要打包对)
//! - 用户点 "更新" → `refresh_from_http()` 拉 `https://models.dev/api.json`,成功写 cache
//! - 拉失败 + 用户拖入文件 → `import_from_json()` 把 JSON 字符串写 cache,作为新基线
//!
//! ## 设计原则
//! - 本模块不依赖 tauri —— 资源路径由 src-tauri 层注入(`bundled_path: PathBuf`)
//! - 不解析 catalog 内容(serde_json::Value 透传,具体 schema 留给前端)
//! - 不写 DB / 不动 model_configs(那是用户私有字段,catalog 只读展示 + 新建时填默认)
//! - 不阻塞 hot path —— 所有 IO 都同步直跑,catalog 3.72 MB 解析一次 ~50ms,可接受

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// 远端拉取 URL。用户环境需要 VPN,失败时退到拖拽导入。
pub const REMOTE_URL: &str = "https://models.dev/api.json";

/// HTTP 拉取超时 —— models.dev 全量 ~3.7 MB,30s 给到充足余量。
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// 拉取的 catalog 来源 —— 用来给前端展示"现在用的是哪一份"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSource {
    /// 包内 bundle (内置默认)
    Bundled,
    /// 用户目录 cache
    Cache,
}

impl CatalogSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::Cache => "cache",
        }
    }
}

/// 写 cache 时的来源标签 —— 用户能区分"我是从远端拉的"还是"我从文件拖进来的"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogOrigin {
    /// 从远端 HTTP 拉取
    Http,
    /// 从用户拖入 / 选中的 JSON 文件导入
    Drop,
}

impl CatalogOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Drop => "drop",
        }
    }
}

/// 写到 cache 同目录的 meta 文件 —— 比对 sha / 展示 "拉取于 xx"。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogMeta {
    /// 实际生效的来源(bundled / cache)
    pub source: CatalogSource,
    /// 这次 cache 文件最初是怎么来的(http / drop) —— bundled 没有这个字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<CatalogOrigin>,
    /// UTC RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Catalog 管理器 —— 持 bundle / cache 路径,启动时构造。
///
/// 不持 catalog 内容(serde_json::Value 3.7 MB 没必要常驻内存),
/// 调用方需要时再 `read_active_json()` 现读现解析。
#[derive(Debug, Clone)]
pub struct CatalogStore {
    /// bundle 内置文件的绝对路径(由 src-tauri 注入,基于 `resource_dir`)
    bundled_path: PathBuf,
    /// 用户目录下的 cache 子目录,持有 api.json + api.json.meta.json
    cache_dir: PathBuf,
}

impl CatalogStore {
    pub fn new(bundled_path: PathBuf, cache_dir: PathBuf) -> Self {
        Self { bundled_path, cache_dir }
    }

    fn cache_json(&self) -> PathBuf { self.cache_dir.join("api.json") }
    fn cache_meta(&self) -> PathBuf { self.cache_dir.join("api.json.meta.json") }

    /// 启动时调一次,拿到当前在用的 source。
    /// 优先级:cache (有且 meta 合法) → bundle (永远兜底)。
    pub fn load_active_source(&self) -> CatalogSource {
        if self.cache_json().exists() && self.cache_meta().exists()
            && self.read_cache_meta().is_ok() {
                return CatalogSource::Cache;
            }
        CatalogSource::Bundled
    }

    /// 读当前生效的 catalog JSON 文本 —— `refresh` / `import` 后再调一次就拿到最新。
    pub fn read_active_json(&self, source: CatalogSource) -> Result<String> {
        match source {
            CatalogSource::Cache => std::fs::read_to_string(self.cache_json()).map_err(Error::from),
            CatalogSource::Bundled => std::fs::read_to_string(&self.bundled_path).map_err(|e| {
                Error::NotFound(format!(
                    "bundle 资源缺失({}): {e}", self.bundled_path.display()
                ))
            }),
        }
    }

    /// 读 cache 侧的 meta。bundle 没 meta(meta 只描述 cache 的来源)。
    pub fn read_cache_meta(&self) -> Result<CatalogMeta> {
        let text = std::fs::read_to_string(self.cache_meta())?;
        Ok(serde_json::from_str(&text)?)
    }

    /// HTTP 拉远端 → 写 cache → 返回新 meta。
    /// 失败(超时 / 非 2xx / DNS 错) → 返回 Err,由上层 fallback 到 drop-zone。
    pub async fn refresh_from_http(&self) -> Result<CatalogMeta> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()?;
        let resp = client.get(REMOTE_URL).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Other(format!("HTTP {} from {}", resp.status(), REMOTE_URL)));
        }
        let bytes = resp.bytes().await?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| Error::Other(format!("non-UTF8 body: {e}")))?;
        validate_catalog(text)?;
        self.write_cache(text, CatalogOrigin::Http)
    }

    /// 把用户传入的 JSON 字符串当 cache 写盘。
    pub fn import_from_json(&self, json: &str) -> Result<CatalogMeta> {
        std::fs::create_dir_all(&self.cache_dir)?;
        validate_catalog(json)?;
        self.write_cache(json, CatalogOrigin::Drop)
    }

    fn write_cache(&self, json: &str, origin: CatalogOrigin) -> Result<CatalogMeta> {
        let sha = sha256_hex(json.as_bytes());
        let size = json.len() as u64;
        // 原子写:先写 .tmp 再 rename,避免半截 JSON 污染 cache
        let tmp_json = self.cache_json().with_extension("json.tmp");
        std::fs::write(&tmp_json, json)?;
        std::fs::rename(&tmp_json, self.cache_json())?;

        let meta = CatalogMeta {
            source: CatalogSource::Cache,
            origin: Some(origin),
            fetched_at: Some(chrono::Utc::now().to_rfc3339()),
            sha256: sha,
            size_bytes: size,
        };
        let tmp_meta = self.cache_meta().with_extension("json.meta.json.tmp");
        std::fs::write(&tmp_meta, serde_json::to_string_pretty(&meta)?)?;
        std::fs::rename(&tmp_meta, self.cache_meta())?;
        Ok(meta)
    }
}

/// 编译期内置的 catalog JSON —— 用于启动时一次性解析
// 已知可关思考的模型集合。用户拖入 / 远端拉取 cache 后需要重启应用
// 才能让 close_thinking 集合刷新(transformer / commands 在启动期
// 固定引用本值,不监听 cache 变更 —— trade-off:模型清单低频更新,
// 重启成本低,代码简洁度大幅提升)。
pub const BUNDLED_CATALOG_JSON: &str = include_str!("../../../../src-tauri/resources/models.dev/api.json");

/// 从 catalog JSON 中提取
// 已知可关思考的 model id 集合。判定规则:reasoning_options 含 toggle
// 或 effort 且 values 含 none。转换业务调 AI 时,如果 model_config.model 在
// 该集合内 —— 尽力塞 reasoning_effort:"none";否则不塞(让模型自决)。
// catalog 不可用 / 解析失败时返回空集合,等价于 全部不尝试关思考。
pub fn parse_close_thinking_models(json: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return set,
    };
    let providers = match v.as_object() {
        Some(o) => o,
        None => return set,
    };
    for (_pid, p) in providers {
        let models = match p.get("models").and_then(|m| m.as_object()) {
            Some(m) => m,
            None => continue,
        };
        for (mid, m) in models {
            let opts = match m.get("reasoning_options").and_then(|x| x.as_array()) {
                Some(a) => a,
                None => continue,
            };
            let supports = opts.iter().any(|o| {
                let t = o.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if t == "toggle" {
                    return true;
                }
                if t == "effort" {
                    return o
                        .get("values")
                        .and_then(|x| x.as_array())
                        .map(|a| a.iter().any(|v| v.as_str() == Some("none")))
                        .unwrap_or(false);
                }
                false
            });
            if supports {
                set.insert(mid.clone());
            }
        }
    }
    set
}

/// 校验 catalog JSON 的最低合法性 —— 只检查顶层是 Object(provider → models)。
/// 不深入校验 schema,因为 models.dev 自己可能在加字段;宽松一点省去同步维护。
pub fn validate_catalog(json: &str) -> Result<()> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let obj = v.as_object()
        .ok_or_else(|| Error::Validation("catalog 顶层必须是 Object(provider map)".into()))?;
    if obj.is_empty() {
        return Err(Error::Validation("catalog 是空对象".into()));
    }
    // 抽样校验一个 provider 必须有 `models` 子对象
    for (_k, v) in obj.iter().take(3) {
        if v.get("models").and_then(|m| m.as_object()).is_none() {
            return Err(Error::Validation(
                "至少一个 provider 缺 models 子对象 —— 不像 models.dev 的 catalog".into(),
            ));
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

/// 给前端的 catalog 状态(轻量,不吐 3.7 MB JSON 内容)。
#[derive(Debug, Clone, Serialize)]
pub struct CatalogStatus {
    pub source: CatalogSource,
    /// 当前 cache meta (若 source=cache)
    pub meta: Option<CatalogMeta>,
    pub bundled_size_bytes: u64,
    pub cache_size_bytes: Option<u64>,
}

impl CatalogStatus {
    pub fn collect(store: &CatalogStore) -> Result<Self> {
        let source = store.load_active_source();
        let meta = if source == CatalogSource::Cache {
            store.read_cache_meta().ok()
        } else {
            None
        };
        let bundled_size = fsl_metadata_len(&store.bundled_path);
        let cache_size = if store.cache_json().exists() {
            Some(fsl_metadata_len(&store.cache_json()))
        } else {
            None
        };
        Ok(Self { source, meta, bundled_size_bytes: bundled_size, cache_size_bytes: cache_size })
    }
}

fn fsl_metadata_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("nsc-catalog-test-{}", uniq()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn uniq() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().to_string()
    }

    #[test]
    fn validate_accepts_minimal_valid() {
        let json = r#"{"minimax":{"models":{"MiniMax-M3":{"id":"MiniMax-M3"}}}}"#;
        validate_catalog(json).unwrap();
    }

    #[test]
    fn validate_rejects_non_object() {
        let json = r#"[1,2,3]"#;
        assert!(validate_catalog(json).is_err());
    }

    #[test]
    fn validate_rejects_empty_object() {
        let json = r#"{}"#;
        assert!(validate_catalog(json).is_err());
    }

    #[test]
    fn validate_rejects_provider_without_models() {
        let json = r#"{"foo":{"name":"x"}}"#;
        assert!(validate_catalog(json).is_err());
    }

    #[test]
    fn write_then_read_meta_and_prefer_cache() {
        let dir = tmpdir();
        let bundled = dir.join("bundle.json");
        std::fs::write(&bundled, "BUNDLED").unwrap();
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();

        let store = CatalogStore::new(bundled, cache.clone());
        assert_eq!(store.load_active_source(), CatalogSource::Bundled);

        let payload = r#"{"minimax":{"models":{"MiniMax-M3":{"id":"MiniMax-M3"}}}}"#;
        let meta = store.import_from_json(payload).unwrap();
        assert_eq!(meta.origin, Some(CatalogOrigin::Drop));
        assert_eq!(meta.source, CatalogSource::Cache);
        assert!(store.cache_json().exists());
        assert!(store.cache_meta().exists());

        assert_eq!(store.load_active_source(), CatalogSource::Cache);
        let read_meta = store.read_cache_meta().unwrap();
        assert_eq!(read_meta.sha256, meta.sha256);
        assert_eq!(read_meta.size_bytes, payload.len() as u64);

        let active = store.read_active_json(CatalogSource::Cache).unwrap();
        assert!(active.contains("MiniMax-M3"));
    }

    #[test]
    fn atomic_write_no_tmp_leftover() {
        let dir = tmpdir();
        let bundled = dir.join("bundle.json");
        std::fs::write(&bundled, "BUNDLED").unwrap();
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let store = CatalogStore::new(bundled, cache.clone());

        store.import_from_json(r#"{"minimax":{"models":{"MiniMax-M3":{"id":"MiniMax-M3"}}}}"#).unwrap();
        let second = r#"{"openai":{"models":{"gpt-5":{"id":"gpt-5"}}}}"#;
        store.import_from_json(second).unwrap();
        let read = store.read_active_json(CatalogSource::Cache).unwrap();
        assert_eq!(read, second);
        let leftover = std::fs::read_dir(cache).unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".tmp"));
        assert!(!leftover, "tmp 残留文件存在,原子写失败");
    }
}
