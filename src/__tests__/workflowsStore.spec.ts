// TDD RED: promote() 当前只 invalidate ['workflows'],缺 ['dataAssets'],
// 导致 Library 数据资产 tab 在转正后看不到新产物 (bug A:转正后产物不可见)。
// 见 src/stores/workflows.ts:65-69。GREEN 阶段再修。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

// vi.hoisted 让 mock factory 与测试用例共享同一个 invalidateQueries 实例,
// 便于断言它被调用时收到的 queryKey 参数。
const { invalidateQueries } = vi.hoisted(() => ({
  invalidateQueries: vi.fn(),
}));

vi.mock('../ipc/commands', () => ({
  // 只测 promote,只需 mock 它;其余 store 入口不触发就不需要 stub。
  promoteWorkflow: vi.fn(async (input: { batchId: number; title: string }) => ({
    id: 99,
    title: input.title,
    upload_id: 1,
    parsed_at: '2026-08-26T00:00:00Z',
    source_filename: 't.txt',
    kind: 'Promoted',
    note: '',
  })),
}));

vi.mock('@tanstack/vue-query', () => ({
  useQueryClient: () => ({
    invalidateQueries,
  }),
}));

import { useWorkflowsStore } from '../stores/workflows';

beforeEach(() => {
  setActivePinia(createPinia());
  invalidateQueries.mockClear();
});

describe('workflows store: promote invalidation', () => {
  it('promote 成功后同时失效 ["workflows"] 与 ["dataAssets"]', async () => {
    const store = useWorkflowsStore();
    await store.promote(1, 'My Promoted Asset');

    // RED: 当前实现只 invalidate ['workflows'],缺 ['dataAssets']。
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ['workflows'] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ['dataAssets'] });
  });

  it('promote 返回 backend 派生的新 DataAsset (regression guard)', async () => {
    const store = useWorkflowsStore();
    const newDa = await store.promote(1, 'My Promoted Asset');
    expect(newDa).toEqual(expect.objectContaining({ id: 99, title: 'My Promoted Asset' }));
  });
});
