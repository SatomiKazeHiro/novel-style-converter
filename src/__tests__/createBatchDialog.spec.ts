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
    const emitted = dialog.emitted('submit');
    expect(emitted).toBeTruthy();
    expect(emitted![0][0].preview_first_chapter).toBeNull();
  });

  it('手写后提交： payload.preview_first_chapter.source = { kind: "manual" }', async () => {
    const dialog = mountDialog();
    await fillRequired(dialog);
    await dialog.find('textarea.seed-output').setValue('我手写的内容');
    await flushPromises();
    // 点击"创建"按钮
    await dialog.find('button.create-btn').trigger('click');
    await flushPromises();
    const payload = dialog.emitted('submit')![0][0];
    expect(payload.preview_first_chapter.content).toBe('我手写的内容');
    expect(payload.preview_first_chapter.source).toEqual({ kind: 'manual' });
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
    const payload = dialog.emitted('submit')![0][0];
    expect(payload.preview_first_chapter.source.kind).toBe('llm');
    expect(payload.preview_first_chapter.source.tokens_in).toBe(100);
    expect(payload.preview_first_chapter.source.tokens_out).toBe(50);
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
    const payload = dialog.emitted('submit')![0][0];
    expect(payload.preview_first_chapter.source).toEqual({ kind: 'llm', tokens_in: 100, tokens_out: 50 });
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
});