# Novel Style Converter

一个 Windows 桌面应用，用大语言模型把导入的小说按章节做**内容压缩**和**文风转换**。底层使用任意 OpenAI 兼容 HTTP API，桌面壳基于 [Tauri 2.x](https://tauri.app/)（Rust 后端 + WebView 前端），前端用 Vue 3 + Element Plus。早期版本用 Rust + [gpui](https://github.com/zed-industries/zed/tree/main/crates/gpui) + [gpui-component](https://github.com/longbridge/gpui-component)，从 iced 0.13 → gpui → Tauri 三次迁移完成。

主要动机是把"coding plan"中未用完的 token 额度消耗在长文本处理上：核心工作就是把长正文交给 LLM，结构与界面只求最小可用。

---

## 功能概览

- **小说导入**：从 `.txt` 文件导入，自动按章节标题正则切分（支持「第一章」「第 N 回」「Chapter N」「卷 N」等中英文标题，也能按空行兜底分隔）
- **上传链路**：前端通过 `tauri-plugin-dialog` 选择文件路径，后端 `nsc_core::upload` 自行读取、解码（UTF-8/GBK）、SHA-256 去重并写入 `%APPDATA%/novel-style-converter/uploads/<sha>.txt`；DB 插入失败时回滚删除物理文件，避免孤儿文件。单文件上限 256 MiB（`MAX_UPLOAD_BYTES`）
- **手动调整章节**：章节表支持新增、删除、合并相邻、上下移动、重命名
- **Prompt 模板管理**：内置 `compress_default`、`style_default` 两条模板，支持复制内置、新建自定义、模板预览（调用 `prompts::render` 实时看渲染结果）
- **模型配置管理**：支持任意 OpenAI 兼容 base_url + api_key + model，可一键测试连接（实际发起一次 `chat` 调用）
- **按章节批量转换**：勾选章节 → 弹参数对话框（prompt / model / 前文原文 / 前文已转换 / 后文 上下文数） → 入队 → worker pool 并发执行
- **多次转换结果保留**：同一章节可保留多条 `transformation_chapters` 记录，Transform 页用 tab 切换，底部显示 tokens_in / tokens_out / status / error
- **队列状态查看**：1 秒自动刷新，分 Pending / Running / Done / Failed 四组，Failed 项可点 [↺ 重试]（重置状态为 pending 并重新入队）
- **失败不重试**：worker 不会自动重试失败的转换，避免 token 失控；用户手动决定

---

## 技术栈

| 层 | 技术 |
|---|---|
| 语言 | Rust 2021 edition，最低 1.75 |
| UI | Tauri 2.x 桌面壳 + Vue 3.5 + Vite 6 + TypeScript 5.6 + Pinia 2.3 + vue-router 4.6 + Element Plus 2.14（前端在 `src/`） |
| 异步运行时 | `tokio`（`rt` + `rt-multi-thread`） |
| HTTP | `reqwest = "0.12"`（`json` + `rustls-tls`，不依赖 OpenSSL） |
| 数据库 | `rusqlite = "0.31"`（`bundled` + `chrono`，自带 SQLite） |
| 文件对话框 | `rfd = "0.15"`（async） |
| 序列化 | `serde` / `serde_json` |
| 时间 | `chrono` |
| 异步 trait | `async-trait` |
| 正则 | `regex` + `once_cell` |
| 测试 mock | `wiremock = "0.6"`（HTTP mock） |
| 测试临时目录 | `tempfile = "3"` |

---

## 项目结构

```
novel-style-converter/
├─ Cargo.toml                  # workspace 根（members: crates/* + src-tauri）
├─ Cargo.lock
├─ package.json                # pnpm 根清单（前端 + @tauri-apps/cli）
├─ pnpm-lock.yaml
├─ vite.config.ts              # Vue dev/build
├─ tsconfig.json
├─ index.html
├─ migrations/                 # 31 个 SQL，0001_init.sql … 0031；见「数据模型」章节
├─ src/                        # Vue 前端
│  ├─ App.vue
│  ├─ main.ts
│  ├─ views/                   # Library / Models / Upload / parse / DataAsset / Transform
│  ├─ components/              # AppShell + Sidebar + Dialogs + Transform 子组件
│  ├─ stores/                  # pinia: library / models / chapters / dataAsset / transformView / theme
│  ├─ ipc/                     # commands.ts + types.ts（手写 IPC bindings）
│  ├─ router/
│  ├─ composables/
│  └─ __tests__/               # vitest
├─ src-tauri/                  # Tauri 2.x 桌面壳（依赖 nsc-core）
│  ├─ Cargo.toml
│  ├─ build.rs                 # tauri_build::build()
│  ├─ capabilities/            # Tauri 2 capability ACL
│  ├─ gen/                     # Tauri 2 generated bindings（不要手动编辑）
│  ├─ icons/                   # 多尺寸 .ico + png（打包用）
│  ├─ tauri.conf.json
│  └─ src/
│     ├─ main.rs               # nsc_lib::run() 入口
│     ├─ lib.rs                # Db + JobQueue 启动 + 注册 Tauri 命令
│     └─ commands/             # models / uploads / chapters / cleaning / data_assets / transformation_novels / transformations
├─ crates/
│  ├─ nsc-core/                # 纯库，无 Tauri/gpui 依赖
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     ├─ lib.rs
│  │     ├─ error.rs           # 9 变体 Error 枚举
│  │     ├─ models/            # Chapter / Prompt / ModelConfig / DataAsset / TransformationNovel / Batch …
│  │     ├─ db/                # pool + migrate + repo/（12 个 repo 文件）
│  │     ├─ ai/                # AiProvider trait + OpenAiProvider + describe_provider_error
│  │     ├─ splitter/          # DefaultSplitter（正则分章）
│  │     ├─ prompts/           # 内置模板 + render
│  │     ├─ cleaner/           # 文本清洗规则
│  │     ├─ encoding.rs        # BOM / UTF-8 / GBK / chardetng
│  │     ├─ text.rs + text/    # 文本工具（zh-aware word_count）
│  │     ├─ sync.rs            # 锁中毒恢复 + panic payload 解析
│  │     ├─ recorder/          # AI 调用记账（非阻塞 channel + 自建 OS 线程）
│  │     ├─ catalog/           # 模型目录
│  │     ├─ startup_recovery.rs / startup_cleanup.rs   # 启动期自愈
│  │     ├─ upload.rs          # 上传（读文件 / 解码 / sha256 / 回滚）
│  │     └─ transformer/       # Transformer trait + DefaultTransformer + JobQueue + BatchScheduler
└─ docs/
   └─ 章节标题正则表达式.png    # 章节标题正则的视觉参考
```

---

## 数据模型

**14 张表**，由 `migrations/` 下 **31** 个 SQL 文件逐条建起来（`0001_init.sql` … `0031_ai_call_log_note`）。
下表是真实 schema（跑完全部迁移后从 `sqlite_master` 导出），不是设计稿：

| 表 | 角色 |
|---|---|
| `uploads` | State 1：一次上传的原始 .txt（正文全文 + sha256） |
| `data_assets` | State 2：一次章节解析的结果（可被多本转换小说引用） |
| `chapters` | 某份 data_asset 下的章节 |
| `transformation_novels` | 转换目标（同一份 data_asset 可起多本） |
| `batches` | 一次批量转换（prompt / model / ctx / mode 固化在这里） |
| `transformation_chapters` | 批次里的单章任务（挂在 `batch_id` 上） |
| `workflow_results` / `workflow_result_chapters` | 批次的结果集（每章一个内容槽，供预览提交/转正） |
| `chapter_previews` | 单章「先预览再决定」的候选稿（同一章可留多版） |
| `prompts` / `model_configs` | 用户配置 |
| `ai_call_logs` | AI 调用记账（denormalized，无 FK，审计用） |
| `transformations` | **遗留空表**（早期单章转换的残留，已被 batches/tc 取代，无代码读写） |
| `schema_versions` | 迁移记账（每条 migration 只跑一次的凭据） |

**数据流**：上传原文（`uploads`）→ 解析成资产（`data_assets` + `chapters`）→ 起转换小说（`transformation_novels`）
→ 建批次（`batches`）把章节排成任务（`transformation_chapters`）→ 结果写进结果集（`workflow_results`）。

```sql
PRAGMA foreign_keys = ON;

CREATE TABLE uploads (
    id INTEGER PRIMARY KEY,
    sha256 TEXT NOT NULL UNIQUE,       -- 去重键
    filename TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    uploaded_at TEXT NOT NULL,         -- RFC3339
    file_path TEXT NOT NULL,           -- 原始 .txt 存档路径
    original_text TEXT NOT NULL DEFAULT '',  -- 原文全文；章节位置用 title_line 的行号定位
    word_count INTEGER NOT NULL DEFAULT 0    -- zh-aware 字数，upload 时一次算好
);

CREATE TABLE data_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    upload_id INTEGER NOT NULL,        -- 注意：**没有** FK（见下）
    title TEXT NOT NULL,
    parsed_at TEXT NOT NULL,
    source_filename TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT 'source',            -- 'source' | 'promoted'
    source_workflow_id INTEGER REFERENCES batches(id) ON DELETE SET NULL,       -- promoted 时指向产出它的批次
    source_data_asset_id INTEGER REFERENCES data_assets(id) ON DELETE SET NULL, -- promoted 时的上游资产
    note TEXT NOT NULL DEFAULT ''
);

CREATE TABLE chapters (
    id INTEGER PRIMARY KEY,
    data_asset_id INTEGER NOT NULL REFERENCES data_assets(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,              -- 章序；重排时整本 renumber（0-based）
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    word_count INTEGER NOT NULL,
    source_kind TEXT NOT NULL DEFAULT 'original',   -- `original` | 转正来源
    source_chapter_id INTEGER REFERENCES chapters(id) ON DELETE SET NULL,
    edited_at TEXT,                    -- NULL = 从未用户编辑
    title_line INTEGER,                -- 标题在 upload.original_text 里的行号；NULL = 无原文坐标
    UNIQUE(data_asset_id, idx)
);

CREATE TABLE batches (
    id INTEGER PRIMARY KEY,
    transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id) ON DELETE CASCADE,
    label TEXT,
    on_failure_policy TEXT NOT NULL DEFAULT 'pause_and_review',  -- | 'skip_failed'
    status TEXT NOT NULL DEFAULT 'pending',   -- pending|running|stopped|paused|completed|terminated|cancelled
    created_at TEXT NOT NULL,
    started_at TEXT,
    ended_at TEXT,
    -- 同质配置：整个批次共用一套 prompt / model / ctx（stopped 后追章直接读这里）
    prompt_id INTEGER, model_config_id INTEGER, mode TEXT,
    ctx_prev_original INTEGER, ctx_prev_transformed INTEGER,
    ctx_next_original INTEGER, ctx_next_transformed INTEGER
);

CREATE TABLE transformation_chapters (
    id INTEGER PRIMARY KEY,
    transformation_novel_id INTEGER NOT NULL REFERENCES transformation_novels(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    mode TEXT NOT NULL,                -- 'compress' | 'style'
    prompt_id INTEGER NOT NULL,
    model_config_id INTEGER NOT NULL,
    ctx_prev_original INTEGER NOT NULL,
    ctx_prev_transformed INTEGER NOT NULL,
    ctx_next_original INTEGER NOT NULL,
    status TEXT NOT NULL,              -- pending|running|done|failed|skipped|cancelled
    result_content TEXT,               -- 收口到结果集后这里会清空
    tokens_in INTEGER,                 -- NULL = provider 未返回 usage（不是 0）
    tokens_out INTEGER,
    error TEXT,
    started_at TEXT,
    completed_at TEXT,
    batch_id INTEGER REFERENCES batches(id) ON DELETE CASCADE,   -- 批次归属
    style_ref_chapter_id INTEGER REFERENCES chapters(id)         -- style 模式的参考章
);
```

其余表的列（`prompts` / `model_configs` / `workflow_results` / `workflow_result_chapters` /
`chapter_previews` / `ai_call_logs` / `transformations` / `schema_versions`）以 `migrations/` 为准 ——
想确认线上库的真实结构，别照本段背，直接查 `sqlite_master`。

### 几个容易踩的 schema 事实

- **删 `uploads` 不会级联删 `data_assets`。** migration 0015 起 `data_assets.upload_id` 是**软引用**
  （审计式，故意不建 FK），`PRAGMA foreign_key_list(data_assets)` 里没有这一条。级联只存在于
  `data_asset → chapters/transformation_novels`。
- **`chapters` 没有 `byte_start` / `byte_end`**：早期版本用 byte offset 切片，0015 已删除，改用
  `title_line`（在原文里的行号）。
- **`data_assets.locked_at` 不存在**：0004 建过，后续迁移已去掉；"锁定"语义现在靠
  `kind` + `source_workflow_id` 表达。
- **单章状态机不含 `completed`**：批次收尾写的是 `stopped`（不是 `completed`）；`completed` 只出现在
  `batches.status` 的枚举里。
- **`ai_call_logs` 没有任何 FK**，`model_config_id` / `context_id` 都是软引用：被引用的行删了，
  日志仍要能读出"当时调的是哪个 model、哪个端点"。

数据库文件位置：`%APPDATA%/novel-style-converter/data.db`（启动时自动 `create_dir_all`）。

---

## Prompt 模板变量

模板使用 `{{var}}` 占位符，运行时由 `prompts::render` 替换：

| 变量 | 含义 |
|---|---|
| `{{chapter_title}}` | 当前章节标题 |
| `{{chapter_content}}` | 当前章节正文 |
| `{{prev_original}}` | 前文原文（前面 `ctx_prev_original` 章，按阅读顺序拼接） |
| `{{next_original}}` | 后文原文 |
| `{{prev_transformed}}` | 前文已转换结果（前面 `ctx_prev_transformed` 章，参考画风用） |
| `{{novel_title}}` | 小说标题 |
| `{{author}}` | 作者（缺省为空串） |

前文 / 后文中「已转换」与「原文」严格区分：已转换结果只作为画风参考，不会污染原文上下文。

若 `ctx_prev_transformed > 0` 但前面没有已转换章节，渲染为 `(暂无已转换参考)`，保证模板结构稳定。

---

## 架构关键点

模块地图、数据流、并发模型（worker pool / scheduler / recorder）、`Db` 所有权与
「`db.lock()` 不可重入」、IPC 命名约定、启动顺序 —— 全部见
**[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)**。

这里只留读代码前值得知道的三条：

- **worker 不自动重试**：AI 失败标 `failed`、写 `error`，等用户在 UI 手动重跑（避免 token 失控）。
- **provider 不返回 `usage` 不构成失败**：`tokens_in/out` 落 NULL，不另估。
- **`migrations/` 永不修改已应用的迁移**：`ALTER TABLE ADD COLUMN` 没有 `IF NOT EXISTS`，
  靠 `schema_versions` 表保证每条只执行一次。

---

## 快速开始

### 前置

- Rust 1.75+（`rustup default stable`）
- Node 20+ 与 npm（前端构建）
- Windows 10+ / 11（其他平台没测过）

### Per-machine 工具配置

仓库不跟踪、每个开发者各自维护的本地配置文件(全部在 `.gitignore` 里):

| 文件 / 目录 | 用途 | 模板 |
|---|---|---|
| `.env` | nsc-desktop 启动时 `dotenvy` 读取,作为 `model_configs` 表为空时的兜底种子(自动插入一条 ModelConfig) | `.env.example` |
| `.mcp.json` | Claude Code 启动时加载的 MCP server 配置(例如 codegraph) | `.mcp.example.json` |
| `.claude/` | Claude Code 项目级 settings / hooks / agents | — |
| `.codegraph/` | CodeGraph 索引数据(`codegraph init` 生成) | — |
| `.cursor/` | Cursor 编辑器本地配置 | — |

模板文件(`.env.example` / `.mcp.example.json`)已提交,新 contributor 复制后改成本地值即可。

### 编译与运行

项目用经典 Tauri 2.x 布局:`src-tauri/` 是 Rust 后端,`src/` 是 Vue 前端,`package.json` / `vite.config.ts` / `tsconfig.json` / `index.html` 在仓库根。

```bash
# 安装前端依赖（pnpm 11+）
pnpm install

# 开发模式：tauri dev 会自动起 vite dev server，再开 Tauri 窗口
pnpm tauri dev

# 仅前端（无 Tauri 窗口）
pnpm dev

# Release 打包（产物在 target/release/bundle/msi/）
pnpm tauri build --bundles msi
```

> **pnpm 10+ 的 `approve-builds` 机制**:`pnpm install` 第一次会提示
> `[ esbuild, vue-demi ]` 是否允许运行 postinstall。这是 pnpm 默认拒绝运行
> 任意 postinstall 以防供应链攻击的安全机制 —— 必须输入 `y` 或事先在
> `pnpm-workspace.yaml` 的 `allowBuilds` 里列白名单(本仓库已配置)。
> esbuild 用来预编译 native binary,vue-demi 给 Vue 2/3 兼容层打补丁。
> 拒绝会导致 `pnpm dev` / `pnpm tauri dev` 启动失败(找不到 esbuild 可执行)。

首次启动会在 `%APPDATA%/novel-style-converter/` 下创建 `data.db` 并自动 seed 两条内置 prompt。

### 已知 API 风险

- **Tauri 2**:依赖 `tauri = "2"` + `@tauri-apps/api@^2` + `@tauri-apps/cli@^2`。IPC bindings **手写**(`src/ipc/commands.ts` + `src/ipc/types.ts`),不再用 specta / specta-typescript 反射生成;`src-tauri/capabilities/` 与 `src-tauri/gen/` 由 Tauri CLI 自动维护,不要手动改。
- **IPC 入参 camelCase,响应保持 snake_case**:Tauri `#[tauri::command]` 自动给入参加 `#[serde(rename_all = "camelCase")]`,前端必须用 camelCase key 传入(`data_asset_id` → `dataAssetId`、`chapter_id` → `chapterId`、`ctx_prev_original` → `ctxPrevOriginal` 等)。单字字段(`id` / `title` / `name`)不受影响。**内层 DTO**(`ModelConfigInput` / `EnqueuePayload` 等)后端显式 `#[serde(rename_all = "snake_case")]`,前端必须按 snake_case 原样发(`base_url` / `api_key` / `max_tokens` / ...),不要 inline 改名。响应类型**不**走 serde rename,继续 snake_case 以匹配 nsc-core 模型。新增 / 修改 IPC 时:在 `src/ipc/commands.ts` 写 inline 翻译、在 `src/__tests__/` 加断言、最好跑一次 `pnpm tauri dev` 实测一次 — 纯 vitest mock 抓不到这个差异。

### 测试

```bash
# Rust：整仓 302 个用例（nsc-core lib 单测 232 + 8 个集成文件 62）
cargo test --workspace

# 只跑 nsc-core
cargo test -p nsc-core

# 单个集成测试文件（真实文件名，不是模块名）
cargo test -p nsc-core --test splitter_new
cargo test -p nsc-core --test cleaner
cargo test -p nsc-core --test ai_openai
cargo test -p nsc-core --test queue_worker_panic
```

**覆盖主力在 `src/` 内嵌的 `#[cfg(test)] mod tests`**，不在 `tests/` 里：`crates/nsc-core/tests/`
目前只有下面 8 个真实集成文件（历史上那批 `#[ignore]` 空壳已全部删除）。同一模块的用例优先写在
被测文件内，能直接访问私有函数。

集成测试文件及其覆盖点：

| 测试文件 | 用例 | 覆盖点 |
|---|---|---|
| `splitter_new.rs` | 23 | 中文章节、回目标题、空行兜底、zh-aware word_count |
| `cleaner.rs` | 18 | 清洗规则（硬折行合并 / 缩进 / 不可见字节归一） |
| `ai_openai.rs` | 9 | wiremock：200 正常解析、usage 缺失 → NULL、审核拦截 422、非 2xx |
| `append_chapters.rs` | 4 | 往已 stopped 的 batch 追章节 |
| `chapters_idx_invariant.rs` | 3 | `chapters.idx` 紧致不变式 |
| `transformer_ctx.rs` | 3 | `read_context` 邻章切片顺序 |
| `promotion_word_count.rs` | 1 | 转正路径的 word_count |
| `queue_worker_panic.rs` | 1 | worker 的 per-job panic 边界（单章 panic 不带走 worker） |

lib 单测按模块分布（`cargo test -p nsc-core --lib -- --list`）：`db::repo` 106、`transformer`
41、`upload` 20、`prompts` 19、`encoding` 11、`batch_scheduler` 8、`provider_cache` 8、`queue` 7、
`startup_recovery` 6、`startup_cleanup` 6、`sync` 4，其余为 models / catalog / text 等。

> **测"派发"只能断言终态。** 调度器测试挂的是真 worker + 立即返回的 provider，派发后数据库状态
> 在另一个线程继续演进；断言中间态（如"刚 reset 完 `started_at` 是 None"）会偶发失败。用轮询等
> 终态（见 `batch_scheduler::tests::wait_chapter_status`），别用 `assert_eq!` 赌时序。

前端测试在 `src/__tests__/`,用 vitest + `vi.mock('@tauri-apps/api/core')` 隔离 IPC(Tauri 2 的 invoke 入口从 `tauri` 改 `core`)。
```bash
pnpm test       # 或 npx vitest run
```

### 冒烟测试(GUI 不阻塞)

跑 release build 验证 main 启动路径不 panic（无显示器 / CI 也能用）：

```bash
# 先产出 release 二进制（smoke.ps1 检查的是 target/release/nsc-desktop.exe）
pnpm tauri build --bundles msi

# 跑 4s 验证不 panic（GNU `timeout` 在 Windows 不可用，PowerShell 替代）
pwsh scripts/smoke.ps1
# 或者: powershell -ExecutionPolicy Bypass -File scripts/smoke.ps1
```

成功：`OK: app launched and ran 4s without panic`，exit 0。
失败：`FAIL: app exited with code N before 4s` 并打 stderr 末尾 20 行。

---

## 使用流程

### 1. 准备 ModelConfig

切到 `🔑 模型` 页 → 「➕ 新增模型」：

- **name**：任意（如 `deepseek` / `gpt-4o`）
- **base_url**：OpenAI 兼容 endpoint（DeepSeek：`https://api.deepseek.com`；本地 Ollama：`http://localhost:11434/v1`；任意代理网关）
- **api_key**：你的 key
- **model**：模型名（如 `deepseek-chat`、`gpt-4o-mini`）
- **max_tokens** / **temperature**：可选
- **concurrency**：per-model 并发上限，**已生效**——`provider_cache` 按 `model_config_id` 建共享信号量，
  每个 job 取一个 permit 限流

填完点 [💾 保存] → [🔌 测试连接] 确认能 ping 通。

### 2. 上传原文

切到 `📂 文件上传` 页(Library uploads tab)→「📥 上传 .txt」：

- 通过 `tauri-plugin-dialog` 选 `.txt` 路径（前端不发字节,后端自读）
- 自动算 sha256 去重；同 sha256 不会重复入库,直接复用已有 upload 行
- 写入 `%APPDATA%/novel-style-converter/uploads/<sha>.txt`；DB 插入失败时自动回滚删除物理文件
- 单文件硬上限 256 MiB（`MAX_UPLOAD_BYTES`）
- 应用读全文存档到 `uploads.original_text`,跳到 `/library/upload/:uploadId` 页

### 3. 文本清洗(可选,Upload 页操作)

- 左栏原文,右栏清洗后;勾选规则后点 [▶ 清洗] 看效果
- 规则只改"行尾无标点的硬折行接上 + 不可见字节归一",**不擅自动缩进或换行**
- 不勾任何规则点 [下一步 →] 等同跳过,直接进章节解析

详见 [§ 清洗规则](#附录清洗规则)。

### 4. 解析章节(parse wizard)

点 [下一步 →] 或「解析章节」按钮:

- 用 `DefaultSplitter` 自动切分章节(中英文标题正则 + 空行兜底)
- 章节可重命名 / 合并相邻 / 调整顺序 / 删除
- 点 [💾 保存为数据资产] 提交 → 生成 `data_assets` 行(默认 unlocked,可后续重解析)

### 5. 自定义 Prompt(可选)

切到 `🔑 模型` 页旁的 Prompt 管理(`models` 是配置,Prompt 编辑在对应弹窗 / 代码路径中):

- 复制内置:点内置行的 [复制内置] → 改名 → 编辑 template → [💾 保存]
- 全新建:列表底部 [➕ 新建]
- 预览:右侧编辑器 → 选章节 → [🔍 预览渲染] 看实时渲染结果

模板变量见上节。

### 6. 触发转换

DataAsset 页 → 顶部 [⚙ 新建转换小说] → 给转换小说起名 → 章节表勾选目标章节 → [⚙ 批量转换]:

- 选 Prompt(kind 与转换模式一致:compress / style)
- 选 Model
- 设三个上下文数(前文原文 / 前文已转换 / 后文)
- [提交]

每个章节插入一条 `transformation_chapters(pending)`,立即 `JobQueue.enqueue(JobSpec)`。

### 7. 查看转换结果

DataAsset 页 → 选某个 transformation_novel → 章节行点 `[▶ 转换结果]`(跳转 `/library/transform/:chapterId`):

- 顶部 ◀ ▶ 翻该 transformation_novel 的所有章节
- 版本 tab:同一章节的多次转换结果(同一 transformation_novel 下,按 id desc)
- 主区:左右栏对照(原文 / 选中 transformation_chapter 的 `result_content`)
- 同步滚动:左右栏 scroll 互锁 50ms 防回环
- 失败 tab 选中:右栏 alert 显示 `transformation_chapter.error`
- 底部:tokens in/out + status + 重新转换(弹 TransformDialog 选 tn + prompt + model + ctx)

---

## 非功能约束

- **平台**：仅 Windows 10+ / 11
- **存储**：单 SQLite 文件，本机位置 `%APPDATA%/novel-style-converter/data.db`
- **API key**：明文存数据库（用户机器本地，无服务器）
- **并发**：全局一个 worker pool，2 个 worker（`src-tauri/src/lib.rs`；代码里**没有**"上限 4"的强制）。
  `ModelConfig.concurrency` 是 **per-model** 并发上限且**已生效**：`provider_cache` 按 `model_config_id`
  建共享信号量，每个 job 取一个 permit 限流（`concurrency <= 0` 会被当作 1）
- **级联删除**：SQLite 外键启用（`PRAGMA foreign_keys = ON`）。级联链是
  `data_asset → chapters / transformation_novels`、`transformation_novel → batches → transformation_chapters`、
  `batch → workflow_results → workflow_result_chapters`。
  **删 upload 不级联**（0015 起 `data_assets.upload_id` 是软引用）：只删 `uploads` 行 + 物理文件，
  已解析出的 data_asset / chapters 会留下 —— UI 删除前先用 `preview_upload_deletion` 告诉用户会留下哪些
- **响应延迟**：UI 不被 IO/网络阻塞（DB 与 HTTP 都跑在 tokio runtime）

---

## 范围外

明确不做（避免需求蔓延）：

- 多用户 / 多人协作 / 云同步
- 重新算 token 计费（依赖 provider 返回 usage）
- 自动重试失败任务（避免 token 失控）
- 多平台打包（仅 Windows）
- Ollama 等非 OpenAI 协议（接口留口子但不实现）
- PDF / EPUB 导入导出（仅 `.txt`）

---

## 设计文档

当前架构（模块地图、数据流、并发模型、IPC 约定、启动顺序）见 **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)**。
历史设计 spec 与实施 plan 未随仓库保留。

---

## 附录:清洗规则

| 规则 | 行为 | 默认 |
|---|---|---|
| `normalize_crlf` | `\r\n` / `\r` → `\n` | 建议开(不可见) |
| `strip_bom` | 去 UTF-8 BOM | 建议开(不可见) |
| `merge_paragraphs` | 行尾无标点 → 接下行;有标点 / 空行 / 下行 `　　` 开头 → 换行原样保留 | 建议开 |
| `add_indent_to_unindented` | 给没有 `　　` 的行补缩进 | 按需 |
| `first_line_is_title` | 配合 `add_indent` 时首行不补 | 按需 |
| `collapse_blank_runs` | ≥3 连续 `\n` 收成 2 个 | 按需 |

**核心约束:任何规则都不能擅自改变原文的换行/缩进结构。** 合并只动"行尾无标点"这一种被硬折断的情形,加缩进是独立规则且仅在显式勾选时生效。

---

## License

MIT