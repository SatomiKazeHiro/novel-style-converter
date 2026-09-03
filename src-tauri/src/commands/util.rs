//! 杂项命令 —— 调外部程序 / 系统浏览器等不归入具体业务模块的命令。
use std::path::Path;

#[tauri::command]
/// 用系统默认浏览器打开外部 URL。Tauri webview 的 `<a target="_blank">` 在 webview 内打开新标签,
/// 不会被引导到外部浏览器 —— 这里走 `open` crate,行为等价于用户在桌面点开链接。
pub fn open_external_url(url: String) -> Result<(), String> {
    if url.is_empty() {
        return Err("url is empty".into());
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(format!("only http/https allowed: {url}"));
    }
    open::that(Path::new(&url)).map_err(|e| e.to_string())
}
