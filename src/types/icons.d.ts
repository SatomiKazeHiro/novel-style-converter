// unplugin-icons 虚拟模块的类型 shim。
// `import IconUpload from '~icons/lucide/upload'` 编译时由 unplugin-icons
// 替换成具体组件,这里只告诉 TS "这是个 Vue 组件"。
declare module '~icons/*' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<{}, {}, any>;
  export default component;
}