# 架构

本文是**代码结构的单一事实来源**。改代码后请同步这里。行为准则（怎么改、别踩什么坑）在
仓库根 `CLAUDE.md`；使用说明与数据模型在 `README.md`。

- 最后校对：2026-09-21（对照当时 HEAD 逐项核过：命令数、repo 数、模块清单）
- 技术栈：Tauri 2（Rust 后端）+ Vue 3（前端），MSI 打包，仅 Windows

---

## 1. 分层与依赖方向

```
src/                 Vue 3 前端（视图 / store / 组件）
  │  src/ipc/commands.ts —— 手写 IPC wrapper（不是代码生成）
  ▼  Tauri invoke
src-tauri/           Tauri 2 外壳：启动装配 + 69 个命令
  │  仅这一层依赖 Tauri
  ▼  普通 Rust 调用
crates/nsc-core/     纯库：业务逻辑，无 Tauri 依赖
```

**依赖只能自上而下。** `nsc-core` 里出现任何 Tauri 类型都是设计事故；反过来前端不得绕过
`src/ipc/` 直接拼 invoke 参数。

---

## 2. 数据流

```
.txt 上传 ──► uploads            （原文全文 + sha256 去重，物理文件存档）
                │ 解析（正则分章 + 人工校对）
                ▼
            data_assets + chapters        （一份资产 = 一次解析结果，可被多本转换小说引用）
                │ 起转换小说
                ▼
            transformation_novels
                │ 建批次（把 prompt/model/ctx/mode 固化在批次上）
                ▼
            batches ──► transformation_chapters   （批次内的单章任务）
                            │ worker 执行
                            ▼
                        workflow_results / workflow_result_chapters   （结果集，每章一个内容槽）
                            │
        chapter_previews ───┘  先预览单章、满意后提交（提交即写进结果槽）

转正：结果集 ──► 新的 data_assets（kind=promoted）+ chapters
```

表结构细节见 `README.md` 的「数据模型」；迁移在 `migrations/`（31 个，`0001`–`0031`）。

---

## 3. `nsc-core` 模块地图

| 模块 | 职责 |
|---|---|
| `db/` | `pool.rs`（`Db` + 迁移执行）/ `migrate.rs`（`SCHEMAS` 注册表）/ `repo/`（**12** 个 repo 文件，一表一文件） |
| `models/` | 领域模型 + `New*` 输入结构（`batch` / `chapter` / `prompt` / `model_config` / `data_asset` / `transformation` / `novel` / `workflow_result` / `ai_call_log`） |
| `ai/` | `AiProvider` trait + `OpenAiProvider`；`provider::describe_provider_error` 把 provider 错误翻成可操作提示 |
| `splitter/` | `rules.rs`：中/英章节正则 + 空行兜底 + zh-aware `word_count` |
| `cleaner/` | 文本清洗规则（合并折行 / 补空行 / 缩进 / 折叠空行） |
| `prompts/` | 内置模板 + `render`（system/user 以独占一行的 `---` 切分） |
| `transformer/` | `JobQueue`（worker pool）/ `DefaultTransformer` / `BatchScheduler`（批次状态机）/ `provider_cache`（per-model 并发信号量）/ `ratio_guard`（产出比例量测）/ `job` |
| `recorder/` | AI 调用记账：非阻塞 channel + 自建 OS 线程（见 §5） |
| `catalog/` | 模型目录（bundled JSON + 远端刷新） |
| `encoding.rs` | BOM / UTF-8 / GBK 检测与解码（chardetng 启发式） |
| `sync.rs` | 锁中毒恢复 + panic payload 解析 |
| `startup_recovery.rs` / `startup_cleanup.rs` | 启动期自愈（收口 running/pending、修 idx） |
| `upload.rs` | 上传：读文件 / 解码 / sha256 / 失败回滚 |
| `text.rs` + `text/` | 文本工具（zh-aware 字数） |
| `error.rs` | 统一 `Error`（**9** 变体，含 `Other` 兜底） |

---

## 4. 数据库所有权

