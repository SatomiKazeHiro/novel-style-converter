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

    if let Some(existing_id) = db.uploads().find_by_sha256(&sha)? {
        return db
            .uploads()
            .get(existing_id)?
            .ok_or_else(|| Error::Other("upload row missing".into()));
    }

    std::fs::create_dir_all(dest_dir)?;
    let dest = dest_dir.join(format!("{sha}.txt"));

    std::fs::write(&dest, &bytes)?;

    let word_count = text::word_count(&text) as i64;
    let insert = db.uploads().insert(&NewUpload {
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
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
    };

    db.uploads()
        .get(id)?
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

    #[test]
    fn sha256_of_empty_is_known_value() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}