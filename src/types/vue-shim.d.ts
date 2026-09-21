// Vue SFC 的类型 shim。
//
// 为什么需要:项目里大量 `import Foo from './Foo.vue'`,但 Vite 与 vitest 用
// esbuild/vue-plugin 处理 `.vue`,**TypeScript 本身不认这个扩展名**。没有本文件时
// `tsc --noEmit` 会对每个 `.vue` 导入报 TS2307「Cannot find module」,导致前端
// **完全没有可用的类型检查门禁** —— CLAUDE.md 自己列的高危项(IPC camelCase /
// snake_case 翻译错了不会被 vitest mock 抓到)恰恰只能靠类型检查兜住。
//
// 与 `types/icons.d.ts`(unplugin-icons shim)同一模式:只告诉 TS「这是个 Vue 组件」,
// 组件内部的 props 具体类型由 `<script setup>` 自己推导,不在这里重复声明。
declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<{}, {}, any>;
  export default component;
}
