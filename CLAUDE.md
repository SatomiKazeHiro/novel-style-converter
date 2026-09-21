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
- "Fix the bug" → "Write a test that reproduces it, then fix it"
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
> 斜杠命令路由。经核实**本仓库并不存在这些命令**,照它调用只会失败 —— 已删除,不要再照那套走。

实际可用的是环境里注册的 skills。调用前先看当前会话的 skill 目录,以其中的
`description` 为准。与本项目相关的通常是:

- 前端界面/样式改动 → `frontend-design`(产出避免通用 AI 审美)
- Vue 里做 GSAP 动画 → `gsap-frameworks`(Vue/Svelte 生命周期与清理)
- 查资料 / 网页检索 → `anysearch`
- 找 / 装更多 skill → `find-skills`;自己写 skill → `skill-creator`

其余通用排查(查 bug、审查 diff、写计划)没有对应 skill,直接按本文件开头
1–4 条的准则做即可。

---

# Project: novel-style-converter

Windows 桌面应用:导入小说 `.txt` → 自动分章 → 用任意 OpenAI 兼容 HTTP API 做
**内容压缩**或**文风转换**。栈:**Tauri 2**(Rust 后端)+ **Vue 3**,打包成 MSI。

**文档分工(改代码后同步对应那份,别让三份互相打架):**

| 文档 | 内容 |
|---|---|
| `docs/ARCHITECTURE.md` | **架构的单一事实来源** —— 模块地图、数据流、并发模型、IPC 约定、启动顺序 |
| `README.md` | 使用说明 + 数据模型(14 张表 / 31 个迁移) |
| 本文件 | 行为准则 + 本项目特有的坑 |

## Build & Run

```bash
pnpm install                 # 前端依赖(pnpm 11+;首次需 pnpm approve-builds 放行 esbuild + vue-demi)
pnpm tauri dev               # Vite(43801) + Tauri 窗口
pnpm dev                     # 仅前端
pnpm tauri build --bundles msi   # 产物在 target/release/bundle/msi/
pwsh scripts/smoke.ps1       # release 冒烟(4s,GUI 不阻塞)
```

## Tests

```bash
pnpm test        # vitest(前端单测)
pnpm typecheck   # 前端唯一的静态门禁 —— 改完 src/ipc/ 或 src/**/*.vue 务必跑
cargo test -p nsc-core
cargo test --workspace       # 整仓 302 个用例
cargo test -p nsc-core --test ai_openai   # 单个集成文件
```

- **覆盖主力在 `src/` 内嵌的 `#[cfg(test)] mod tests`**(lib 单测 273 个)。
  `crates/nsc-core/tests/` 只剩 6 个真实集成文件,**已无空壳**。
- **新测试写进被测模块的 `mod tests`**,不要再往 `tests/` 加集成文件。
- **测"派发"只能断言终态。** `scheduler_with_notifier` 之类的夹具挂的是**真 worker +
  立即返回的 provider**,派发后数据库状态在另一个线程继续演进:断言"刚 reset 完
  `started_at` 是 None"必然偶发失败(实测 15 次挂 1 次),`advance_batch` 还会顺手把
  下一章也跑掉。用轮询等终态(`batch_scheduler::tests::wait_chapter_status`)。
- **不为凑覆盖率写无意义断言。** 断言"模块导出了函数"这种事由 `tsc` / 编译器免费保证,
  别用测试再包一层。

## 本项目特有的坑

