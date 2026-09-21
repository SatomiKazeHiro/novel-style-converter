# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

## Skill routing

> 历史遗留提醒:本文件曾列过一套 `/office-hours`、`/investigate`、`/review` 之类的
> 斜杠命令路由。经核实**本仓库并不存在这些命令**(既无 `.claude/commands/`,
> 个人 skills 目录里也没有),照它调用只会失败 —— 已删除,不要再照那套走。

实际可用的是环境里注册的 skills。调用前先看当前会话的 skill 目录,以其中的
`description` 为准。与本项目相关的通常是:

- 前端界面/样式改动 → `frontend-design`(产出避免通用 AI 审美)
- Vue 里做 GSAP 动画 → `gsap-frameworks`(Vue/Svelte 生命周期与清理)
- 查资料 / 网页检索 → `anysearch`
- 找 / 装更多 skill → `find-skills`;自己写 skill → `skill-creator`

其余通用排查(查 bug、审查 diff、写计划)没有对应 skill,直接按本文件开头
1–4 条的准则做即可。

## Project: novel-style-converter

Windows desktop app that imports a novel (`.txt`), auto-splits by chapter, then runs LLM-based compression or style-transfer on each chapter via any OpenAI-compatible HTTP API. Stack: **Tauri 2** (Rust backend) + **Vue 3**, packaged as MSI on Windows.

### Build & Run

```bash
# Frontend deps (pnpm 11+; first run needs `pnpm approve-builds` for esbuild + vue-demi)
pnpm install

# Dev: starts Vite (port 43801) + Tauri window
pnpm tauri dev

# Frontend only (no Tauri window)
pnpm dev

# Release MSI bundle → target/release/bundle/msi/
pnpm tauri build --bundles msi

# Release smoke test (4s, GUI-independent)
pwsh scripts/smoke.ps1
```

### Tests

```bash
# Frontend unit tests (vitest, mocks @tauri-apps/api/core)
pnpm test

# Frontend type check —— 前端唯一的静态门禁。
# IPC 的 camelCase(外圈)/ snake_case(内层)翻译写错时,vitest 的 mock 抓不到,
# 只有 typecheck 或真机跑一次能发现。改完 src/ipc/ 或 src/**/*.vue 请务必跑。
pnpm typecheck

# Rust core (unit tests 在 src/ 内嵌 #[cfg(test)])
cargo test -p nsc-core

# 单个集成测试文件
cargo test -p nsc-core --test transformer_ctx
cargo test -p nsc-core --test queue_worker_panic
cargo test -p nsc-core --test ai_openai
```

**测试现状**:`crates/nsc-core/tests/` 下**已无空壳**,只剩 6 个真实集成测试文件;
覆盖主力在 `src/` 内嵌的 `#[cfg(test)]` 模块(lib 单测 273 个,整仓 302 个用例)。

| 文件 | 覆盖 |
|---|---|
| `transformer_ctx.rs` | `read_context` 的邻章切片顺序 |
| `append_chapters.rs` | 追章节到 stopped batch |
| `promotion_word_count.rs` | 转正路径的 word_count |
| `chapters_idx_invariant.rs` | chapters.idx 不变式 |
| `queue_worker_panic.rs` | worker 的 per-job panic 边界 |
| `ai_openai.rs` | `OpenAiProvider`(wiremock):usage 缺失、审核拦截、非 2xx |

约定:**新测试写进被测模块的 `#[cfg(test)] mod tests`**(同文件,能直接摸私有函数),
不要再往 `tests/` 加集成文件。要动 `splitter` / `cleaner` / `batch_scheduler` /
`db::repo` 之前先看该模块有没有 `mod tests`(splitter 23 个、cleaner 18 个已在内嵌)。

