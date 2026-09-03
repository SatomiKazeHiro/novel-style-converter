import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('../ipc/commands', () => ({
  appendChaptersToBatch: vi.fn(async (p: { batchId: number; chapterIds: number[] }) => ({
    batch_id: p.batchId,
    added_tc_ids: [100, 101],
  })),
}));

vi.mock('@tanstack/vue-query', () => ({
  useQueryClient: () => ({
    invalidateQueries: vi.fn(),
  }),
}));

import { useWorkflowsStore } from '../stores/workflows';
import { appendChaptersToBatch } from '../ipc/commands';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
});

describe('workflows store: appendChapters', () => {
  it('调用正确 IPC 入参', async () => {
    const store = useWorkflowsStore();
    await store.appendChapters({ batchId: 7, chapterIds: [10, 11] });
    expect(appendChaptersToBatch).toHaveBeenCalledWith({ batchId: 7, chapterIds: [10, 11] });
  });

  it('返回 backend result 含 batch_id 和 added_tc_ids', async () => {
    const store = useWorkflowsStore();
    const res = await store.appendChapters({ batchId: 7, chapterIds: [10] });
    expect(res.batch_id).toBe(7);
    expect(res.added_tc_ids).toEqual([100, 101]);
  });

  it('失败时错误冒泡', async () => {
    const store = useWorkflowsStore();
    (appendChaptersToBatch as Mock).mockRejectedValueOnce(new Error('仅 stopped'));
    await expect(store.appendChapters({ batchId: 7, chapterIds: [10] }))
      .rejects.toThrow('仅 stopped');
  });
});