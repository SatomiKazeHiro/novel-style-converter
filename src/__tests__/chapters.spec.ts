import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';

const TEXT = "content intro\n\n第一章：开篇\nbody1 line 1\nbody1 line 2\nbody1 line 3\n第二章今世只想生孩子\nbody2 line 1\nbody2 line 2\nbody2 line 3\n第三章：误会\nbody3 line 1\nbody3 line 2";

vi.mock('../ipc/commands', () => ({
  getUploadText: vi.fn(async () => TEXT),
  getUpload: vi.fn(async (id) => ({ id, filename: 'sample.txt', size: TEXT.length })),
  findDataAssetByUpload: vi.fn(async () => []),
  listChapterSegments: vi.fn(async () => SEGMENTS),
  listCommittedSegments: vi.fn(async () => []),
  commitDataAsset: vi.fn(async () => 1),
}));

import { useChaptersStore } from '../stores/chapters';
import * as ipcCommands from '../ipc/commands';
const { commitDataAsset } = ipcCommands;

// title_line = 标题行 0-based 行号(见 TEXT)。
const SEGMENTS = [
  { title: '第一章：开篇', content: 'body1 line 1\nbody1 line 2\nbody1 line 3', word_count: 6, title_line: 2 },
  { title: '第二章今世只想生孩子', content: 'body2 line 1\nbody2 line 2\nbody2 line 3', word_count: 6, title_line: 6 },
  { title: '第三章：误会', content: 'body3 line 1\nbody3 line 2', word_count: 6, title_line: 10 },
];

beforeEach(() => { setActivePinia(createPinia()); });
afterEach(() => { vi.clearAllMocks(); });

describe('chapters store: 栈化 chapterSplits', () => {
  it('load 用 title_line 初始化 chapterSplits 与 titles', async () => {
    const store = useChaptersStore();
    await store.load(1);
    expect([...store.chapterSplits].map(Number).sort((a,b)=>a-b)).toEqual([2, 6, 10]);
    expect(store.titles.get('6')).toBe('第二章今世只想生孩子');
    expect(store.workingChapters.map((c) => c.title)).toEqual(['第一章：开篇', '第二章今世只想生孩子', '第三章：误会']);
    expect(store.dirty).toBe(false);
  });

  it('toggleChapterSplit 删标题行 → 该章并入上一章', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('6');
    expect(store.workingChapters.map((c) => c.title)).toEqual(['第一章：开篇', '第三章：误会']);
    expect(store.workingChapters[0].content).toContain('body2 line 1');
    expect(store.dirty).toBe(true);
  });

  it('toggleChapterSplit 加 body 行 → 切出新章', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('4');
    expect(store.workingChapters.length).toBe(4);
    expect(store.workingChapters[1].title).toBe('body1 line 2');
  });

  it('同一行 toggle 两次净变化 0 + 标题恢复', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('6');
    store.toggleChapterSplit('6');
    expect(store.workingChapters.map((c) => c.title)).toEqual(['第一章：开篇', '第二章今世只想生孩子', '第三章：误会']);
    expect(store.dirty).toBe(false);
  });

  it('updateTitle 改标题只改 title 不改 content', async () => {
    const store = useChaptersStore();
    await store.load(1);
    const before = store.workingChapters[1].content;
    store.updateTitle('6', '楔子');
    expect(store.workingChapters[1].title).toBe('楔子');
    expect(store.workingChapters[1].content).toBe(before);
    expect(store.dirty).toBe(true);
  });

  it('reset 恢复 initialChapterSplits + initialTitles', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.toggleChapterSplit('4');
    store.updateTitle('6', '楔子');
    store.reset();
    expect([...store.chapterSplits].map(Number).sort((a,b)=>a-b)).toEqual([2, 6, 10]);
    expect(store.titles.get('6')).toBe('第二章今世只想生孩子');
    expect(store.dirty).toBe(false);
  });

  it('updateTitle + toggle-off + toggle-on preserves user title edit', async () => {
    const store = useChaptersStore();
    await store.load(1);
    store.updateTitle('6', '楔子');
    expect(store.workingChapters[1].title).toBe('楔子');
    store.toggleChapterSplit('6');
    store.toggleChapterSplit('6');
    expect(store.workingChapters[1].title).toBe('楔子');
    expect(store.dirty).toBe(true);
  });

  it('toggleChapterSplit 越界抛错', async () => {
    const store = useChaptersStore();
    await store.load(1);
    expect(() => store.toggleChapterSplit('999999')).toThrow(/越界/);
  });

  it('recomputeInitialFromSegs title_line 越界抛错', async () => {
    const store = useChaptersStore();
    await store.load(1);
    expect(() => store.recomputeInitialFromSegs([
      { title: 'bad', content: '', word_count: 0, title_line: 999999 },
    ])).toThrow(/越界/);
  });

  it('commit 传 {title_line, title, content} 形状', async () => {
    const store = useChaptersStore();
    await store.load(1);
    await store.commit('My Novel');
    expect(commitDataAsset).toHaveBeenCalledWith(1, {
      title: 'My Novel',
      chapters: [
        { title: '第一章：开篇', content: expect.any(String), title_line: 2 },
        { title: '第二章今世只想生孩子', content: expect.any(String), title_line: 6 },
        { title: '第三章：误会', content: expect.any(String), title_line: 10 },
      ],
    });
  });
});