PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS novels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    author TEXT,
    source_path TEXT,
    imported_at TEXT NOT NULL,
    notes TEXT
);

CREATE TABLE IF NOT EXISTS chapters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    novel_id INTEGER NOT NULL REFERENCES novels(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    title TEXT NOT NULL,
    original_content TEXT NOT NULL,
    word_count INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chapters_novel ON chapters(novel_id, idx);

CREATE TABLE IF NOT EXISTS transformations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    mode TEXT NOT NULL,
    prompt_id INTEGER NOT NULL,
    model_config_id INTEGER NOT NULL,
    ctx_prev_original INTEGER NOT NULL,
    ctx_prev_transformed INTEGER NOT NULL,
    ctx_next_original INTEGER NOT NULL,
    status TEXT NOT NULL,
    result_content TEXT,
    tokens_in INTEGER,
    tokens_out INTEGER,
    error TEXT,
    started_at TEXT,
    completed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_transformations_chapter ON transformations(chapter_id);
CREATE INDEX IF NOT EXISTS idx_transformations_status ON transformations(status);

CREATE TABLE IF NOT EXISTS prompts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    template TEXT NOT NULL,
    is_builtin INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS model_configs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key TEXT NOT NULL,
    model TEXT NOT NULL,
    max_tokens INTEGER,
    temperature REAL,
    concurrency INTEGER NOT NULL DEFAULT 3
);