- **改 Rust/TS 源码不要走 PowerShell 文本管线。** `Get-Content` 在中文 Windows 上按 ANSI
  读入 UTF-8,把**中文注释变乱码并吞行**,`-replace` 后写回就是整文件损坏。
  - 改代码用编辑工具(按 UTF-8 读写),或明确用
    `[System.IO.File]::ReadAllText($p, [Text.Encoding]::UTF8)` +
    `New-Object Text.UTF8Encoding($false)` 写回(无 BOM)。
  - 同理:模板里的引号一律用全角 `“ ”`,ASCII `"` 会截断 Rust 字符串字面量。
  - **已中招怎么查**:扫 `鈹|鐢|璋冪|瀹归|瀛楁|鍛藉` 这类字符。2026-09 就发现
    `src/ipc/types.ts` 88 行 + `src-tauri/src/lib.rs` 6 行中招 —— 注释全废、代码仍能编译,
    所以长时间没人察觉。
  - **还原方法(机械可还原大部分)**:损坏 = 「UTF-8 字节 → 按 GBK 解码 → 再存 UTF-8」。
    把文本按 GBK **反查回字节**再按 UTF-8 解即可(`瀛楁鍛藉悕绾﹀畾` → `字段命名约定`);
    GBK 表示不了的位置已被换成 `?`,只能按上下文重写。
- **`db.lock()` 不可重入。** 持有 `db.xxx()` 的 repo guard 时再取锁会**永久挂住**(不是报错)。
  guard 活到语句结束,所以别把 `db.yyy()` 写进 `if let db.xxx()…` 的条件里。
- **加 schema 变更**:新增 `migrations/000N_*.sql`(**永不修改已应用的 migration**)→
  在 `crates/nsc-core/src/db/migrate.rs` 的 `SCHEMAS` 末尾注册(注意 key 与文件名历史上
  存在错位,见该文件内注释)→ `db/repo/` 加 repo 函数 → **若新增 repo 文件**:在
  `db/repo/mod.rs` 导出,并在 `db/pool.rs` 的 `Db` 上加同名访问器
  (`pub fn xxx(&self) -> XxxRepo<'_> { XxxRepo { conn: self.lock() } }`)——业务层一律
  通过 `db.xxx()` 拿 repo,不直接构造。对**已存在的表**做 `ALTER TABLE ADD COLUMN` 时
  要自己验证"旧库升级"路径(写法见 `db/pool.rs::tests::ratio_note_migration_...`:
  复制真实库 → 删列删版本记录 → 重开 → 断言)。
- **加 IPC 命令**:实现 → `src-tauri/src/lib.rs` 的 `invoke_handler!` 注册 →
  `src/ipc/commands.ts` 加类型化 wrapper(外圈 camelCase、内层 snake_case)→ 需要时扩
  `src/ipc/types.ts` → 跑 `pnpm typecheck`(vitest 的 mock 抓不到命名写错)。
- **`tauri.conf.json`** 的 `beforeDevCommand` / `beforeBuildCommand` 必须是
  `pnpm dev` / `pnpm build`(从仓库根运行);历史上出现过写死的绝对路径(`D:/NewCode/...`)。
- **`vite.config.ts`** 的 `server.watch.ignored` 排除 `**/target/**`、`**/crates/**`、
  `**/src-tauri/**`、`**/migrations/**`、`**/dist/**` —— 去掉后 cargo 的 rustdoc HTML
  会触发依赖扫描爆炸。端口写死 43801 + `strictPort`。
- **不要用 `window.confirm` / `window.alert`** —— `tauri-plugin-dialog` 的注入脚本
  (`init-iife.js`) 会把它们改写成 `plugin:dialog|confirm` / `|message`，但该插件**只注册了
  `message` / `open` / `save` 三个命令**，且 ACL 里没有任何权限指向 `confirm`（`allow-confirm`
  在 2.7.2 里是 `allow-message` 的别名）。于是 `window.confirm` 必然抛
  `dialog.confirm not allowed. Command not found`。用插件自己的
  `import { confirm } from '@tauri-apps/plugin-dialog'`（走被授权的 `message`），
  或应用内的 `<ConfirmDialog>`。
  测这类逻辑时要 `vi.mock('@tauri-apps/plugin-dialog')`，**不能** `vi.spyOn(window, 'confirm')`
  —— 那等于测了个假实现（vitest 里没有插件注入，spy 永远"成功"）。
- **API key 明文存在 `%APPDATA%/novel-style-converter/data.db`**(单机用途)。`.env` 已
  gitignore,**绝不提交真实 key**;归档 model 时 `api_key` 会被抹成空串。
