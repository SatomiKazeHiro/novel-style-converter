import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('../ipc/commands', () => ({
  listPrompts: vi.fn(async () => ([
    { id: 1, name: 'style-prompt', kind: 'style', template: '...', is_builtin: false, archived: 0 },
  ])),
  listModels: vi.fn(async () => ([
    { id: 1, name: 'm', base_url: 'http://x', api_key: 'k', model: 'm',
    max_tokens: null, max_context: null, temperature: null,
    disable_thinking: false, concurrency: 1, archived: 0 },
  ])),
  previewFirstChapter: vi.fn(async () => ({
    content: 'LLM 输出内容', tokens_in: 100, tokens_out: 50,
  })),
  getChapter: vi.fn(async () => ({
    id: 100, data_asset_id: 1, idx: 0, title: 'ch0',
    body: '原文正文', word_count: 10, source_kind: 'original',
    source_chapter_id: null, edited_at: null,
  })),
}));

import CreateBatchDialog from '../components/CreateBatchDialog.vue';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
});

// 必须传 open: true —— Dialog 内部 v-if="open",不传的话 Dialog 根本不会渲染,
// 所有 find() 都拿不到元素。
const defaultProps = {
  open: true,
  tnId: 1,
  selectedChapterIds: [100],
  previewChapterId: 100,
};

/// `wrapper.emitted('submit')` 的元素在 vue-test-utils 里是 `unknown`,
/// 取 payload 前先断言成期望形状 —— 否则 `--noEmit` 类型检查会报 TS18046。
type SubmitPayload = {
  preview_first_chapter: {
    content: string;
    source: { kind: 'llm'; tokens_in: number | null; tokens_out: number | null } | { kind: 'manual' };
  } | null;
};

function submitPayload(dialog: ReturnType<typeof mountDialog>): SubmitPayload {
  const emitted = dialog.emitted('submit');
  expect(emitted).toBeTruthy();
  return emitted![0][0] as SubmitPayload;
}

/// 取 seed 并断言非空 —— `preview_first_chapter` 类型上是可空的,
/// 测试里凡要读它的字段都应先过这里(否则 `--noEmit` 报 TS18047)。
function submitSeed(dialog: ReturnType<typeof mountDialog>) {
  const seed = submitPayload(dialog).preview_first_chapter;
  expect(seed).not.toBeNull();
  return seed!;
}

/// 取 LLM 来源的 seed 并收窄到 llm 分支(tokens 只在该分支上存在)。
function submitLlmSeed(dialog: ReturnType<typeof mountDialog>) {
  const seed = submitSeed(dialog);
  expect(seed.source.kind).toBe('llm');
  return { ...seed, source: seed.source as Extract<typeof seed.source, { kind: 'llm' }> };
}

function mountDialog(overrides: Record<string, unknown> = {}) {
  return mount(CreateBatchDialog, {
    props: { ...defaultProps, ...overrides },
    attachTo: document.body,
    // CreateBatchDialog 顶层包了 ui/Dialog.vue,后者用 Teleport 把内容送到 body,
    // vue-test-utils 2.4 在 Teleport 下 wrapper.element 变 undefined,find() 拿不到。
    // stub Dialog 后 Teleport 不再触发,slot 内容直接渲染在 wrapper 里,find() 正常。
    global: {
      stubs: {
        Dialog: {
          template: '<div class="dialog-stub"><slot /><slot name="footer" /></div>',
        },
      },
    },
  });
}

async function fillRequired(dialog: ReturnType<typeof mountDialog>) {
  // 等待 dialog 打开时的 listPrompts/listModels 完成
  await flushPromises();
  // setValue 用字符串 —— happy-dom + Vue v-model 用 _value (number) 匹配,
  // vue-test-utils 2.4 setValue(number) 不触发 v-model 更新;setValue('1') 正常。
  await dialog.find('select.prompt-select').setValue('1');
  await dialog.find('select.model-select').setValue('1');
  await dialog.find('input.label-input').setValue('test-batch');
  await flushPromises();
}

