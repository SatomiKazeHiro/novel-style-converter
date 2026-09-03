// RED: 新设计 (literary/catalog) 的 AppendChaptersDialog 测试。
// 当前 src/components/AppendChaptersDialog.vue 还是 stacked label/value 形态 ——
// 还没有 eyebrow / title / config 单行 mono / status-strip 三段 / data-role 这些
// 钩子,所有 test 应当 fail。
//
// 等 GREEN agent 把 Dialog 改成文学/册页风格视觉后,本套测试通过即可。
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { ref } from 'vue';

import AppendChaptersDialog from '../components/AppendChaptersDialog.vue';

// 数据样例:383 章小说已处理过 1 个续工作流 batch(已包含第 1 章)。
// 任务 contract 要求后续 source 全是 non_empty_result_count = 0(可再补充)。
const SOURCES = [
  { chapter_id: 50, idx: 1, title: '第一章 开篇', word_count: 2304, non_empty_result_count: 1 },
  { chapter_id: 51, idx: 2, title: '第二章 试炼', word_count: 2166, non_empty_result_count: 0 },
  { chapter_id: 52, idx: 3, title: '第三章 遇仙', word_count: 2412, non_empty_result_count: 0 },
  { chapter_id: 53, idx: 4, title: '第四章 抉择', word_count: 1980, non_empty_result_count: 0 },
];

const BATCH_TCS = [
  // 第 1 章已在 batch(章 50) → 该行 disabled
  { tc_id: 1, chapter_id: 50, chapter_idx: 1, chapter_title: '第一章 开篇', status: 'completed', error: null, content_preview: '', is_empty_slot: false },
];

// ─── Mocks ────────────────────────────────────────────────────────────────

vi.mock('../ipc/commands', () => ({
  listTransformationSourceChapters: vi.fn(async () => SOURCES),
  listWorkflowChapters: vi.fn(async () => BATCH_TCS),
  appendChaptersToBatch: vi.fn(async () => ({ batch_id: 1, added_tc_ids: [99] })),
  // 以下导出 AppendChaptersDialog 没有用到 —— 仍要求 stub,免得别处万一导入即崩。
  createWorkflow: vi.fn(),
  listWorkflows: vi.fn(),
  getWorkflow: vi.fn(),
  stopWorkflow: vi.fn(),
  retryWorkflowChapters: vi.fn(),
  deleteWorkflow: vi.fn(),
  promoteWorkflow: vi.fn(),
  listDataAssetsByUpload: vi.fn(),
}));

// 按 queryKey 同步喂数据 —— useQuery 的 data 是 Ref,我们直接 ref(SOURCES) 让 row /
// total / in-batch 这些数值断言在 mount() 同步生效,GREEN 阶段不用到处 flushPromises。
function syncUseQuery({ queryKey, queryFn }: any) {
  const data = ref<unknown[] | undefined>(undefined);
  if (Array.isArray(queryKey)) {
    const head = String(queryKey[0] ?? '');
    if (head === 'transformationSourceChapters') data.value = SOURCES;
    else if (head === 'workflowChapters') data.value = BATCH_TCS;
    else data.value = [];
  } else {
    data.value = [];
  }
  void queryFn; // 不再回退到 queryFn —— 同步 mock 已经覆盖两个已知 key。
  return {
    data,
    isLoading: ref(false),
    error: ref(null),
    refetch: vi.fn(),
  };
}

vi.mock('@tanstack/vue-query', () => ({
  useQuery: (opts: any) => syncUseQuery(opts),
  // 任何其它 vue-query 导出都不应该被调用;explicit stub 让调用方 throw。
  useQueryClient: () => ({ invalidateQueries: vi.fn() }),
}));

// stub 一下 dialog-stack 里的 nextStack,避免 happy-dom 下副作用。
vi.mock('../components/ui/dialog-stack', () => ({ nextStack: () => 1000 }));

// ─── Helper ──────────────────────────────────────────────────────────────

function mountDialog(extraProps: Record<string, unknown> = {}) {
  return mount(AppendChaptersDialog, {
    props: {
      open: true,
      batchId: 1,
      transformationNovelId: 7,
      promptName: 'compress_default',
      modelDisplayName: 'MiniMax-M3',
      mode: 'compress',
      ctxPrevOriginal: 1,
      ctxPrevTransformed: 1,
      ctxNextOriginal: 1,
      ...extraProps,
    },
    attachTo: document.body,
    global: {
      stubs: {},
    },
  });
}

// ─── Tests ───────────────────────────────────────────────────────────────

