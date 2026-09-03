-- uploads.word_count:在 upload_file() 时由 nsc_core::text::word_count(&original_text)
-- 一次算好,避免 list 列表时扫全文字符串(256MB 小说会很卡)。
-- 老 upload 行 word_count = 0;重传后会被填上真实值。

ALTER TABLE uploads ADD COLUMN word_count INTEGER NOT NULL DEFAULT 0;