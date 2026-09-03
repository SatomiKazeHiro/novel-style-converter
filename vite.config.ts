/// <reference types="vitest" />
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import Icons from 'unplugin-icons/vite';

export default defineConfig({
  plugins: [
    vue(),
    Icons({
      compiler: 'vue3',
      // 只打包 lucide,其它图标集不引进来。
      collections: ['lucide'],
    }),
  ],
  clearScreen: false,
  server: {
    // WebView2 在 Windows 上把 'localhost' 解析为 IPv4 127.0.0.1,容易和 Vite 监听
    // [::1](IPv6) 错位,造成 ERR_CONNECTION_REFUSED。固定 127.0.0.1 走 IPv4 最稳。
    host: '127.0.0.1',
    port: 43801,
    strictPort: true,
    watch: {
      // Vite 默认递归扫描整个项目根,而 cargo build 产物在 target/doc/ 里
      // 会生成几千个 rustdoc HTML,导致依赖扫描炸掉。明确排除。
      ignored: ['**/target/**', '**/crates/**', '**/src-tauri/**', '**/migrations/**', '**/dist/**'],
    },
    fs: {
      // 限制 Vite 文件系统访问只到 src/ 与 index.html,防止它误扫 target/。
      allow: ['.'],
    },
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
  },
  optimizeDeps: {
    entries: ['index.html'],
    include: ['vue', 'vue-router', 'pinia', '@tauri-apps/api'],
  },
  test: {
    include: ['src/**/*.{test,spec}.ts'],
    exclude: ['node_modules', 'dist'],
    environment: 'happy-dom',
  },
});