describe('AppendChaptersDialog: literary / catalog redesign (RED)', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    document.body.innerHTML = '';
  });

  it('renders dialog root with serif theme class', () => {
    const wrapper = mountDialog();
    const root = wrapper.find('[data-role="dialog-root"]');
    expect(root.exists()).toBe(true);
    expect(root.classes().join(' ')).toMatch(/dialog-literary|literary|catalog/);
  });

  it('renders the eyebrow with workflow id and model display name in tiny mono', () => {
    const wrapper = mountDialog();
    const eyebrow = wrapper.find('[data-role="eyebrow"]');
    expect(eyebrow.exists()).toBe(true);
    // eyebrow 文本要含 "续工作流" + 工作流 #1 + 模型名,且样式上是个 mono 小元素
    expect(eyebrow.text()).toContain('续工作流');
    expect(eyebrow.text()).toContain('#1');
    expect(eyebrow.text()).toContain('MiniMax-M3');
    // className 里应该有 mono 提示(由 agent 决定具体类名,可能是 mono / eyebrow / small-caps)
    const className = eyebrow.classes().join(' ');
    expect(className).toMatch(/mono|eyebrow|small-cap|small/);
  });

  it('renders the serif title "补充第 2 章起 · 续作"', () => {
    const wrapper = mountDialog();
    const title = wrapper.find('[data-role="title"]');
    expect(title.exists()).toBe(true);
    expect(title.classes().join(' ')).toMatch(/title|serif/);
    // 内容形态:补充第 {第一个可补充章节的 idx} 章起 · 续作
    // 本 fixture 里章 50(idx=1)已在 batch → 第一个可补充的是 idx=2(chapter_id=51)。
    // 标题必须用 idx(用户视角的"第几章")而不是 chapter_id(DB 主键),否则真实
    // 数据下会显示成"补充第 4021 章起"。原 RED 断言写的 51 是 chapter_id,已修正。
    expect(title.text()).toMatch(/补充第\s*2\s*章起/);
  });

  it('renders configuration as a single mono line with · separators', () => {
    const wrapper = mountDialog();
    const config = wrapper.find('[data-role="config"]');
    expect(config.exists()).toBe(true);
    // 不能再是 stacked <div class="ctx-row"> 标签/值表 —— 必须单行
    const txt = config.text();
    expect(txt).toContain('compress');
    expect(txt).toContain('前文原文');
    expect(txt).toContain('前文转换');
    expect(txt).toContain('后文原文');
    // · 分隔符必须出现至少 3 次(把 4 段串起来)
    const dotCount = (txt.match(/·/g) ?? []).length;
    expect(dotCount).toBeGreaterThanOrEqual(3);
    // 内部不应再有 ctx-row 之类的 label/value 行
    expect(config.find('[class*="ctx-row"], [class*="row-label"]').exists()).toBe(false);
  });

  it('renders status strip with three sections: selected / total / in-batch', () => {
    const wrapper = mountDialog();
    expect(wrapper.find('[data-role="status-selected"]').exists()).toBe(true);
    expect(wrapper.find('[data-role="status-total"]').exists()).toBe(true);
    expect(wrapper.find('[data-role="status-inbatch"]').exists()).toBe(true);

    // total = 3 (SOURCES.length - 1 in batch = 3);in-batch = 1
    const total = wrapper.find('[data-role="status-total"]');
    expect(total.text()).toMatch(/3/);
    const inbatch = wrapper.find('[data-role="status-inbatch"]');
    expect(inbatch.text()).toMatch(/1/);
    expect(inbatch.text()).toMatch(/batch/);
  });

  it('renders each chapter row with a big serif chapter number', () => {
    const wrapper = mountDialog();
    const rows = wrapper.findAll('[data-role="chapter-row"]');
    expect(rows.length).toBe(SOURCES.length);

    const firstNum = rows[0].find('[data-role="chapter-num"]');
    expect(firstNum.exists()).toBe(true);
    expect(firstNum.text().replace(/\s+/g, '')).toMatch(/^#?1$/);
    // 该数字应当是大号 serif / mono 元素
    expect(firstNum.classes().join(' ')).toMatch(/num|number|chapter-num/);

    // idx=4 那一行也要能被识别
    const lastNum = rows[3].find('[data-role="chapter-num"]');
    expect(lastNum.text().replace(/\s+/g, '')).toMatch(/^#?4$/);
  });

  it('marks already-in-batch rows as disabled with muted style + badge', () => {
    const wrapper = mountDialog();
    const rows = wrapper.findAll('[data-role="chapter-row"]');
    // chapter_id=50 / idx=1 在 BATCH_TCS 里 → 第 0 行应为 disabled
    const disabledRow = rows[0];
    const ds = disabledRow.attributes('data-in-batch');
    const cls = disabledRow.classes().join(' ');
    expect(ds === 'true' || /disabled|in-batch|muted/.test(cls)).toBe(true);

    // 整 dialog 文本里要有 "已在 batch" (英文 badge)
    expect(wrapper.text()).toMatch(/已在\s*batch/);
    // badge 元素存在 + 在 disabledRow 内部
    const badge = disabledRow.find('[data-role="in-batch-badge"]');
    expect(badge.exists()).toBe(true);
    expect(badge.text()).toMatch(/已在\s*batch/);
  });

  it('confirm button is disabled when 0 chapters selected', () => {
    const wrapper = mountDialog();
    const btn = wrapper.find('[data-role="confirm-btn"]');
    expect(btn.exists()).toBe(true);
    // 默认 0 选 → button disabled 一定有
    expect(btn.attributes('disabled')).toBeDefined();
  });

  it('confirm button label reflects current selection count (default 0)', () => {
    const wrapper = mountDialog();
    const btn = wrapper.find('[data-role="confirm-btn"]');
    expect(btn.exists()).toBe(true);
    expect(btn.text()).toMatch(/补充\s*0\s*章/);
  });

  it('range toolbar exists above the list and is compact', () => {
    const wrapper = mountDialog();
    const toolbar = wrapper.find('[data-role="range-toolbar"]');
    expect(toolbar.exists()).toBe(true);
  });

  it('selected chapter row gets a vermillion left border style class', () => {
    const wrapper = mountDialog();
    // 模拟点击第一个可补充行(idx=2 = chapter_id=51)
    const rows = wrapper.findAll('[data-role="chapter-row"]');
    // 第一行(idx=1) 是 in-batch → 直接拿第二行(idx=2)
    const target = rows[1];
    const checkbox = target.find('input[type="checkbox"]');
    void checkbox.setValue(true);
    return wrapper.vm.$nextTick().then(() => {
      const cls = target.classes().join(' ');
      expect(cls).toMatch(/selected|is-selected|vermillion/);
    });
  });
});