> **测"派发"必须断言终态,不能断言中间态。** `scheduler_with_notifier` 之类的夹具挂的是
> **真 worker + 立即返回的 provider**,派发后数据库状态在另一个线程继续演进:断言"刚
> reset 完 `started_at` 是 None"必然偶发失败(实测 15 次挂 1 次),`advance_batch` 还会
> 顺手把下一章也跑掉。用轮询等终态(`batch_scheduler::tests::wait_chapter_status`),
> 别用 `assert_eq!` 去赌时序。

### Architecture (current — post-Phase 11)

- **Cargo workspace** at root: `crates/nsc-core` (pure lib) + `src-tauri` (shell).
  Single pnpm package at root. **不存在 `crates/nsc-desktop/`**(早期阶段的目录已移除)。
- **`crates/nsc-core/src/`** — no Tauri deps. 顶层模块(改前先 `ls` 一次,别照抄本列表):
  - `db/`(`pool`、`migrate`、`repo/` 下 13 个 repo 文件)· `models/`(Novel / Chapter /
    Prompt / ModelConfig / DataAsset / TransformationNovel / TransformationChapter / Batch …)
  - `ai/`(`AiProvider` trait + `OpenAiProvider`;`provider::describe_provider_error`
    负责把 provider 错误翻成可操作提示)
  - `splitter/rules.rs`(zh/en 章节正则 + 空行兜底 + zh-aware word_count)·
    `cleaner/`(清洗规则)· `encoding.rs`(BOM / UTF-8 / GBK / chardetng)
  - `prompts/`(builtin 模板 + `render`;system/user 以独占一行的 `---` 切分)·
    `transformer/`(`JobQueue` worker pool + `DefaultTransformer` + `BatchScheduler` +
    `ratio_guard` 产出比例量测)
  - `recorder/`(AI 调用记账:非阻塞 channel + **自建 OS 线程**,线程内再建 tokio
    current-thread runtime 落 `ai_call_logs`;不用 `tokio::spawn`,因为调用方线程
    可能还没有 reactor —— 见模块头注释)· `catalog/`(模型目录)·
    `sync.rs`(锁中毒恢复 + panic payload 解析,后台线程共用)·
    `startup_cleanup.rs` / `startup_recovery.rs`(启动期自愈)·
    `text.rs` + `text/` · `upload.rs` · `error.rs`(**9** variants)
- **`src-tauri/`** — Tauri 2 shell。`lib.rs`:打开
  `%APPDATA%/novel-style-converter/data.db` → 跑迁移 → `startup_recovery` /
  `startup_cleanup` → `seed_builtin_prompts`(只 seed 内置 prompt 模板;Rust 侧
  **没有**默认 ModelConfig 的 seed 逻辑,模型由用户在 UI 里配)→ 起 `JobQueue`(2 workers)
  + `BatchScheduler` + recorder writer → 注册 70 个命令。
  `commands/` 已拆为 13 个模块:`models` / `uploads` / `chapters` / `cleaning` /
  `data_assets` / `transformation_novels` / `transformations` / `workflows` /
  `prompts` / `ai_call_logs` / `catalog` / `overview` / `util`。
  `.env` 只用于本地开发(`dotenvy` 读入,失败静默忽略)。
- **`src/`** — Vue 3 frontend。`views/`:Library(uploads / data-assets /
  transformations 三个 tab)、Models、Upload、parse(章节向导)、DataAsset、Transform、
  TransformationNovelDetail、Prompts、AiCalls、Overview。`stores/`:library、models、
  chapters、dataAsset、transformView、prompts、workflows、theme。`components/`(含
  `ui/` 基础组件)、`composables/`、`utils/`、`types/`(全局 `.d.ts` shim)。
  IPC 绑定在 `src/ipc/{commands.ts, types.ts}` —— **手写,不是生成的**。
  Router `src/router/index.ts`:`/uploads`、`/data-assets`、`/transformations`、
  `/library/upload/:uploadId`(+ `/parse`)、`/library/data/:dataAssetId`、
  `/library/transform/:chapterId`、`/library/transformation/:tnId`、`/models`、
  `/prompts`、`/ai-calls`、`/overview`。