`Db` 就是 `Mutex<Connection>`，`Db::open()` / `open_in_memory()` 返回 **`Arc<Db>`**。
全应用**只有这一条连接**，worker / scheduler / recorder / 命令层共享同一个 `Arc<Db>`。

- ✅ `move || Ok(db.clone())` —— 克隆同一个 `Arc<Db>`
- ❌ `move || Db::open(&db_path)` —— 多开连接，会把已根治的 `SQLITE_BUSY` 请回来
- `db.lock()` 返回 `MutexGuard<Connection>`，走 `sync::lock_recover`（中毒恢复而非 panic）

**`db.lock()` 不可重入** —— 一边持有 `db.xxx()` 返回的 repo guard，一边再取锁会**永久挂住**
（不是报错）。guard 活到语句结束，所以别把 `db.yyy()` 写进 `if let db.xxx()…` 的条件里。

---

## 5. 并发模型

```
                   ┌──────────────┐
Tauri 命令 ───────►│  JobQueue    │  2 个 worker（lib.rs 启动值）
                   │  worker pool │  每 job 取一个 per-model permit 限流
                   └──────┬───────┘
                          │ 完成回调（notifier）
                          ▼
                   ┌──────────────┐
                   │BatchScheduler│  批次状态机：派下一章 / 收尾 / 失败策略
                   └──────────────┘

AI 调用 ──► ChannelRecorder（容量 4096，满时 drop，不阻塞 hot path）
                    │ 非阻塞 channel
                    ▼
            spawn_writer：自建 OS 线程 + 线程内 tokio current_thread runtime
                    │
                    ▼
                ai_call_logs
```

两个关键取舍：

- **recorder 用自建 OS 线程而不是 `tokio::spawn`** —— 调用方线程可能还没有 tokio reactor
  （`run()` 是 builder 同步阶段，`.run()` 之前直接 `tokio::spawn` 会 panic
  `there is no reactor running`）。
- **worker 有 per-job panic 边界** —— 单个 job panic 被 `catch_unwind` 隔离（记日志 + 标
  failed + 继续消费下一个 job），不会带走 worker。给 worker 加新 `await` 时别拆掉这层。

`JobQueue` 的两个工厂：

- `db_factory() -> Result<Arc<Db>>`（**返回 `Arc<Db>`**，不是 owned `Db`）
- `provider_factory(&ModelConfig) -> Box<dyn AiProvider>`（必须 owned，否则装不进
  `Box<dyn Transformer>`）

---

## 6. 失败与重试语义

- **worker 不自动重试**：失败章节停在 `failed`，等用户在 UI 手动重跑（避免 token 失控）。
- **内容审核拦截是确定性的**：如 MiniMax 的 `422 new_sensitive`，同一段正文重试必然再失败
  —— 提示语由 `ai::provider::describe_provider_error` 给出。
- **provider 不返回 `usage`**：`tokens_in/out` 落 **NULL**（不是 0），不构成失败。
- **批次收尾写 `stopped`**（不是 `completed`）；`completed` 只存在于 `batches.status` 枚举。

---

## 7. IPC 约定（Tauri 2）

| 位置 | 命名 | 说明 |
|---|---|---|
| 外层 invoke 参数 | **camelCase** | Tauri 自动加 `#[serde(rename_all = "camelCase")]`：`dataAssetId` / `chapterIds` / `ctxPrev*` / `baseUrl` … |
| 内层 DTO | **snake_case** | 后端显式 `#[serde(rename_all = "snake_case")]`：`base_url` / `api_key` / `max_tokens` |
| 响应类型 | **snake_case** | 对齐 `nsc-core` 模型字段，不做 rename |

- IPC 绑定**手写**在 `src/ipc/{commands.ts, types.ts}`，不是代码生成。
- 前端唯一的静态门禁是 `pnpm typecheck` —— `vitest` 的 mock 抓不到 camelCase/snake_case 写错。
- 加命令：`src-tauri/src/commands/<module>.rs` 实现 → `lib.rs` 的 `invoke_handler!` 注册
  → `src/ipc/commands.ts` 加类型化 wrapper → 需要时扩 `src/ipc/types.ts` → 跑 typecheck。

