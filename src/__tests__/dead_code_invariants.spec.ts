/// 死代码防御性守卫。配套 `docs/2026-08-28-dead-code-audit.md`.
///
/// 工作机制:
/// 1. **named import**: 不写 `import * as format from '...'`;只写 named import。
///    一旦未来 commit 误删 `formatSize` / `countWords` / 任一 store / composable,
///    import 行 TS 编译即失败,测试运行时立刻可见。
/// 2. **typeof 断言**: 防御运行时把导出替换成 `undefined` / 占位对象。
/// 3. **路径断言**: `crates/nsc-desktop` 不存在 — 防止它被误恢复。
/// 4. **未跟踪文件快照**: 仅记录"当前有",不强制存在(用户可手动删)。
///
/// 测试不强制做任何 git 操作,不调用 `git rm`;只断言当前文件系统状态。
import { describe, it, expect } from 'vitest';
// `@types/node` 未安装;runtime 是 Node,vitest 跑在 Node 里。
// TLA + dynamic import + 局部类型断言,绕开 TS 编译错误,运行时 OK。
// @ts-expect-error -- node:fs types not in this project (no @types/node)
const _fs: typeof import('node:fs') = await import('node:fs');
const existsSync = _fs.existsSync;
const readFileSync = _fs.readFileSync;
const join = (...parts: string[]) => parts.join('/').replace(/\/+/g, '/');

// ── utils/format ── 6 个 named export ────────────────────────────────────────
import {
  formatSize,
  formatTime,
  formatTimeShort,
  formatWordCount,
  formatDate,
  countWords,
} from '../utils/format';

// ── utils/status-locale ── 2 个 named export ─────────────────────────────────
import { formatBatchStatus, formatChapterStatus } from '../utils/status-locale';

// ── utils/prompt-locale ── formatPromptKind ──────────────────────────────────
import { formatPromptKind } from '../utils/prompt-locale';

// ── utils/splitChapters ── 3 个 named export ─────────────────────────────────
import {
  stripInvisibles,
  stripTrailingInvisibles,
  isVisuallyEmptyLine,
} from '../utils/splitChapters';

// ── 8 个 pinia stores ───────────────────────────────────────────────────────
import { useThemeStore } from '../stores/theme';
import { usePromptsStore } from '../stores/prompts';
import { useModelsStore } from '../stores/models';
import { useLibraryStore } from '../stores/library';
import { useDataAssetStore } from '../stores/dataAsset';
import { useTransformViewStore } from '../stores/transformView';
import { useChaptersStore } from '../stores/chapters';
import { useWorkflowsStore } from '../stores/workflows';

// ── 4 个 composables ────────────────────────────────────────────────────────
import { useTooltip } from '../composables/useTooltip';
import { useCatalog } from '../composables/useCatalog';
import { useDynamicTableHeight } from '../composables/useDynamicTableHeight';
import { useParseEditor } from '../composables/useParseEditor';

// ROOT: `pnpm test` / `pnpm exec vitest` 都从仓库根启动 vitest,
// 所以 cwd 就是仓库根。`import.meta.url` 在 vitest+happy-dom 里
// 不是 file:// scheme,不能用 fileURLToPath,所以走 cwd。
// @ts-expect-error -- `process` 是 Node 全局,不在配置的 TS lib 里
const ROOT = process.cwd();

describe('dead code invariants', () => {
  describe('active exports — utils/format', () => {
    it('exports 6 named functions', () => {
      expect(typeof formatSize).toBe('function');
      expect(typeof formatTime).toBe('function');
      expect(typeof formatTimeShort).toBe('function');
      expect(typeof formatWordCount).toBe('function');
      expect(typeof formatDate).toBe('function');
      expect(typeof countWords).toBe('function');
    });
  });

  describe('active exports — utils/status-locale', () => {
    it('exports 2 formatters', () => {
      expect(typeof formatBatchStatus).toBe('function');
      expect(typeof formatChapterStatus).toBe('function');
    });
  });

  describe('active exports — utils/prompt-locale', () => {
    it('exports formatPromptKind', () => {
      expect(typeof formatPromptKind).toBe('function');
    });
  });

  describe('active exports — utils/splitChapters', () => {
    it('exports 3 helpers', () => {
      expect(typeof stripInvisibles).toBe('function');
      expect(typeof stripTrailingInvisibles).toBe('function');
      expect(typeof isVisuallyEmptyLine).toBe('function');
    });
  });

  describe('active stores — 8 pinia defineStore', () => {
    it('all 8 stores are defined', () => {
      // `defineStore` 返回 store factory(可调用函数),所以 typeof === 'function'
      expect(typeof useThemeStore).toBe('function');
      expect(typeof usePromptsStore).toBe('function');
      expect(typeof useModelsStore).toBe('function');
      expect(typeof useLibraryStore).toBe('function');
      expect(typeof useDataAssetStore).toBe('function');
      expect(typeof useTransformViewStore).toBe('function');
      expect(typeof useChaptersStore).toBe('function');
      expect(typeof useWorkflowsStore).toBe('function');
    });
  });

  describe('active composables — 4 useX', () => {
    it('all 4 composables are defined', () => {
      expect(typeof useTooltip).toBe('function');
      expect(typeof useCatalog).toBe('function');
      expect(typeof useDynamicTableHeight).toBe('function');
      expect(typeof useParseEditor).toBe('function');
    });
  });

  describe('workspace layout', () => {
    it('crates/nsc-desktop does NOT exist (legacy removed)', () => {
      // 若未来 commit 误创建该目录,此测试失败 — 强制人工 review
      expect(existsSync(join(ROOT, 'crates/nsc-desktop'))).toBe(false);
    });

    it('workspace Cargo.toml members = ["crates/nsc-core", "src-tauri"]', () => {
      // 防止未来有人加 "crates/nsc-desktop" 进 workspace
      const cargo = readFileSync(join(ROOT, 'Cargo.toml'), 'utf8');
      expect(cargo).toContain('crates/nsc-core');
      expect(cargo).toContain('src-tauri');
      expect(cargo).not.toContain('crates/nsc-desktop');
    });
  });

  // 删除了 untracked debug files snapshot — 9 个低价值文件已被用户清理,
  // 该 snapshot 不再有意义;信息已在 docs/2026-08-28-dead-code-audit.md 记录。
});