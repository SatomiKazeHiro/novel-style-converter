import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { ModelConfig, ModelConfigInput, TestModelReport } from '../ipc/types';
import {
  deleteModel as ipcDeleteModel,
  listModels as ipcListModels,
  listModelsIncludingArchived as ipcListModelsIncludingArchived,
  restoreModel as ipcRestoreModel,
  testModel as ipcTestModel,
  upsertModel as ipcUpsertModel,
} from '../ipc/commands';

export const useModelsStore = defineStore('models', () => {
  const models = ref<ModelConfig[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const testing = ref(false);
  /// 顶部"显示已归档"开关。包含归档行会拉 list_models_including_archived。
  const includeArchived = ref(false);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      models.value = includeArchived.value
        ? await ipcListModelsIncludingArchived()
        : await ipcListModels();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function setIncludeArchived(v: boolean) {
    includeArchived.value = v;
    await load();
  }

  async function save(input: ModelConfigInput): Promise<number> {
    const id = await ipcUpsertModel(input);
    await load();
    return id;
  }

  /// 软删：后端把 archived=1 + api_key=''。前端从当前视图 filter 掉该行；
  /// 切到"显示已归档"视图再 load() 即可恢复。
  async function remove(id: number): Promise<void> {
    await ipcDeleteModel(id);
    models.value = models.value.filter((m) => m.id !== id);
  }

  /// 取消软删。
  async function restore(id: number): Promise<void> {
    await ipcRestoreModel(id);
    if (!includeArchived.value) {
      // 当前视图不显示归档：不刷，等用户自己切开关。
      return;
    }
    await load();
  }

  /// `test_model` 返回结构化报告。后端不再抛错 —— 错误字符串塞 `report.error`。
  /// UI 直接读 report 字段完整展示 latency / tokens / preview / error。
  async function test(input: ModelConfigInput): Promise<TestModelReport> {
    testing.value = true;
    error.value = null;
    try {
      return await ipcTestModel(input);
    } catch (e: unknown) {
      // 后端几乎不会抛错（失败走 report.error）；只防 IPC 通道异常。
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      throw new Error(msg);
    } finally {
      testing.value = false;
    }
  }

  const hasAnyModel = computed(() => models.value.length > 0);

  return { models, loading, error, testing, includeArchived, hasAnyModel, load, setIncludeArchived, save, remove, restore, test };
});
