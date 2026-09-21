//! Upload service: validate, decode, hash, dedup, write, insert with rollback.
//!
//! The Tauri command is a thin adapter; this module owns the business logic
//! so it can be unit-tested without a Tauri runtime.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::encoding::{decode_to_utf8, DecodedText};
use crate::error::{Error, Result};
use crate::models::{NewUpload, Upload};
use crate::text;

/// Hard cap on a single uploaded file's size (bytes).
/// 256 MiB is generous for a novel txt while still bounding memory + IO.
pub const MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// Register a new upload from a user-chosen file path.
///
/// Atomicity model: file write succeeds, then DB insert is attempted; on
/// insert failure the file is removed so we never leave an orphan on disk.
/// On dedup hit, no new file is written.
///
/// * `source`  — user-chosen path to a regular file (validated)
/// * `filename` — display name; trimmed, must be non-empty
/// * `dest_dir` — directory to write `<sha>.txt` into (created if missing)
pub fn upload_file(
    db: &Db,
    source: &Path,
    filename: &str,
    dest_dir: &Path,
) -> Result<Upload> {
    let filename = filename.trim();
    if filename.is_empty() {
        return Err(Error::Validation("文件名不能为空".into()));
    }

    let meta = std::fs::metadata(source)?;
    if !meta.is_file() {
        return Err(Error::Validation(format!(
            "{} 不是文件",
            source.display()
        )));
    }
    let size = meta.len();
    if size == 0 {
        return Err(Error::Validation("文件为空".into()));
    }
    if size > MAX_UPLOAD_BYTES {
        return Err(Error::Validation(format!(
            "文件过大: {size} bytes (上限 {MAX_UPLOAD_BYTES} bytes)"
        )));
    }

    let bytes = std::fs::read(source)?;
    let DecodedText { text, .. } = decode_to_utf8(&bytes)
        .map_err(|e| Error::Validation(format!("解码失败: {e}")))?;

    let sha = sha256_hex(&bytes);

    // 去重命中:直接返回已有行,不写新文件。
    //
    // **repo guard 必须在本 if let 之前就取好并复用**:`MutexGuard` 是不可重入的,
    // 而 `if let Some(x) = db.uploads().find(..)` 里那个临时 guard 会存活到**整个
    // if let 块结束** —— 块内若再调 `db.uploads()` 就会自死锁(永久挂住,不是报错)。
    // 之前正是这个写法,导致**同一文件第二次上传就挂死**。
    let repo = db.uploads();
    if let Some(existing_id) = repo.find_by_sha256(&sha)? {
        return repo
            .get(existing_id)?
            .ok_or_else(|| Error::Other("upload row missing".into()));
    }
    drop(repo);

    std::fs::create_dir_all(dest_dir)?;
    let dest = dest_dir.join(format!("{sha}.txt"));

    std::fs::write(&dest, &bytes)?;

    let word_count = text::word_count(&text) as i64;
    // 同理:整段只取一次 repo,避免 insert 与随后的 get 各自取锁。
    let repo = db.uploads();
    let insert = repo.insert(&NewUpload {
        sha256: sha,
        filename: filename.to_string(),
        byte_size: size as i64,
        file_path: dest.to_string_lossy().to_string(),
        original_text: text,
        word_count,
    });

    let id = match insert {
        Ok(id) => id,
        Err(e) => {
            // 写盘成功但入库失败 → 删掉刚落盘的文件,不留孤儿。
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
    };

    repo.get(id)?
        .ok_or_else(|| Error::Other("upload row missing".into()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "中文" 的 GBK 字节(与 encoding.rs 测试同源,实测确认后固化)。
    const GBK_ZHONGWEN: &[u8] = &[214, 208, 206, 196];

    struct Fixture {
        _dir: tempfile::TempDir,
        dest: std::path::PathBuf,
    }

    /// 建一个临时「源文件 + 目标目录」环境。
    fn fixture(name: &str, bytes: &[u8]) -> (Fixture, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join(name);
        std::fs::write(&src, bytes).unwrap();
        let dest = dir.path().join("uploads");
        (Fixture { _dir: dir, dest }, src)
    }

    fn fresh_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn sha256_of_empty_is_known_value() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// 成功路径:文件落盘为 `<sha>.txt`,DB 行字段齐备,正文是解码后的 UTF-8。
    #[test]
    fn upload_writes_file_and_inserts_row() {
        let db = fresh_db();
        let (fx, src) = fixture("a.txt", "第一章 正文".as_bytes());
        let u = upload_file(&db, &src, "我的小说.txt", &fx.dest).unwrap();

        assert_eq!(u.filename, "我的小说.txt");
        assert_eq!(u.original_text, "第一章 正文");
        assert_eq!(u.word_count, 5); // zh-aware 字数:空格不计 → 第一章正文 = 5
        assert_eq!(u.byte_size, "第一章 正文".len() as i64); // 字节数含空格,与字数不同

        // 落盘文件名 = sha + .txt,内容 = 原始字节
        let sha = sha256_hex("第一章 正文".as_bytes());
        assert_eq!(u.sha256, sha);
        assert_eq!(u.file_path, fx.dest.join(format!("{sha}.txt")).to_string_lossy());
        assert_eq!(std::fs::read(&u.file_path).unwrap(), "第一章 正文".as_bytes());

        // 可从 DB 读回
        let got = db.uploads().get(u.id).unwrap().unwrap();
        assert_eq!(got.sha256, u.sha256);
        assert_eq!(got.word_count, u.word_count);
    }

    /// 目标目录不存在时会被创建(create_dir_all)。
    #[test]
    fn upload_creates_dest_dir() {
        let db = fresh_db();
        let (fx, src) = fixture("a.txt", b"hello");
        let nested = fx.dest.join("a").join("b");
        assert!(!nested.exists());
        let u = upload_file(&db, &src, "f.txt", &nested).unwrap();
        assert!(std::path::Path::new(&u.file_path).exists());
    }

    /// **GBK 文件**:入库的 original_text 是解码后的 UTF-8(不是原始字节),
    /// 且磁盘上仍保留原始字节(只在读全文时现解)。
    #[test]
    fn gbk_source_is_decoded_in_db_but_raw_bytes_kept_on_disk() {
        let db = fresh_db();
        let (fx, src) = fixture("gbk.txt", GBK_ZHONGWEN);
        let u = upload_file(&db, &src, "gbk.txt", &fx.dest).unwrap();
        assert_eq!(u.original_text, "中文", "入库应是解码后的 UTF-8");
        assert_eq!(u.word_count, 2);
        assert_eq!(std::fs::read(&u.file_path).unwrap(), GBK_ZHONGWEN, "磁盘保留原始字节");
    }

    /// 文件名两端空白被 trim。
    #[test]
    fn filename_is_trimmed() {
        let db = fresh_db();
        let (fx, src) = fixture("a.txt", b"x");
        let u = upload_file(&db, &src, "  book.txt  ", &fx.dest).unwrap();
        assert_eq!(u.filename, "book.txt");
    }

    /// 空 / 纯空白文件名被拒(Validation,不是 panic)。
    #[test]
    fn empty_filename_is_rejected() {
        let db = fresh_db();
        let (fx, src) = fixture("a.txt", b"x");
        for name in ["", "   ", "\t\n"] {
            let err = upload_file(&db, &src, name, &fx.dest).unwrap_err();
            assert!(err.to_string().contains("文件名不能为空"), "实际: {err}");
        }
    }

    /// 路径指向目录 → 拒绝。
    #[test]
    fn directory_source_is_rejected() {
        let db = fresh_db();
        let dir = tempfile::tempdir().unwrap();
        let err = upload_file(&db, dir.path(), "d.txt", &dir.path().join("out")).unwrap_err();
        assert!(err.to_string().contains("不是文件"), "实际: {err}");
    }

    /// 空文件被拒。
    #[test]
    fn empty_file_is_rejected() {
        let db = fresh_db();
        let (fx, src) = fixture("empty.txt", b"");
        let err = upload_file(&db, &src, "e.txt", &fx.dest).unwrap_err();
        assert!(err.to_string().contains("文件为空"), "实际: {err}");
    }

    /// 超过 MAX_UPLOAD_BYTES 被拒(用 set_len 造稀疏文件,不真占 256MB 磁盘)。
    #[test]
    fn oversized_file_is_rejected_without_reading_it() {
        let db = fresh_db();
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.txt");
        let f = std::fs::File::create(&src).unwrap();
        f.set_len(MAX_UPLOAD_BYTES + 1).unwrap();
        drop(f);

        let err = upload_file(&db, &src, "big.txt", &dir.path().join("out")).unwrap_err();
        assert!(err.to_string().contains("文件过大"), "实际: {err}");
        assert!(!dir.path().join("out").exists(), "被拒时不该建目标目录");
        assert!(db.uploads().list().unwrap().is_empty(), "被拒时不该入库");
    }

    /// 不存在的源路径 → Io 错误(不是 panic)。
    #[test]
    fn missing_source_errors() {
        let db = fresh_db();
        let dir = tempfile::tempdir().unwrap();
        let err = upload_file(&db, &dir.path().join("nope.txt"), "n.txt", &dir.path().join("o")).unwrap_err();
        assert!(matches!(err, Error::Io(_)), "应是 Io 错误,实际: {err:?}");
    }

    /// **去重**:同一内容第二次上传直接返回已有行,不写第二个文件、不新增行。
    #[test]
    fn duplicate_content_returns_existing_without_new_file() {
        let db = fresh_db();
        let (fx, src1) = fixture("a.txt", b"same content");
        let first = upload_file(&db, &src1, "a.txt", &fx.dest).unwrap();

        // 另建同名内容的文件(不同路径)
        let other_dir = tempfile::tempdir().unwrap();
        let src2 = other_dir.path().join("copy.txt");
        std::fs::write(&src2, b"same content").unwrap();
        let second = upload_file(&db, &src2, "copy.txt", &fx.dest).unwrap();

        assert_eq!(second.id, first.id, "同内容应命中已有 upload");
        assert_eq!(second.filename, "a.txt", "返回的是首次入库的行(文件名不更新)");
        assert_eq!(db.uploads().list().unwrap().len(), 1, "不应新增行");
        // 目标目录里只有一个文件(按 sha 命名,天然不会重复)
        let files: Vec<_> = std::fs::read_dir(&fx.dest).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 1, "去重命中不应写第二个文件: {files:?}");
    }

    /// 不同内容 → 两行两个文件。
    #[test]
    fn different_content_creates_separate_rows() {
        let db = fresh_db();
        let (fx, src1) = fixture("a.txt", b"content A");
        let a = upload_file(&db, &src1, "a.txt", &fx.dest).unwrap();
        let (fx2, src2) = fixture("b.txt", b"content B");
        let b = upload_file(&db, &src2, "b.txt", &fx2.dest).unwrap();
        assert_ne!(a.id, b.id);
        assert_ne!(a.sha256, b.sha256);
        assert_eq!(db.uploads().list().unwrap().len(), 2);
    }

    /// word_count 用解码后的文本算(而不是字节数)—— 中文 1 字符 = 1 字。
    #[test]
    fn word_count_uses_decoded_text() {
        let db = fresh_db();
        let (fx, src) = fixture("a.txt", "一二三".as_bytes());
        let u = upload_file(&db, &src, "a.txt", &fx.dest).unwrap();
        assert_eq!(u.word_count, 3);
        assert_eq!(u.byte_size, 9, "字节数是 UTF-8 长度,与字数不同");
    }
}