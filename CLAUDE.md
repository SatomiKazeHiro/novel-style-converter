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

When the user's request matches an available skill, invoke it via the Skill tool. When in doubt, invoke the skill.

Key routing rules:
- Product ideas/brainstorming → invoke /office-hours
- Strategy/scope → invoke /plan-ceo-review
- Architecture → invoke /plan-eng-review
- Design system/plan review → invoke /design-consultation or /plan-design-review
- Full review pipeline → invoke /autoplan
- Bugs/errors → invoke /investigate
- QA/testing site behavior → invoke /qa or /qa-only
- Code review/diff check → invoke /review
- Visual polish → invoke /design-review
- Ship/deploy/PR → invoke /ship or /land-and-deploy
- Save progress → invoke /context-save
- Resume context → invoke /context-restore
- Author a backlog-ready spec/issue → invoke /spec

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

# Rust core: db/repos, splitter, prompts, ai_openai (wiremock), transformer, queue
cargo test -p nsc-core

# Single test file
cargo test -p nsc-core --test splitter
cargo test -p nsc-core --test queue
cargo test -p nsc-core --test ai_openai
```

### Architecture (current — post-Phase 11)

- **Cargo workspace** at root: `crates/nsc-core` (pure lib) + `src-tauri` (shell). Single pnpm package at root.
- **`crates/nsc-core/src/`** — no Tauri deps. Modules:
  - `db/` (`pool`, `migrate`, 6 repos in `repo/`) · `models/` (Novel/Chapter/Prompt/ModelConfig/DataAsset/TransformationNovel/TransformationChapter)
  - `ai/` (`AiProvider` trait + `OpenAiProvider` only) · `splitter/rules.rs` (zh/en chapter regex + blank-line fallback + zh-aware word_count)
  - `prompts/` (builtin templates + `render` / `render_raw`) · `transformer/` (`JobQueue` worker pool + `DefaultTransformer`)
  - `cleaner/` (清洗规则; see README) · `encoding/` (BOM/UTF-8/GBK/chardetng) · `text/` · `error.rs` (8 variants)
- **`src-tauri/`** — Tauri 2 shell. `lib.rs` opens `%APPDATA%/novel-style-converter/data.db`, starts `JobQueue` (2 workers), seeds default `ModelConfig` from `.env`, then registers all commands. `commands/` modules: `models`, `uploads`, `chapters`, `cleaning`, `data_assets`, `transformation_novels`, `transformations`.
- **`src/`** — Vue 3 frontend. Views: `Library` (uploads / data-assets / transformations tabs), `Models`, `Upload`, `parse` (chapter wizard), `DataAsset`, `Transform`. Stores: `library`, `models`, `chapters`, `dataAsset`, `transformView`, `theme`. Components in `src/components/`: dialogs, `Sidebar`, transform sub-components. IPC bindings live in `src/ipc/{commands.ts, types.ts}` — **hand-written, not generated**. Router: `src/router/index.ts` (`/uploads`, `/data-assets`, `/library/upload/:id`, `/library/upload/:id/parse`, `/library/data/:id`, `/library/transform/:chapterId`, `/models`).

### Critical invariants

- **`Db` is `Send` but NOT `Sync`** (rusqlite `Connection` has internal `RefCell`).
  - **Never** capture `Arc<Db>` into `tokio::spawn` / `spawn_blocking` closures or `Task::perform` futures.
  - **Always** capture `db_path: PathBuf` and call `Db::open(&path)` inside the worker to get an owned `Db`.
- **`JobQueue`** requires two factories: `db_factory()` returning owned `Result<Db>`, and `provider_factory(&ModelConfig)` returning owned `Box<dyn AiProvider>` (NOT a reference — `DefaultTransformer` owns the provider so it fits in `Box<dyn Transformer>`).
- **Schema migrations** in `migrations/` (`0001_init.sql` … `0006_transformation_novels_data_asset_fk.sql`). All `CREATE TABLE` / `CREATE INDEX` use `IF NOT EXISTS` because worker factories reopen the same DB file repeatedly — must stay idempotent.
- **IPC payload convention (Tauri 2)**:
  - **Outer invoke args** are camelCased automatically by Tauri (e.g. `dataAssetId`, `chapterIds`, `promptId`, `modelConfigId`, `ctxPrev*`, `ctxNext*`, `baseUrl`, `apiKey`, `maxTokens`).
  - **Inner DTOs** (e.g. `ModelConfigInput`, `EnqueuePayload`) keep snake_case fields (`base_url`, `api_key`, `max_tokens`, etc.) — backend uses explicit `#[serde(rename_all = "snake_case")]`. Frontend must NOT inline-rename these.
  - **Response types** keep snake_case (match nsc-core model fields).
  - See header comment of `src/ipc/commands.ts` for the canonical reference.
- **API key**: plaintext in SQLite at `%APPDATA%/novel-style-converter/data.db`. Single-machine use. `.env` is gitignored; never commit real keys.
- **JobQueue workers**: 2 default (lib.rs), 4 max. `ModelConfig.concurrency` field exists but unused — reserved for future per-model throttling.
- **Failure handling**: worker does NOT auto-retry. Failed jobs stay `Failed` until user manually re-enqueues.

### Common pitfalls

- **`crates/nsc-desktop/` is an empty directory** (legacy from Phase 7-9). Don't add code here — the live shell is `src-tauri/`.
- **`tauri.conf.json`** historically had wrong absolute paths (`D:/NewCode/...`) in `beforeDevCommand` / `beforeBuildCommand`. Must be `pnpm dev` / `pnpm build` (run from repo root).
- **`vite.config.ts`** excludes `**/target/**`, `**/crates/**`, `**/src-tauri/**`, `**/migrations/**` from Vite watch — without this, cargo's rustdoc HTML triggers dep-scan explosion.
- **Adding a new IPC command**: implement in `src-tauri/src/commands/<module>.rs`, register in `src-tauri/src/lib.rs` `invoke_handler!`, add typed wrapper to `src/ipc/commands.ts` (camelCase outer args, snake_case inner DTO), extend `src/ipc/types.ts` if needed, then write a `vitest` mock asserting the exact `invoke` call shape — the camelCase translation is easy to break silently (mocked IPC won't catch it).
- **Adding a schema change**: bump `migrations/000N_*.sql` (never edit applied migrations); all DDL must remain `IF NOT EXISTS`; add a corresponding repo function in `crates/nsc-core/src/db/repo/`; export from `crates/nsc-core/src/db/mod.rs`; surface via a Tauri command only if frontend needs it.