### Critical invariants

- **`Db` 是 `Send + Sync`,可以自由跨线程共享。**
  `Db` = `Mutex<Connection>`(`db/pool.rs`),拿它的方式一律是 `Arc<Db>`:
  `Db::open()` / `Db::open_in_memory()` 返回 `Arc<Self>`,写操作靠内部的 Mutex 串行化,
  provider worker / scheduler / recorder / 命令层共享**同一个** `Arc<Db>`。
  **不要**再按"每个线程各自 `Db::open(path)`"来设计:
  - ✅ `move || Ok(db.clone())`(克隆同一个 `Arc<Db>`)
  - ❌ `move || Db::open(&db_path)`(多开 Connection,反而把已根治的 SQLITE_BUSY 请回来)
  - ✅ 需要借用底层连接时用 `db.lock()`(返回 `MutexGuard<Connection>`);它走
    `sync::lock_recover`,中毒时恢复而非 panic —— **全应用只有这一条 Connection**,
    这里一旦 `expect` panic,中毒会传染给每个线程的每一次取锁。
- **`JobQueue`** 需要两个工厂:
  - `db_factory() -> Result<Arc<Db>>`(**返回 `Arc<Db>`,不是 owned `Db`**)
  - `provider_factory(&ModelConfig) -> Box<dyn AiProvider>`(必须 owned:不能返回
    `&dyn AiProvider`,否则装不进 `Box<dyn Transformer>`)
- **worker 有 per-job panic 边界**:单个 job panic 会被 `catch_unwind` 隔离(记日志 +
  标 failed + 继续消费下一个 job),不会带走 worker。给 worker 加新的 `await`/调用时
  别把这层边界拆掉(见 `queue.rs` 里的注释)。
- **Schema migrations** 在 `migrations/`(目前到 `0031_ai_call_log_ratio_note.sql`)。
  DDL 保持 `IF NOT EXISTS`;`ALTER TABLE ... ADD COLUMN` **没有** `IF NOT EXISTS`
  (SQLite 不支持),靠 `db/pool.rs::run_schemas` 的 `schema_versions` 表保证只跑一次 ——
  所以**不要**依赖"migration 可以重复执行",而要保证"同一条 migration 只被记录执行一次"。
- **IPC payload convention (Tauri 2)**:
  - **Outer invoke args** 由 Tauri 自动 camelCase(`dataAssetId`、`chapterIds`、
    `promptId`、`modelConfigId`、`ctxPrev*`、`ctxNext*`、`baseUrl`、`apiKey`、`maxTokens`)。
  - **Inner DTOs**(`ModelConfigInput`、`EnqueuePayload` 等)保持 snake_case
    (`base_url`、`api_key`、`max_tokens`)—— 后端显式 `#[serde(rename_all = "snake_case")]`,
    前端**不要** inline 改名。
  - **Response types** 保持 snake_case(对齐 nsc-core 模型字段)。
  - 权威参考见 `src/ipc/commands.ts` 头部注释。
- **API key**:明文存在 `%APPDATA%/novel-style-converter/data.db`。单机用途。
  `.env` 已 gitignore,**绝不提交真实 key**。归档 model 时 `api_key` 会被抹成空串。
- **`JobQueue` workers**:`lib.rs` 起 2 个。**代码里没有"上限 4"的强制**,那只是设计意图。
  `ModelConfig.concurrency` **已实现**并被使用:`provider_cache.rs` 按它建 per-model
  信号量,`queue.rs` 每个 job 取一个 permit 限流(**不是** unused)。
- **失败处置**:worker 不自动重试,失败章节停在 `Failed`,等用户在 UI 手动重跑。
  注意 provider 侧的内容审核拦截(如 MiniMax 的 `422 new_sensitive`)是**确定性**的,
  同一段正文重试必然再失败 —— 见 `ai::provider::describe_provider_error` 给出的提示。

