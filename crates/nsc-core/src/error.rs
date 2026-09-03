/// nsc-core 统一错误类型。`thiserror` 派生,8 变体 + 1 兜底:
/// - `Db` — rusqlite 错误(连接 / SQL / 约束)
/// - `Io` — 文件 / 路径 / 读写
/// - `Http` — reqwest 错误(网络层)
/// - `Ai` — LLM 调用失败(非 2xx 响应 / 空 choices / 解析失败)
/// - `Splitter` — 章节切分错误
/// - `Validation` — 入参校验失败(用户调用层语义)
/// - `NotFound` — 资源不存在(查询无结果)
/// - `Serde` — JSON 序列化 / 反序列化
/// - `Other` — 兜底,经 `Error::msg(...)` 构造
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]        Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]              Io(#[from] std::io::Error),
    #[error("http error: {0}")]            Http(#[from] reqwest::Error),
    #[error("ai provider: {0}")]           Ai(String),
    #[error("splitting: {0}")]             Splitter(String),
    #[error("validation: {0}")]            Validation(String),
    #[error("not found: {0}")]             NotFound(String),
    #[error("serde: {0}")]                 Serde(#[from] serde_json::Error),
    #[error("{0}")]                        Other(String),
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self { Self::Other(s.into()) }
}

pub type Result<T> = std::result::Result<T, Error>;