describe('CreateBatchDialog: 首章种子可选化 (spec 2026-09-01)', () => {
  it('默认状态：seedContent 为空、previewOutput 为空、seedSource 为 null', async () => {
    const dialog = mountDialog();
    await flushPromises();
    // vue-test-utils 2.x:.element 是 getter(属性),不是方法
    expect((dialog.find('textarea.seed-output').element as HTMLTextAreaElement).value).toBe('');
    expect((dialog.find('textarea.preview-output').element as HTMLTextAreaElement).value).toBe('');
  });

  it('提交且 seedContent 为空： payload.preview_first_chapter = null', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    // 不调 previewFirstChapter;不手写
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const payload = submitPayload(dialog);
    expect(payload.preview_first_chapter).toBeNull();
  });

  it('手写后提交： payload.preview_first_chapter.source = { kind: "manual" }', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('我手写的内容');
    await flushPromises();
    // 点击"创建"按钮
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const seed = submitSeed(dialog);
    expect(seed.content).toBe('我手写的内容');
    expect(seed.source).toEqual({ kind: 'manual' });
  });

  it('生成预览 + 复制后提交： payload.preview_first_chapter.source = { kind: "llm", tokens_in, tokens_out }', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('button.gen-preview-btn').trigger('click');
    await flushPromises();
    await dialog.find('button.copy-btn').trigger('click');
    await flushPromises();
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const seed = submitLlmSeed(dialog);
    expect(seed.source.tokens_in).toBe(100);
    expect(seed.source.tokens_out).toBe(50);
  });

  it('切换 previewChapterId： seedContent / previewOutput 被清空', async () => {
    const dialog = mountDialog();
    await flushPromises();
    // 手写一些内容
    await dialog.find('textarea.seed-output').setValue('initial');
    await flushPromises();
    // 切换 props.previewChapterId
    await dialog.setProps({ previewChapterId: 999 });
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element as HTMLTextAreaElement).value).toBe('');
  });

  it('重选 prompt / model： seedContent 不被清', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('user content');
    await flushPromises();
    // 重选 prompt(同一值,触发 change)
    await dialog.find('select.prompt-select').setValue('1');
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element as HTMLTextAreaElement).value).toBe('user content');
  });

  it('"↑ 复制"按钮在 previewOutput 为空时禁用', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    const copyBtn = dialog.find('button.copy-btn');
    expect(copyBtn.attributes('disabled')).toBeDefined();
  });

  it('"清空"按钮： seedContent=""、seedSource=null', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('to clear');
    await flushPromises();
    await dialog.find('button.clear-btn').trigger('click');
    await flushPromises();
    expect((dialog.find('textarea.seed-output').element as HTMLTextAreaElement).value).toBe('');
  });

  it('canSubmit 永真（除基础必填外）： seedContent 空也能点创建', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    const createBtn = dialog.find('button.create-btn');
    expect(createBtn.attributes('disabled')).toBeUndefined();
  });

  // 覆盖 onCopyFromPreview 的 confirm 分支（I-1）
  it('"↑ 复制"在 seedContent 非空时弹 confirm,确定=追加', async () => {
    vi.spyOn(window, 'confirm').mockReturnValueOnce(true);
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('existing content');
    await dialog.find('button.gen-preview-btn').trigger('click');
    await flushPromises();
    await dialog.find('button.copy-btn').trigger('click');
    await flushPromises();
    const seedTextarea = dialog.find('textarea.seed-output').element as HTMLTextAreaElement;
    expect(seedTextarea.value).toBe('existing content\n\nLLM 输出内容');
  });

  it('"↑ 复制"在 seedContent 非空时弹 confirm,取消=替换', async () => {
    vi.spyOn(window, 'confirm').mockReturnValueOnce(false);
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('existing content');
    await dialog.find('button.gen-preview-btn').trigger('click');
    await flushPromises();
    await dialog.find('button.copy-btn').trigger('click');
    await flushPromises();
    const seedTextarea = dialog.find('textarea.seed-output').element as HTMLTextAreaElement;
    expect(seedTextarea.value).toBe('LLM 输出内容');
    // 提交后 source 应该是 LLM
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const seed = submitSeed(dialog);
    expect(seed.source).toEqual({ kind: 'llm', tokens_in: 100, tokens_out: 50 });
  });

  // 覆盖 seed-source-hint 的 v-if 分支（I-2）
  it('seed-source-hint: LLM 路径显示消耗 tokens 信息', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('button.gen-preview-btn').trigger('click');
    await flushPromises();
    await dialog.find('button.copy-btn').trigger('click');
    await flushPromises();
    expect(dialog.find('.seed-source-hint').text()).toContain('100');
    expect(dialog.find('.seed-source-hint').text()).toContain('50');
  });

  it('seed-source-hint: 手写路径显示不消耗 tokens', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('hand written');
    await flushPromises();
    expect(dialog.find('.seed-source-hint').text()).toContain('不消耗');
  });

  // provider 不返回 usage 时(tokens 为 null),预览仍应可用,UI 显示 "—" 而不是 "null"。
  // 回归点:后端 ChatResponse.tokens_* 改成 Option 后,前端若直接渲染会显示 "null"。
  it('provider 未返回 usage:tokens 为 null 时仍可复制,显示 "—"', async () => {
    const { previewFirstChapter } = await import('../ipc/commands');
    (previewFirstChapter as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      content: 'LLM 输出内容', tokens_in: null, tokens_out: null,
    });
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('button.gen-preview-btn').trigger('click');
    await flushPromises();
    await dialog.find('button.copy-btn').trigger('click');
    await flushPromises();
    // seed 仍应建立,且 tokens 为 null(而不是 0,也不是 undefined)
    const hint = dialog.find('.seed-source-hint').text();
    expect(hint).toContain('—');
    expect(hint).not.toContain('null');
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const seed = submitSeed(dialog);
    expect(seed.source).toEqual({
      kind: 'llm', tokens_in: null, tokens_out: null,
    });
  });

  // 预览区结构（h3.section-title + .label-box 行）—— 「原文」标签从表头挪进第一行 label-box，
  // 并新增预览/转换结果的字数计数。回归点：模板重构后这些节点必须仍在，且标签不与
  // 右侧元信息/按钮错位。
  it('预览区结构：section-title + 三个 label-box 行', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    const titles = dialog.findAll('h3.section-title').map((n) => n.text());
    expect(titles).toContain('预览章节');
    const boxes = dialog.findAll('.label-box');
    expect(boxes.length).toBe(3);
    for (const box of boxes) {
      expect(box.find('.preview-label').exists()).toBe(true);
    }
    // 「原文」元信息现在在第一行 label-box 内（旧结构里在 .preview-header）
    expect(boxes[0].text()).toContain('原文');
    expect(dialog.find('.preview-header').exists()).toBe(false);
  });

  it('字数计数：预览输出与转换结果各有自己的计数', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    // 生成预览 → 预览行出现字数
    await dialog.find('button.gen-preview-btn').trigger('click');
    await flushPromises();
    const boxes = dialog.findAll('.label-box');
    expect(boxes[1].text()).toContain(`${'LLM 输出内容'.length} 字`);
    // 从预览复制 → 转换结果行出现字数
    await dialog.find('button.copy-btn').trigger('click');
    await flushPromises();
    expect(dialog.findAll('.label-box')[2].text()).toContain(`${'LLM 输出内容'.length} 字`);
  });

  it('按钮文案不再带符号前缀（↑ / ⚙ 已移除）', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    expect(dialog.find('button.copy-btn').text()).toBe('从预览复制');
    expect(dialog.find('button.create-btn').text()).toBe('创建');
  });
});