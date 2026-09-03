import { defineStore } from 'pinia';
import { ref, watch } from 'vue';

export type Theme = 'light' | 'dark';

const STORAGE_KEY = 'nsc.theme';

function read_initial(): Theme {
  if (typeof window === 'undefined') return 'light';
  const v = window.localStorage.getItem(STORAGE_KEY);
  return v === 'dark' ? 'dark' : 'light';
}

export const useThemeStore = defineStore('theme', () => {
  const theme = ref<Theme>(read_initial());

  watch(theme, (v) => {
    if (typeof window === 'undefined') return;
    window.localStorage.setItem(STORAGE_KEY, v);
    document.documentElement.dataset.theme = v;
  }, { immediate: true });

  function toggle() {
    theme.value = theme.value === 'light' ? 'dark' : 'light';
  }

  return { theme, toggle };
});