### Common pitfalls

- **改 Rust 源码不要走 PowerShell 文本管线。** `Get-Content` 在中文 Windows 上会把
  UTF-8 文件按 ANSI 读入,把**中文注释变成乱码、并吞掉换行**,`-replace` 批量替换后
  写回就是整文件损坏(实测把两个 repo 文件改到无法编译,只能 `git checkout` 重做)。
  - 改代码用编辑工具(会按 UTF-8 读写),或明确用
    `[System.IO.File]::ReadAllText($p, [Text.Encoding]::UTF8)` +
    `New-Object Text.UTF8Encoding($false)` 写回(无 BOM)。
  - 同理:模板里的引号一律用全角 `“ ”`,ASCII `"` 会截断 Rust 字符串字面量。
  - **已经中招怎么查**:全仓扫 `鈹|鐢|璋冪|瀹归|瀛楁|鍛藉` 这类字符即可定位
    (`Select-String -Path **/*.rs,*.ts -Pattern '鈹|鐢|璋冪'`)。2026-09 就发现
    `src/ipc/types.ts` 88 行 + `src-tauri/src/lib.rs` 6 行中招 —— 注释全废、
    代码仍能编译,所以没人察觉。
  - **还原方法(机械可还原大部分)**:损坏是「UTF-8 字节 → 按 GBK 解码 → 再存 UTF-8」。
    把 mojibake 文本按 GBK **反查回字节**、再按 UTF-8 解码即可,例如
    `瀛楁鍛藉悕绾﹀畾` → `字段命名约定`。Node 下有现成路径(`TextDecoder('gbk')`
    建反查表);但原字节中 GBK 无法表示的部分已被替换成 `?`,那些位置只能按上下文重写。
- **`tauri.conf.json`** 的 `beforeDevCommand` / `beforeBuildCommand` 必须是
  `pnpm dev` / `pnpm build`(从仓库根运行);历史上出现过写死的绝对路径
  (`D:/NewCode/...`)。当前值正确,改配置时别退回绝对路径。
- **`vite.config.ts`** 的 `server.watch.ignored` 排除 `**/target/**`、`**/crates/**`、
  `**/src-tauri/**`、`**/migrations/**`、`**/dist/**` —— 去掉后 cargo 的 rustdoc HTML
  会触发依赖扫描爆炸。端口写死 43801 + `strictPort`。
- **加 IPC 命令**:在 `src-tauri/src/commands/<module>.rs` 实现 → 在
  `src-tauri/src/lib.rs` 的 `invoke_handler!` 注册 → 在 `src/ipc/commands.ts` 加类型化
  wrapper(外圈 camelCase、内层 snake_case)→ 需要时扩 `src/ipc/types.ts` → 跑
  `pnpm typecheck`(它能抓到 camelCase/snake_case 写错,`vitest` mock 抓不到)。
- **加 schema 变更**:新增 `migrations/000N_*.sql`(**永不修改已应用的 migration**)→
  在 `crates/nsc-core/src/db/migrate.rs` 的 `SCHEMAS` 末尾注册(注意 key 与文件名
  历史上存在错位,见该文件内注释)→ `db/repo/` 加 repo 函数 → **若新增了 repo 文件**:
  在 `db/repo/mod.rs` 里 `pub mod` + `pub use` 导出,并在 `db/pool.rs` 的 `Db` 上加
  同名访问器(`pub fn xxx(&self) -> XxxRepo<'_> { XxxRepo { conn: self.lock() } }`)——
  业务层一律通过 `db.xxx()` 拿 repo,不直接构造 → 只有前端需要时才加 Tauri 命令。
  对**已存在的表**做 `ALTER TABLE ADD COLUMN` 时要自己验证"旧库升级"路径(见
  `db/pool.rs::tests::ratio_note_migration_adds_column_to_existing_table` 的写法:
  复制真实库 → 删列删版本记录 → 重开 → 断言)。

