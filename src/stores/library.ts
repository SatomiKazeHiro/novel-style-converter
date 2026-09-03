import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { UploadSummary, DataAssetRow, TransformationNovelSummary } from '../ipc/types';
import {
  listUploads as ipcListUploads,
  uploadFile as ipcUploadFile,
  deleteUpload as ipcDeleteUpload,
  listDataAssets as ipcListDataAssets,
  deleteDataAsset as ipcDeleteDataAsset,
  listTransformationNovels as ipcListTransformationNovels,
  createTransformationNovel as ipcCreateTransformationNovel,
  updateTransformationNovel as ipcUpdateTransformationNovel,
  deleteTransformationNovel as ipcDeleteTransformationNovel,
} from '../ipc/commands';

export const useLibraryStore = defineStore('library', () => {
  const uploads = ref<UploadSummary[]>([]);
  const dataAssets = ref<DataAssetRow[]>([]);
  const transformationNovels = ref<TransformationNovelSummary[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const uploading = ref(false);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      const [u, d, t] = await Promise.all([
        ipcListUploads(),
        ipcListDataAssets(),
        ipcListTransformationNovels(),
      ]);
      uploads.value = u;
      dataAssets.value = d;
      transformationNovels.value = t;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function upload(input: { file_path: string; filename: string }): Promise<UploadSummary> {
    uploading.value = true;
    try {
      const result = await ipcUploadFile(input);
      await load();
      return result;
    } finally {
      uploading.value = false;
    }
  }

  async function removeUpload(id: number): Promise<void> {
    await ipcDeleteUpload(id);
    // upload 删了 → FK CASCADE 顺带清 data_assets / chapters / transformation_novels。
    // 全文 reload 比手动算交集简单可靠,避免遗漏未来新增的关联表。
    await load();
  }

  async function createTransformationNovel(input: { data_asset_id: number; title: string }): Promise<number> {
    const id = await ipcCreateTransformationNovel(input);
    await load();
    return id;
  }

  async function renameTransformationNovel(payload: { id: number; title: string }): Promise<void> {
    await ipcUpdateTransformationNovel(payload);
    await load();
  }

  async function removeTransformationNovel(id: number): Promise<void> {
    await ipcDeleteTransformationNovel(id);
    // 仅影响 transformation_chapters(chapters / data_assets 不动),本地 filter 就够。
    transformationNovels.value = transformationNovels.value.filter((n) => n.id !== id);
  }

  async function removeDataAsset(id: number): Promise<void> {
    await ipcDeleteDataAsset(id);
    // data_asset 删了 → FK CASCADE 顺带清 transformation_novels;load 同步 tn + 不变的 da 数组。
    await load();
  }

  return {
    uploads, dataAssets, transformationNovels, loading, error, uploading,
    load, upload, removeUpload,
    removeDataAsset,
    createTransformationNovel, renameTransformationNovel, removeTransformationNovel,
  };
});