# 死代码审计报告

- **日期**: 2026-08-28
- **审计者**: Claude (基于实际验证,不做臆测)
- **HEAD**: `c7befaa` (分支 `codex/remove-workflow`)
- **范围**: 抽样审计(未穷尽),聚焦未跟踪文件、重复定义、过期文档
- **约束**: 本报告仅做标记,不做修改。用户明确要求"禁止动手修改"

---

## 摘要

| # | 类型 | 风险 | 建议动作 |
|---|---|---|---|
| 1 | 9 个未跟踪调试文件 + 2 张截图 | 低 | 用户手动删除 |
| 2 | `dataAsset.ts` 本地 `countWords` 与 `utils/format.ts` 重复 | 中 | 用户决定去留;改动涉及前端 word count 一致性 |
| 3 | 旧 spec 引用已删除的 `crates/nsc-desktop/` 目录 | 低 | 用户可选清理 |
| 4 | `docs/superpowers/plans/2026-08-25-...md` 仍提及已移除的 `first_line_title` | 低 | 用户可选更新(或者作为历史保留) |
| 5 | 11 份 spec/plan 文档可能与现状不同步 | 低 | 用户按需复查 |

---

## 1. 未跟踪文件清单

来源:`git status --porcelain`(实际输出 11 行 untracked)。

按用途分类:

| 类别 | 数量 | 描述 |
|---|---|---|
| 调试脚本 | 7 | 仓库根的开发期残留(JS / Python / Node / PowerShell 各一,JS 调试放 `src/__tests__/`)——内容仅对生成当时的一次性调试有用,长期保留无价值 |
| 用户截图 | 2 | `docs/` 下 1280×900、仓库根 1280×720,标记问题用——已记录在 issue,长期保留无价值 |
| 有效 vitest spec | 2 | `status-locale.spec.ts`(15 case) + `workflowsStore.spec.ts`(2 case),**尚未纳入 git 跟踪**但内容是有效测试,不应清理。 |

**澄清**:
- 上述 9 个调试/截图文件(7 + 2)是开发期残留,长期保留无价值。
- 另外 2 个 `__tests__/spec.ts` **虽然 `git status` 显示 `??`(未跟踪)**,但内容是有效 vitest 测试,不应被当作"调试文件"清理。

**建议动作**:用户手动删除这 9 个低价值文件。报告仅做标记,不替用户执行。

(本节不列具体文件名——避免与 §8 配套的「防御性 vitest spec」产生删除矛盾:如果未来 commit 清理了这 9 个文件,spec 里的 snapshot 仍以「数量=9」snapshot,不需要更新文件名清单。)

---

## 2. 重复的 `countWords` 定义

`src/stores/dataAsset.ts:191` 在文件末尾定义了局部 `countWords(text: string): number`:

```ts
function countWords(text: string): number {
  return text.replace(/\s/g, '').length;
}
```

同函数在 `src/utils/format.ts:45` 已存在:

```ts
export function countWords(s: string): number {
  if (s.length === 0) return 0;
  let n = 0;
  for (const c of s) {
    if (!/\s/.test(c)) n += 1;
  }
  return n;
}
```

两份实现对同一输入产生相同结果(都等价于"字符数 - 空白字符数";JS `/\s/` 涵盖 ASCII 空白 + NBSP 等 Unicode 空白,两边语义一致)。

调用情况:
- `format.ts` 版本被 `TransformationNovelDetail.vue:32` 导入使用。
- `dataAsset.ts` 版本仅在 `saveEdit`(本文件 line 165)内部调用,**未导出**,不与 util 版本冲突,但形成事实重复。
- `dataAsset.ts` 顶部**没有** import `countWords`——纯靠本地 shadow。

**澄清**:用户提示的路径 `crates/nsc-core/src/stores/dataAsset.ts:191` 在仓库中不存在。实际位置是 `src/stores/dataAsset.ts:191`(前端 TS 文件,非 Rust crate)。

**风险**:中。前端 word count 必须与后端 `nsc-core` 的 `text::word::count` 严格一致,否则 saveEdit 后 UI 字数与 DB 字数会分歧。重复定义增加了未来分叉的风险。

**建议动作**:统一到 `utils/format.ts:45`,删除 `dataAsset.ts:191` 的局部定义。改动面小,但需确认后端 word count 公式(已与 `nsc-core` 一致)。

---

## 3. `crates/nsc-desktop/` 目录

- 实际状态:**目录不存在**(`ls crates/nsc-desktop` → `No such file or directory`)。
- 实际存在的 `nsc-desktop` 包名在 `src-tauri/Cargo.toml:2`(`name = "nsc-desktop"`),这是 `src-tauri/` 的 package 名,**不是** 一个独立目录。
- 仓库根 `Cargo.toml:2` 当前 workspace = `["crates/nsc-core", "src-tauri"]`,不包含 `crates/nsc-desktop`。

