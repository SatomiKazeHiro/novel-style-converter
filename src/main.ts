import { createApp } from 'vue';
import { createPinia } from 'pinia';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';
import App from './App.vue';
import router from './router';
import './base.css';

/// TanStack Query Client —— 全局异步数据缓存/失效/loading 三态的统一来源。
/// - retry:false: 后端返回 error 就是 error,不盲目重试掩盖问题。
/// - staleTime:5s: 同一 query key 5s 内的重复 mount 不重拉,详情页来回切不抖。
/// - refetchOnWindowFocus:false: Tauri 窗口聚焦不触发重拉,避免后端意外负载。
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      staleTime: 5000,
      refetchOnWindowFocus: false,
    },
  },
});

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(VueQueryPlugin, { queryClient });
app.mount('#app');
