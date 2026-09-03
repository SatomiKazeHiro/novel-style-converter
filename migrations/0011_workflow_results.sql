-- Migration 0011:workflow_results / workflow_result_chapters 结果表,
-- 与存量 batch 回填,把陈旧 batch 状态归一为 stopped。
--
-- 设计要点:
-- * workflow_results 1:1 挂在 batches 上(batch_id UNIQUE),batches 删则级联。
-- * workflow_result_chapters 仅记录"该工作流下已被采纳的章节结果"——
--   done 的 task 才把 result_content 拷过去,其它状态保持 NULL。
-- * tc.(batch_id, chapter_id) 加 UNIQUE 索引:一个 batch 内一个 chapter 只允许
--   一行 tc,这与 worker 写结果槽(Task 5)的语义一致。
-- * 历史陈旧 batch 状态(pending/running/paused/completed/terminated/cancelled)
--   在新模型下全部统一为 stopped——运行过的工作流即视为"已停"。后续 Task 8
--   在启动时根据实际情况再决定是否恢复成 pending。

CREATE TABLE IF NOT EXISTS workflow_results (
  id         INTEGER PRIMARY KEY,
  batch_id   INTEGER NOT NULL UNIQUE REFERENCES batches(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_result_chapters (
  id                 INTEGER PRIMARY KEY,
  workflow_result_id INTEGER NOT NULL REFERENCES workflow_results(id) ON DELETE CASCADE,
  chapter_id         INTEGER NOT NULL REFERENCES chapters(id),
  content            TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  UNIQUE(workflow_result_id, chapter_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tc_batch_chapter
  ON transformation_chapters(batch_id, chapter_id)
  WHERE batch_id IS NOT NULL;

INSERT OR IGNORE INTO workflow_results (id, batch_id, created_at)
SELECT id, id, created_at FROM batches;

INSERT OR IGNORE INTO workflow_result_chapters
  (workflow_result_id, chapter_id, content, created_at, updated_at)
SELECT wr.id, tc.chapter_id,
       CASE WHEN tc.status='done' THEN tc.result_content ELSE NULL END,
       COALESCE(tc.completed_at, COALESCE(tc.started_at, wr.created_at)),
       COALESCE(tc.completed_at, COALESCE(tc.started_at, wr.created_at))
  FROM transformation_chapters tc
  JOIN workflow_results wr ON wr.batch_id = tc.batch_id
 WHERE tc.batch_id IS NOT NULL;

UPDATE transformation_chapters SET status='skipped'
 WHERE status='cancelled' AND batch_id IS NOT NULL;

UPDATE batches
   SET status='stopped',
       ended_at = COALESCE(ended_at, started_at, created_at)
 WHERE status IN ('pending','running','paused','completed','terminated','cancelled');