旧文档中残留的 `crates/nsc-desktop` 提及位置(均过时):

| 文档 | 行号 |
|---|---|
| `docs/superpowers/plans/2026-07-31-upload-refactor.md` | 586, 695, 1042 |
| `docs/superpowers/plans/2026-08-03-prompt-management-ui.md` | 320 |
| `docs/superpowers/plans/2026-08-03-transform-workflow.md` | 2691 |
| `docs/superpowers/plans/2026-08-14-regenerate-preview.md` | 115, 135 |

**建议动作**:用户可选清理;但既然 `src-tauri` 的 package name 仍是 `nsc-desktop`,旧文档的"用 `cargo build -p nsc-desktop`" 指令其实仍可工作(只是路径应该是 `src-tauri/` 而非 `crates/nsc-desktop/`)。不建议自动批量修改——文档已属于历史归档。

---

## 4. 过期 spec: `first_line_title`

`docs/superpowers/plans/2026-08-25-chapter-title-line-unification.md:148` 写:

> 新增辅助函数(放在 `first_line_title` 附近):

实际状态:
- `first_line_title` 在 commit `235b6db`(2026-08-25)已被移除。
- `split_first_line` 在 commit `235b6db` 同期引入,定义在 `crates/nsc-core/src/splitter/rules.rs:59`。
- 旧 doc 描述的"放在 `first_line_title` 附近"已无对应函数。

**建议动作**:用户可选更新 line 148 的措辞(把 `first_line_title` 改成 `split_first_line`),或者把整份 doc 视为历史归档。

---

## 5. Spec/Plan 文档可能与现状不同步

`docs/superpowers/specs/`(11 份)和 `docs/superpowers/plans/`(11 份)共 22 份历史文档,均为 2026-07-31 至 2026-08-26 期间产出。
- 用户已声明"改过代码"——多数 spec/plan 与最终实现可能存在小差异(命名、字段、边界)。
- 这些文档不属于可执行 ground truth,只是设计意图的快照。
- **本次审计不深入比对**——属于另一类工作(对照 spec 与代码 diff)。

**建议动作**:用户按需抽检;不在本次报告范围内。

---

## 6. 已验证的"非死代码"(防止过度兜底)

按 `CLAUDE.md` "避免兜底"——以下项目已验证**不是**死代码,不应被改动:

| 项目 | 调用方 | 结论 |
|---|---|---|
| `src/utils/format.ts` 全 6 导出 | `Library.vue` / `parse.vue` / `Upload.vue` / `AiCalls.vue` / `TransformationNovelDetail.vue` / `DataAsset.vue` / `CatalogUpdateDialog.vue` / `AiCallDetail.vue` / `UploadNode.vue` 等 | 活跃 |
| `src/utils/status-locale.ts` 全 2 导出 | `TransformationNovelDetail.vue` / `BatchNode.vue` / `status-locale.spec.ts` | 活跃 |
| `src/utils/prompt-locale.ts` 的 `formatPromptKind` | `Prompts.vue` / `PromptEditDialog` / `AppendChaptersDialog` / `CreateBatchDialog` / `TransformDialog` / `PromptViewDialog` / `prompt-locale.spec.ts` | 活跃 |
| `src/utils/splitChapters.ts` 的 `stripInvisibles` / `stripTrailingInvisibles` | `stores/chapters.ts` | 活跃 |
| `src/utils/splitChapters.ts` 的 `isVisuallyEmptyLine` | `composables/useParseEditor.ts` | 活跃 |
| 8 个 pinia stores (`theme` / `prompts` / `models` / `library` / `dataAsset` / `transformView` / `chapters` / `workflows`) | 全部被 views 导入使用 | 活跃 |
| 4 个 composables (`useTooltip` / `useCatalog` / `useDynamicTableHeight` / `useParseEditor`) | 各自有导入方 | 活跃 |
| `crates/nsc-core::db::repo::batch::count_by_status` | `src-tauri/src/commands/transformation_novels.rs:57` | 1 caller,非死代码 |

---

## 7. 范围声明

- 本报告**不是**穷尽审计;只覆盖被显式标记的"潜在死代码"样本。
- 未覆盖:rust 代码内 dead code(需要 `cargo clippy` + 跨 crate 引用图)、未引用的 CSS class、Vue 组件 prop 字段、Tauri command 末端未被前端调用的部分。
- 报告不做修改,所有"建议动作"需由用户复核后手工执行。

---

## 8. 配套产物

- `src/__tests__/dead_code_invariants.spec.ts`:防御性 vitest spec,通过 named import 与快照守卫上述关键产物。
  - 测试不强制删除未跟踪文件,仅快照当前状态。
  - named import 写法让"未来 commit 误删 `formatSize` / `countWords` / 任一 composable" 在测试运行时立刻编译失败。