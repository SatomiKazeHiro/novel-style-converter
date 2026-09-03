import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Prompt, PromptInput } from '../ipc/types';
import {
  countPromptUsage as ipcCountPromptUsage,
  deletePrompt as ipcDeletePrompt,
  listPrompts as ipcListPrompts,
  listPromptsIncludingArchived as ipcListPromptsIncludingArchived,
  restorePrompt as ipcRestorePrompt,
  upsertPrompt as ipcUpsertPrompt,
} from '../ipc/commands';

/// 写后 reload 比 diff 简单,prompt 表典型 < 20 条,成本可忽略。
export const usePromptsStore = defineStore('prompts', () => {
  const prompts = ref<Prompt[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  /// 默认隐藏已归档 —— list 行为与后端 list_prompts 对齐(archived=0 过滤)。
  /// 切到 true 时 store 仍提供 prompts(全量),Prompts.vue 自己决定渲染哪些行。
  const includeArchived = ref(false);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      prompts.value = includeArchived.value
        ? await ipcListPromptsIncludingArchived()
        : await ipcListPrompts();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  function setIncludeArchived(v: boolean) {
    if (includeArchived.value === v) return;
    includeArchived.value = v;
    void load();
  }

  async function upsert(payload: PromptInput): Promise<number> {
    const id = await ipcUpsertPrompt(payload);
    await load();
    return id;
  }

  async function remove(id: number): Promise<void> {
    await ipcDeletePrompt(id);
    await load();
  }

  async function restore(id: number): Promise<void> {
    await ipcRestorePrompt(id);
    await load();
  }

  /// 引用计数 —— 删除 prompt 前弹 confirm 用。返回 Promise<number>,让调用方 await。
  async function countUsage(id: number): Promise<number> {
    return ipcCountPromptUsage(id);
  }

  return { prompts, loading, error, includeArchived, load, upsert, remove, restore, countUsage, setIncludeArchived };
});
