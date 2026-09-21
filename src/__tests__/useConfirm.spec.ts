import { describe, it, expect, beforeEach } from 'vitest';
import { confirmDialog, alertDialog, useConfirmHost } from '../composables/useConfirm';

/// useConfirm 的契约测试。
///
/// 这套服务是全局单例：业务侧 `await confirmDialog(...)`，宿主侧 `settle(ok)`。
/// 两边通过模块级状态交接，所以**状态必须在每个用例前清干净**，否则用例会互相串味。
/// `current` 是 shallowRef，检测到有未决请求就按"取消"结束它。
describe('useConfirm', () => {
  const { current, settle } = useConfirmHost();

  beforeEach(() => {
    if (current.value) settle(false);
  });

  it('未调用时没有待决请求', () => {
    expect(current.value).toBeNull();
  });

  it('confirmDialog 把请求交给宿主，并在 settle(true) 后 resolve 为 true', async () => {
    const p = confirmDialog({ title: 'T', message: 'M', confirmText: '删', kind: 'danger' });
    expect(current.value).toMatchObject({ title: 'T', message: 'M', confirmText: '删', kind: 'danger' });

    settle(true);
    await expect(p).resolves.toBe(true);
    expect(current.value).toBeNull();
  });

  it('settle(false)（取消 / 关闭 / 遮罩）resolve 为 false', async () => {
    const p = confirmDialog({ title: 'T', message: 'M' });
    settle(false);
    await expect(p).resolves.toBe(false);
  });

  it('alertDialog 只给一个按钮：alertOnly 为 true 且 cancelText 为「知道了」', () => {
    void alertDialog({ title: '提示', message: '太大了' });
    expect(current.value).toMatchObject({ alertOnly: true, cancelText: '知道了' });
    settle(true);
  });

  it('前一个未决请求被新请求顶掉时，按"取消"结束，不会永久悬空', async () => {
    const first = confirmDialog({ title: '第一个', message: 'M' });
    const second = confirmDialog({ title: '第二个', message: 'M' });

    await expect(first).resolves.toBe(false); // 被顶掉 → 视为取消
    expect(current.value?.title).toBe('第二个');

    settle(true);
    await expect(second).resolves.toBe(true);
  });
});