---

## 8. 前端结构

- `views/`：`Library`（uploads / data-assets / transformations 三 tab）、`Upload`、`parse`（章节向导）、
  `DataAsset`、`Transform`、`TransformationNovelDetail`、`Prompts`、`Models`、`AiCalls`、`Overview`
- `stores/`（pinia）：`library`、`models`、`chapters`、`dataAsset`、`transformView`、`prompts`、
  `workflows`、`theme`
- `composables/`：`useCatalog`、`useParseEditor`、`useDynamicTableHeight`、`useTooltip`
- `router/index.ts`：`/uploads`、`/data-assets`、`/transformations`、
  `/library/upload/:uploadId`（+ `/parse`）、`/library/data/:dataAssetId`、
  `/library/transform/:chapterId`、`/library/transformation/:tnId`、`/models`、`/prompts`、
  `/ai-calls`、`/overview`

---

## 9. 启动顺序（`src-tauri/src/lib.rs`）

1. `dotenvy::dotenv()`（本地开发用，失败静默忽略）
2. `create_dir_all(%APPDATA%/novel-style-converter)` → `Db::open(data.db)` → 跑迁移
3. `startup_recovery::run` → `startup_cleanup::run`（自愈）
4. `seed_builtin_prompts`（同步内置 prompt 模板；**没有**默认 ModelConfig 的 seed，模型由用户配）
5. 起 `ChannelRecorder` + `spawn_writer`
6. 起 `JobQueue`（2 worker）+ `BatchScheduler`，把 worker 的 done/failed 回调接给 scheduler
7. 注册 69 个命令

---

## 10. 测试布局

- 覆盖主力在 `src/` 内嵌的 `#[cfg(test)] mod tests`（lib 单测 273 个）
- `crates/nsc-core/tests/` 只剩 6 个真实集成文件：`transformer_ctx` / `append_chapters` /
  `promotion_word_count` / `chapters_idx_invariant` / `queue_worker_panic` / `ai_openai`
- 整仓 `cargo test --workspace` = 302 个用例
- 前端 `pnpm test`（vitest）+ `pnpm typecheck`

**写测试的两个硬约定**：新测试写进被测模块的 `mod tests`；测"派发"只能断言终态
（夹具挂的是真 worker + 立即返回的 provider，断言中间态必然偶发失败）。

---

## 11. 来历（只解释"为什么现在长这样"）

早期是 Rust + [gpui](https://github.com/zed-industries/zed/tree/main/crates/gpui) 桌面壳，前端是
gpui 原生组件；经历 **iced 0.13 → gpui → Tauri 2** 三次迁移，前端换成 Vue 3。这一节只保留
**今天仍能解释现状**的几条，实施流水账不再保留（也没有价值）：

- **为什么用虚拟滚动的章节表**：gpui 时代全量渲染 1623 章节直接卡死，改虚拟滚动后沿用至今。
- **为什么 `src-tauri` 的包名仍叫 `nsc-desktop`**：`crates/nsc-desktop/` 目录已随 Phase 9 移除
  （`crates/` 下现在只有 `nsc-core`），但 package name 没跟着改，`cargo build -p nsc-desktop`
  仍可用。`crates/nsc-desktop/`、`crates/gpui-prototype/` 都**不存在**，看到别去找。
- **为什么 IPC 是手写 wrapper**：曾尝试 spec/specta 反射生成，后改为手写
  `src/ipc/{commands.ts,types.ts}`，换来的是命名约定要人工守（见 §7）。
- **为什么数据模型是四段式**：最初的 `novels` / `transformations` 两张表被
  upload → parse → data_asset → transformation_novel 取代；后来又加了 `batches` 让
  prompt/model/ctx 固化在批次上，并引入结果集与 `chapter_previews` 支持「先预览再提交」。
- **打包踩过的坑**：`@tauri-apps/cli@1.6.3` 硬编码 `--features custom-protocol` 而
  tauri 1.8.3 已移除该 feature，靠空 stub feature 过关；`--bundles nsis` 需联网下载，当时不可用。
