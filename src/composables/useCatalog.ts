import { computed, ref, shallowRef } from 'vue';
import { catalogReadActive } from '../ipc/commands';
import type { CatalogData, CatalogModel, CatalogProvider } from '../ipc/catalog';

// 单一全局缓存 —— catalog 是只读大对象，没必要每开一次 ModelDialog 都重新拉。
const data = shallowRef<CatalogData | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
let inflight: Promise<CatalogData> | null = null;

async function load(force = false): Promise<CatalogData> {
  if (data.value && !force) return data.value;
  if (inflight && !force) return inflight;
  loading.value = true;
  error.value = null;
  inflight = (async () => {
    try {
      const json = await catalogReadActive();
      const parsed = JSON.parse(json) as CatalogData;
      data.value = parsed;
      return parsed;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
      inflight = null;
    }
  })();
  return inflight;
}

function reset(): void {
  data.value = null;
  error.value = null;
  inflight = null;
}

export function useCatalog() {
  const providerList = computed(() => {
    const d = data.value;
    if (!d) return [] as Array<{ id: string; name: string; api: string | undefined; hasApi: boolean; modelCount: number }>;
    return Object.values(d)
      .map((p) => ({
        id: p.id,
        name: p.name,
        api: p.api,
        hasApi: !!p.api,
        modelCount: Object.keys(p.models).length,
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  function getProvider(id: string): CatalogProvider | null {
    return data.value?.[id] ?? null;
  }

  function getModel(providerId: string, modelId: string): CatalogModel | null {
    return data.value?.[providerId]?.models?.[modelId] ?? null;
  }

  function modelList(providerId: string): Array<{ id: string; name: string }> {
    const p = data.value?.[providerId];
    if (!p) return [];
    return Object.values(p.models)
      .map((m) => ({ id: m.id, name: m.name }))
      .sort((a, b) => a.name.localeCompare(b.name));
  }

  return {
    data,
    loading,
    error,
    load,
    reset,
    providerList,
    getProvider,
    getModel,
    modelList,
  };
}
