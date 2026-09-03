-- Migration 0024: chapter_previews 表(单章节预览草稿)

CREATE TABLE chapter_previews (
  id INTEGER PRIMARY KEY,
  batch_id INTEGER NOT NULL,
  chapter_id INTEGER NOT NULL,
  custom_input TEXT,
  preview_content TEXT,
  tokens_in INTEGER,
  tokens_out INTEGER,
  error TEXT,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (batch_id) REFERENCES batches(id) ON DELETE CASCADE,
  FOREIGN KEY (chapter_id) REFERENCES chapters(id) ON DELETE CASCADE
);
CREATE INDEX idx_chapter_previews_chap ON chapter_previews(batch_id, chapter_id, id DESC);
